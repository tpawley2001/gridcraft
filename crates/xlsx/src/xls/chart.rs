//! Charts in `.xls` files ([MS-XLS] 2.1.7.20.1, 2.2.3): an embedded chart is a chart substream
//! (a BOF with `dt` = 0x0020) right after its worksheet's chart Obj record (2.4.181), placed by
//! the client anchor of the drawing shape before it (MsoDrawing, 2.4.168); a chart sheet is a
//! BoundSheet8 whose substream is a chart substream. Both become model charts whose series are
//! references, so their values follow the cells.
//!
//! Read: the chart group type (Bar, Line, Pie, Area, Scatter, Radar and their stacked, 100%,
//! doughnut and bubble forms), each series' name, values, categories and bubble sizes (BRAI
//! references, or the literal values the substream caches), the chart and axis titles (Text +
//! ObjectLink + SeriesText), the legend's side, value axis bounds (ValueRange), major gridlines,
//! series colours that aren't automatic (LineFormat, AreaFormat) and value data labels
//! (AttachedLabel `fShowValue` in a whole-series DataFormat). A chart whose group type
//! has no model kind (surface) is skipped with a warning.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{
    Anchor, AnchorMode, AxisScale, Chart, ChartKind, ChartScale, ChartSheetPrint, Color, Image, LegendPos, PreservedPart, PreservedRel,
    PreservedSheet, PrintSettings, Series, Sheet, Visibility, Workbook,
};

use super::drawing::{self, BStore, Blip, ShapeProps};
use super::formula::{Links, Mode, decode};
use super::records::{Cursor, Record, Records};
use super::{BOF, Budget, EOF};
use crate::{IoError, ReadReport};

/// BOF `dt` of a chart substream.
const CHART_SUBSTREAM: u16 = 0x0020;
/// Series read per chart at most.
const MAX_SERIES: usize = 256;
/// Charts read per file at most.
const MAX_CHARTS: usize = 10_000;
/// Cached literal points kept per chart at most.
const MAX_CACHED: usize = 65_536;
/// Begin/End nesting followed at most.
const MAX_DEPTH: usize = 256;
/// OfficeArt records walked per MsoDrawing record at most.
const MAX_ART_RECORDS: usize = 100_000;

// Chart substream record types ([MS-XLS] 2.3).
const NUMBER: u16 = 0x0203;
const LABEL: u16 = 0x0204;
const CHART: u16 = 0x1002;
const SERIES: u16 = 0x1003;
const DATA_FORMAT: u16 = 0x1006;
const LINE_FORMAT: u16 = 0x1007;
const MARKER_FORMAT: u16 = 0x1009;
const AREA_FORMAT: u16 = 0x100A;
const ATTACHED_LABEL: u16 = 0x100C;
const SERIES_TEXT: u16 = 0x100D;
const CHART_FORMAT: u16 = 0x1014;
const LEGEND: u16 = 0x1015;
const BAR: u16 = 0x1017;
const LINE: u16 = 0x1018;
const PIE: u16 = 0x1019;
const AREA: u16 = 0x101A;
const SCATTER: u16 = 0x101B;
const AXIS: u16 = 0x101D;
const VALUE_RANGE: u16 = 0x101F;
const AXIS_LINE: u16 = 0x1021;
const TEXT: u16 = 0x1025;
const OBJECT_LINK: u16 = 0x1027;
const BEGIN: u16 = 0x1033;
const END: u16 = 0x1034;
const RADAR: u16 = 0x103E;
const SURF: u16 = 0x103F;
const RADAR_AREA: u16 = 0x1040;
const AXIS_PARENT: u16 = 0x1041;
const SER_TO_CRT: u16 = 0x1045;
const SER_PARENT: u16 = 0x104A;
const BRAI: u16 = 0x1051;
const BOP_POP: u16 = 0x1061;
const SI_INDEX: u16 = 0x1065;

/// Obj `ot` values ([MS-XLS] 2.5.152 FtCmo): a chart, a picture and a note.
const OT_CHART: u16 = 0x0005;
const OT_PICTURE: u16 = 0x0008;
const OT_NOTE: u16 = 0x0019;
/// OfficeArtClientAnchorSheet record type ([MS-XLS] 2.5.193).
const CLIENT_ANCHOR: u16 = 0xF010;

/// Whether a BOF record opens a chart substream.
pub(super) fn is_chart_bof(rec: &Record<'_>) -> bool {
    rec.id == BOF && rec.parts.first().and_then(|p| p.get(2..4)).is_some_and(|dt| dt == CHART_SUBSTREAM.to_le_bytes())
}

/// An OfficeArtClientAnchorSheet: the cells under a shape's corners, with offsets in 1/1024 of
/// the column width and 1/256 of the row height.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct ClientAnchor {
    col_l: u16,
    dx_l: u16,
    row_t: u16,
    dy_t: u16,
    col_r: u16,
    dx_r: u16,
    row_b: u16,
    dy_b: u16,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16_at(b, at)?) | (u32::from(u16_at(b, at.checked_add(2)?)?) << 16))
}

/// The last client anchor in one MsoDrawing record's OfficeArt records, with the properties of
/// its shape (the FOPTs since the shape's FSP): containers (`recVer` 0xF) are entered, atoms
/// skipped by their length.
fn client_anchor(data: &[u8]) -> Option<(ClientAnchor, ShapeProps)> {
    let mut pos = 0usize;
    let mut found = None;
    let mut props = ShapeProps::default();
    for _ in 0..MAX_ART_RECORDS {
        let (Some(ver), Some(ty), Some(len)) = (u16_at(data, pos), u16_at(data, pos.saturating_add(2)), u32_at(data, pos.saturating_add(4))) else {
            break;
        };
        let body = pos.saturating_add(8);
        if ver & 0x000F == 0x000F {
            pos = body;
            continue;
        }
        if ty == drawing::FSP {
            props = ShapeProps::default();
        } else if ty == drawing::FOPT || ty == drawing::TERTIARY_FOPT {
            let end = body.saturating_add(usize::try_from(len).unwrap_or(usize::MAX)).min(data.len());
            drawing::fopt(ver >> 4, data.get(body..end).unwrap_or_default(), &mut props);
        } else if ty == CLIENT_ANCHOR {
            // flags, then colL, dxL, rwT, dyT, colR, dxR, rwB, dyB.
            let f = |i: usize| u16_at(data, body.saturating_add(2 + 2 * i));
            if let (Some(col_l), Some(dx_l), Some(row_t), Some(dy_t), Some(col_r), Some(dx_r), Some(row_b), Some(dy_b)) =
                (f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7))
            {
                found = Some((ClientAnchor { col_l, dx_l, row_t, dy_t, col_r, dx_r, row_b, dy_b }, props.clone()));
            }
        }
        pos = body.saturating_add(usize::try_from(len).unwrap_or(usize::MAX));
    }
    found
}

/// A worksheet's embedded charts and pictures, collected while its substream is read.
#[derive(Default)]
pub(super) struct Embedded {
    /// The last drawing shape's anchor and properties, until an Obj record claims them.
    anchor: Option<(ClientAnchor, ShapeProps)>,
    /// The anchor of the chart Obj whose substream comes next.
    pending: Option<ClientAnchor>,
    charts: Vec<(Option<ClientAnchor>, Chart)>,
    pictures: Vec<(ClientAnchor, ShapeProps)>,
}

impl Embedded {
    /// An MsoDrawing record: remembers its shape's anchor and properties.
    pub(super) fn drawing(&mut self, rec: &Record<'_>) {
        if let Some(a) = client_anchor(&rec.parts.concat()) {
            self.anchor = Some(a);
        }
    }

    /// An Obj record: a chart or picture takes the shape's anchor; other shapes are reported.
    pub(super) fn obj(&mut self, rec: &Record<'_>, report: &mut ReadReport) {
        let ot = rec.parts.first().and_then(|p| u16_at(p, 4));
        let anchor = self.anchor.take();
        match ot {
            Some(OT_CHART) => self.pending = anchor.map(|(a, _)| a),
            Some(OT_PICTURE) => {
                if let Some(a) = anchor {
                    self.pictures.push(a);
                }
            }
            Some(OT_NOTE) => {}
            _ => report.warn("Shapes in .xls files are not imported yet."),
        }
    }

    /// A chart substream (its BOF just read): read to its EOF. Returns `false` when the stream
    /// ends inside it (cut off), so nothing more of the sheet can be read.
    pub(super) fn chart(&mut self, recs: &mut Records<'_>, budget: &mut Budget, links: &Links, report: &mut ReadReport) -> Result<bool, IoError> {
        let anchor = self.pending.take();
        match read(recs, budget, links, report)? {
            Read::Chart(ch) => self.charts.push((anchor, *ch)),
            Read::Skipped => {}
            Read::CutOff => return Ok(false),
        }
        Ok(true)
    }

    /// Places the charts and pictures on the sheet (its column widths and row heights are known
    /// by now). Pictures whose blip isn't a PNG or JPEG in the store are skipped with one warning.
    pub(super) fn finish(self, sheet: &mut Sheet, store: &BStore, report: &mut ReadReport) {
        for (anchor, mut ch) in self.charts {
            if let Some(a) = anchor {
                ch.anchor = place(sheet, &a);
            }
            sheet.charts.push(ch);
        }
        let mut skipped = false;
        for (a, props) in self.pictures {
            let Some(Blip::Picture { data, mime }) = props.pib.and_then(|pib| store.get(pib)) else {
                skipped = true;
                continue;
            };
            let anchor = place(sheet, &a);
            sheet.images.push(Image {
                hidden: props.hidden,
                hyperlink: None,
                name: props.name,
                id: 0,
                anchor,
                data: Arc::clone(data),
                mime: (*mime).to_string(),
                alt: props.alt.unwrap_or_default(),
                blip_ext: vec![],
            });
        }
        // A damaged store was reported when it was read.
        if skipped && !store.damaged {
            report.warn("Some pictures in .xls files use a format that isn't imported (only PNG and JPEG are).");
        }
    }
}

/// A client anchor in sheet units.
fn place(sheet: &Sheet, a: &ClientAnchor) -> Anchor {
    // BIFF8 has 256 columns; offsets beyond a whole cell are clamped as Excel does.
    let col = |c: u16| u32::from(c.min(255));
    let x = |c: u16, d: u16| sheet.col_left(col(c)) + f64::from(d.min(1024)) / 1024.0 * f64::from(sheet.col_width(col(c)));
    let y = |r: u16, d: u16| sheet.row_top(u32::from(r)) + f64::from(d.min(256)) / 256.0 * f64::from(sheet.row_height(u32::from(r)));
    let (x1, y1) = (x(a.col_l, a.dx_l), y(a.row_t, a.dy_t));
    let (x2, y2) = (x(a.col_r, a.dx_r), y(a.row_b, a.dy_b));
    let cell = CellRef::new(u32::from(a.row_t), col(a.col_l));
    Anchor {
        cell,
        dx: (x1 - sheet.col_left(cell.col)).max(0.0) as f32,
        dy: (y1 - sheet.row_top(cell.row)).max(0.0) as f32,
        width: (x2 - x1).max(0.0) as f32,
        height: (y2 - y1).max(0.0) as f32,
        mode: AnchorMode::MoveAndSize,
    }
}

/// Reads the chart substream of a chart sheet at `pos`. A substream that isn't a readable chart
/// gives `None` with a warning.
pub(super) fn chart_sheet(stream: &[u8], pos: usize, budget: &mut Budget, links: &Links, report: &mut ReadReport) -> Result<Option<Chart>, IoError> {
    let mut recs = Records::new(stream, pos);
    let first = match recs.next_record() {
        Ok(Some(r)) => r,
        Ok(None) | Err(IoError::Format(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    if pos >= stream.len() || !is_chart_bof(&first) {
        return Ok(None);
    }
    Ok(match read(&mut recs, budget, links, report)? {
        Read::Chart(ch) => Some(*ch),
        Read::Skipped | Read::CutOff => None,
    })
}

/// What reading a chart substream gave.
enum Read {
    Chart(Box<Chart>),
    /// Not imported (a warning says why); the substream was read to its EOF.
    Skipped,
    /// The stream ends inside the substream (a warning says so).
    CutOff,
}

/// Reads a chart substream up to its EOF (its BOF already read). A chart that can't be imported
/// is skipped with a warning; only resource limits are errors.
fn read(recs: &mut Records<'_>, budget: &mut Budget, links: &Links, report: &mut ReadReport) -> Result<Read, IoError> {
    let mut records = vec![];
    let mut depth = 1usize;
    let mut nested = false;
    loop {
        let rec = match recs.next_record() {
            Ok(Some(rec)) => rec,
            Ok(None) | Err(IoError::Format(_)) => {
                report.warn("A chart cut off by the end of the file was skipped.");
                return Ok(Read::CutOff);
            }
            Err(e) => return Err(e),
        };
        budget.record()?;
        match rec.id {
            BOF => {
                nested = true;
                depth += 1;
            }
            EOF => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ if depth == 1 => records.push(rec),
            _ => {}
        }
    }
    budget.charts = budget.charts.saturating_add(1);
    if budget.charts > MAX_CHARTS {
        report.warn(format!("Only the first {MAX_CHARTS} charts were imported."));
        return Ok(Read::Skipped);
    }
    if nested {
        report.warn("A damaged chart was skipped.");
        return Ok(Read::Skipped);
    }
    match parse(&records, links, report) {
        Ok(ch) => Ok(Read::Chart(Box::new(ch))),
        Err(Skip::Damaged) => {
            report.warn("A damaged chart was skipped.");
            Ok(Read::Skipped)
        }
        Err(Skip::Why(why)) => {
            report.warn(format!("A chart was skipped: {why}."));
            Ok(Read::Skipped)
        }
    }
}

enum Skip {
    Damaged,
    Why(String),
}

impl From<IoError> for Skip {
    fn from(_: IoError) -> Self {
        Skip::Damaged
    }
}

/// What a Begin record opens: the record before it.
#[derive(Clone, Copy, PartialEq)]
enum Block {
    Series(usize),
    Text(usize),
    Axis(u16),
    Group(usize),
    /// A DataFormat; the series it formats as a whole, if any.
    DataFormat(Option<usize>),
    AxisParent(u16),
    Other,
}

/// A chart group type record.
#[derive(Clone, Copy)]
enum GroupType {
    Bar { transpose: bool, stacked: bool, percent: bool },
    Line { stacked: bool, percent: bool },
    Pie { donut: bool },
    Area { stacked: bool, percent: bool },
    Scatter { bubbles: bool },
    Radar,
    Unsupported(&'static str),
}

struct Group {
    icrt: u16,
    ty: Option<GroupType>,
    secondary: bool,
}

#[derive(Default)]
struct Ser {
    /// Its position among the substream's Series records (the column of its cached values).
    index: usize,
    name: Option<String>,
    values: Option<String>,
    categories: Option<String>,
    bubbles: Option<String>,
    group: u16,
    /// A trendline or error bar series (SerParent).
    aux: bool,
    /// LineFormat: colour when not automatic, and whether a line is drawn.
    line: Option<(Option<Color>, bool)>,
    /// AreaFormat: solid fill colour when not automatic.
    fill: Option<Color>,
    /// MarkerFormat: whether markers are drawn.
    marker: Option<bool>,
}

#[derive(Default)]
struct TextBox {
    text: Option<String>,
    link: Option<u16>,
}

enum Lit {
    Num(f64),
    Str(String),
}

fn color(rgb: u32) -> Color {
    // LongRGB: red, green, blue, reserved.
    Color::rgb((rgb & 0xFF) as u8, ((rgb >> 8) & 0xFF) as u8, ((rgb >> 16) & 0xFF) as u8)
}

/// The state of one chart substream as its records are read.
#[derive(Default)]
struct Parser {
    /// What each open Begin block belongs to.
    stack: Vec<Block>,
    /// What the previous record would open if a Begin followed it.
    last: Option<Block>,
    series: Vec<Ser>,
    series_seen: usize,
    groups: Vec<Group>,
    texts: Vec<TextBox>,
    legend: Option<LegendPos>,
    frame: Option<(f32, f32)>,
    scale: ChartScale,
    gridlines: bool,
    /// A whole-series AttachedLabel shows values.
    value_labels: bool,
    cache_kind: u16,
    cache: HashMap<(u16, u16), Vec<(u16, Lit)>>,
    cached: usize,
    unreadable: bool,
}

impl Parser {
    fn top(&self) -> Option<Block> {
        self.stack.last().copied()
    }

    /// The `iax` of the AxisParent block the reader is in (0, the primary axes, outside any).
    fn axis_parent(&self) -> u16 {
        self.stack.iter().rev().find_map(|b| if let Block::AxisParent(i) = b { Some(*i) } else { None }).unwrap_or(0)
    }

    fn top_series(&mut self) -> Option<&mut Ser> {
        match self.top() {
            Some(Block::Series(i)) => self.series.get_mut(i),
            _ => None,
        }
    }

    /// The series whose whole-series DataFormat block the reader is in.
    fn formatted_series(&mut self) -> Option<&mut Ser> {
        match self.top() {
            Some(Block::DataFormat(Some(i))) => self.series.get_mut(i),
            _ => None,
        }
    }

    fn record(&mut self, rec: &Record<'_>, links: &Links, report: &mut ReadReport) -> Result<(), Skip> {
        let mut c = Cursor::new(&rec.parts);
        let opens = match rec.id {
            BEGIN => {
                if self.stack.len() >= MAX_DEPTH {
                    return Err(Skip::Damaged);
                }
                self.stack.push(self.last.take().unwrap_or(Block::Other));
                return Ok(());
            }
            END => {
                self.stack.pop();
                None
            }
            CHART => {
                let (_x, _y, dx, dy) = (c.u32()?, c.u32()?, c.u32()?, c.u32()?);
                // FixedPoint points → sheet units.
                let px = |v: u32| (f64::from(v) / 65_536.0 * 4.0 / 3.0) as f32;
                self.frame = Some((px(dx), px(dy)));
                None
            }
            SERIES => self.series_record(report),
            BRAI => {
                self.brai(&mut c, links)?;
                None
            }
            SERIES_TEXT => {
                self.series_text(&mut c)?;
                None
            }
            TEXT => {
                self.texts.push(TextBox::default());
                Some(Block::Text(self.texts.len() - 1))
            }
            OBJECT_LINK => {
                if let Some(Block::Text(i)) = self.top()
                    && let Some(t) = self.texts.get_mut(i)
                {
                    t.link = Some(c.u16()?);
                }
                None
            }
            SER_TO_CRT => {
                if let Some(s) = self.top_series() {
                    s.group = c.u16()?;
                }
                None
            }
            SER_PARENT => {
                if let Some(s) = self.top_series() {
                    s.aux = true;
                }
                None
            }
            DATA_FORMAT => {
                let xi = c.u16()?;
                Some(Block::DataFormat(match self.top() {
                    Some(Block::Series(i)) if xi == 0xFFFF => Some(i),
                    _ => None,
                }))
            }
            LINE_FORMAT | AREA_FORMAT | MARKER_FORMAT => {
                self.format(rec.id, &mut c)?;
                None
            }
            ATTACHED_LABEL => {
                // fShowValue ([MS-XLS] 2.4.10); point labels and a short payload are ignored.
                if self.formatted_series().is_some() && c.u16().is_ok_and(|flags| flags & 0x0001 != 0) {
                    self.value_labels = true;
                }
                None
            }
            AXIS_PARENT => Some(Block::AxisParent(c.u16()?)),
            AXIS => Some(Block::Axis(c.u16()?)),
            AXIS_LINE => {
                if self.top() == Some(Block::Axis(1)) && self.axis_parent() == 0 && c.u16()? == 1 {
                    self.gridlines = true;
                }
                None
            }
            VALUE_RANGE => {
                self.value_range(&mut c)?;
                None
            }
            CHART_FORMAT => {
                c.skip(18)?;
                let icrt = c.u16()?;
                let secondary = self.axis_parent() == 1;
                self.groups.push(Group { icrt, ty: None, secondary });
                Some(Block::Group(self.groups.len() - 1))
            }
            BAR | LINE | PIE | AREA | SCATTER | RADAR | RADAR_AREA | BOP_POP | SURF => {
                if let Some(Block::Group(g)) = self.top() {
                    let ty = group_type(rec.id, &mut c)?;
                    if let Some(gr) = self.groups.get_mut(g)
                        && gr.ty.is_none()
                    {
                        gr.ty = Some(ty);
                    }
                }
                None
            }
            LEGEND => {
                if self.legend.is_none() {
                    self.legend = Some(legend_side(&mut c)?);
                }
                None
            }
            SI_INDEX => {
                self.cache_kind = c.u16()?;
                None
            }
            NUMBER | LABEL if (1..=3).contains(&self.cache_kind) && self.cached < MAX_CACHED => {
                let (row, col) = (c.u16()?, c.u16()?);
                c.u16()?;
                let v = if rec.id == NUMBER { Lit::Num(c.f64()?) } else { Lit::Str(c.string()?) };
                self.cache.entry((self.cache_kind, col)).or_default().push((row, v));
                self.cached += 1;
                None
            }
            _ => None,
        };
        self.last = opens;
        Ok(())
    }

    fn series_record(&mut self, report: &mut ReadReport) -> Option<Block> {
        let index = self.series_seen;
        self.series_seen += 1;
        if self.series.len() < MAX_SERIES {
            self.series.push(Ser { index, ..Ser::default() });
            return Some(Block::Series(self.series.len() - 1));
        }
        if index == MAX_SERIES {
            report.warn(format!("Only the first {MAX_SERIES} series of a chart were imported."));
        }
        None
    }

    /// BRAI: a series' name, values, categories or bubble sizes as a reference.
    fn brai(&mut self, c: &mut Cursor<'_, '_>, links: &Links) -> Result<(), Skip> {
        if self.top_series().is_none() {
            return Ok(());
        }
        let id = c.u8()?;
        let rt = c.u8()?;
        c.skip(4)?;
        let cce = usize::from(c.u16()?);
        if rt != 2 || cce == 0 {
            return Ok(());
        }
        if cce > c.remaining() {
            return Err(Skip::Damaged);
        }
        let rgce = c.rest();
        let rgce = rgce.get(..cce).unwrap_or(&rgce);
        let Ok(e) = decode(rgce, &[], CellRef::new(0, 0), Mode::Cell, links) else {
            self.unreadable = true;
            return Ok(());
        };
        let text = gridcraft_formula::print(&e);
        if let Some(s) = self.top_series() {
            match id {
                0 => s.name = Some(text),
                1 => s.values = Some(text),
                2 => s.categories = Some(text),
                3 => s.bubbles = Some(text),
                _ => {}
            }
        }
        Ok(())
    }

    /// SeriesText: a literal series name, or a title's text.
    fn series_text(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), Skip> {
        c.u16()?;
        let text = c.short_string()?;
        match self.top() {
            Some(Block::Series(i)) => {
                if let Some(s) = self.series.get_mut(i)
                    && s.name.is_none()
                {
                    s.name = Some(text);
                }
            }
            Some(Block::Text(i)) => {
                if let Some(t) = self.texts.get_mut(i) {
                    t.text = Some(text);
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// LineFormat, AreaFormat and MarkerFormat of a whole series.
    fn format(&mut self, id: u16, c: &mut Cursor<'_, '_>) -> Result<(), Skip> {
        let Some(s) = self.formatted_series() else { return Ok(()) };
        match id {
            LINE_FORMAT => {
                let rgb = c.u32()?;
                let lns = c.u16()?;
                c.u16()?;
                let auto = c.u16()? & 0x0001 != 0;
                s.line = Some(((!auto && lns != 5).then(|| color(rgb)), lns != 5));
            }
            AREA_FORMAT => {
                let fore = c.u32()?;
                c.u32()?;
                let fls = c.u16()?;
                let auto = c.u16()? & 0x0001 != 0;
                if !auto && fls == 1 {
                    s.fill = Some(color(fore));
                }
            }
            _ => {
                c.u32()?;
                c.u32()?;
                let imk = c.u16()?;
                let auto = c.u16()? & 0x0001 != 0;
                s.marker = Some(auto || imk != 0);
            }
        }
        Ok(())
    }

    /// ValueRange of a primary axis: its fixed bounds and units.
    fn value_range(&mut self, c: &mut Cursor<'_, '_>) -> Result<(), Skip> {
        let Some(Block::Axis(w)) = self.top() else { return Ok(()) };
        if w >= 2 || self.axis_parent() != 0 {
            return Ok(());
        }
        let nums = [c.f64()?, c.f64()?, c.f64()?, c.f64()?, c.f64()?];
        let flags = c.u16()?;
        let on = |bit: u16| flags & (1 << bit) != 0;
        // fAutoMin, fAutoMax, fAutoMajor, fAutoMinor are bits 0-3; fLog 5; fReversed 6.
        let num = |i: usize| nums.get(i).copied().filter(|v| v.is_finite() && !on(i as u16));
        let mut s = AxisScale { reverse: on(6), ..AxisScale::default() };
        if on(5) {
            s.log_base = Some(10.0);
        } else {
            s.min = num(0);
            s.max = num(1);
            s.major_unit = num(2).filter(|v| *v > 0.0);
            s.minor_unit = num(3).filter(|v| *v > 0.0);
            if let (Some(a), Some(b)) = (s.min, s.max)
                && a >= b
            {
                s.min = None;
                s.max = None;
            }
        }
        if w == 1 {
            self.scale.y = s;
        } else {
            self.scale.x = s;
        }
        Ok(())
    }

    /// A series' cached literal points of `kind` (1 values, 2 categories, 3 bubble sizes) as an
    /// array constant.
    fn literal(&self, kind: u16, s: &Ser) -> Option<String> {
        let mut pts: Vec<&(u16, Lit)> = self.cache.get(&(kind, u16::try_from(s.index).ok()?))?.iter().collect();
        pts.sort_by_key(|(r, _)| *r);
        let items: Vec<String> = pts
            .iter()
            .map(|(_, v)| match v {
                Lit::Num(n) => format!("{n}"),
                Lit::Str(t) => format!("\"{}\"", t.replace('"', "\"\"")),
            })
            .collect();
        (!items.is_empty()).then(|| format!("{{{}}}", items.join(",")))
    }

    fn group_of(&self, s: &Ser) -> usize {
        self.groups.iter().position(|g| g.icrt == s.group).unwrap_or(0)
    }

    fn series_out(&self, s: &Ser, kinds: &[ChartKind], combo: bool) -> Option<Series> {
        let values = s.values.clone().or_else(|| self.literal(1, s))?;
        let gi = self.group_of(s);
        let kind = kinds.get(gi).copied().unwrap_or_default();
        let line_like = matches!(
            kind,
            ChartKind::Line | ChartKind::LineMarkers | ChartKind::LineStacked | ChartKind::Scatter | ChartKind::ScatterLines | ChartKind::Radar
        );
        let line_color = s.line.and_then(|(c, _)| c);
        let color = if line_like { line_color.or(s.fill) } else { s.fill.or(line_color) };
        Some(Series {
            name: s.name.clone(),
            categories: s.categories.clone().or_else(|| self.literal(2, s)),
            values,
            bubble_sizes: s.bubbles.clone().or_else(|| self.literal(3, s)),
            color,
            secondary: combo && self.groups.get(gi).is_some_and(|g| g.secondary),
            kind: combo.then_some(kind),
            ..Series::default()
        })
    }

    fn finish(mut self, report: &mut ReadReport) -> Result<Chart, Skip> {
        if self.groups.is_empty() {
            return Err(Skip::Why("it has no chart type".into()));
        }
        if let Some(name) = self.groups.iter().find_map(|g| if let Some(GroupType::Unsupported(n)) = g.ty { Some(n) } else { None }) {
            return Err(Skip::Why(format!("{name} charts can't be imported yet")));
        }
        let data: Vec<&Ser> = self.series.iter().filter(|s| !s.aux).collect();
        let kinds: Vec<ChartKind> = (0..self.groups.len())
            .map(|gi| {
                let members: Vec<&Ser> = data.iter().copied().filter(|s| self.group_of(s) == gi).collect();
                self.groups.get(gi).map(|g| kind_of(g, &members)).unwrap_or_default()
            })
            .collect();
        let mut used: Vec<usize> = data.iter().map(|s| self.group_of(s)).collect();
        used.sort_unstable();
        used.dedup();
        let combo = used.len() > 1;
        let out: Vec<Series> = data.iter().filter_map(|s| self.series_out(s, &kinds, combo)).collect();
        if self.unreadable {
            report.warn("Some chart series refer to data in a way that can't be imported; those references were left out.");
        }
        if out.is_empty() {
            return Err(Skip::Why("it has no data series that can be imported".into()));
        }
        let text = |link: u16| self.texts.iter().find(|t| t.link == Some(link)).and_then(|t| t.text.clone()).filter(|t| !t.is_empty());
        let kind = if combo { ChartKind::Combo } else { used.first().and_then(|g| kinds.get(*g)).copied().unwrap_or_default() };
        let (width, height) = self.frame.filter(|(w, h)| *w > 0.0 && *h > 0.0).unwrap_or((480.0, 288.0));
        if !matches!(kind, ChartKind::Scatter | ChartKind::ScatterLines | ChartKind::Bubble) {
            self.scale.x = AxisScale::default();
        }
        Ok(Chart {
            hidden: false,
            id: 0,
            kind,
            anchor: Anchor { width, height, ..Anchor::default() },
            title: text(1),
            series: out,
            legend: self.legend.unwrap_or(LegendPos::None),
            data_labels: self.value_labels,
            gridlines: self.gridlines,
            style: 2,
            x_title: text(3),
            y_title: text(2),
            source: None,
            by_rows: false,
            pivot: None,
            scale: self.scale,
            data_table: None,
        })
    }
}

/// A chart group type record (Bar, Line, Pie, Area, Scatter, Radar, RadarArea, BopPop, Surf).
fn group_type(id: u16, c: &mut Cursor<'_, '_>) -> Result<GroupType, Skip> {
    Ok(match id {
        BAR => {
            c.u32()?;
            let f = c.u16()?;
            GroupType::Bar { transpose: f & 1 != 0, stacked: f & 2 != 0, percent: f & 4 != 0 }
        }
        LINE | AREA => {
            let f = c.u16()?;
            let (stacked, percent) = (f & 1 != 0, f & 2 != 0);
            if id == LINE { GroupType::Line { stacked, percent } } else { GroupType::Area { stacked, percent } }
        }
        PIE => {
            c.u16()?;
            GroupType::Pie { donut: c.u16()? != 0 }
        }
        SCATTER => {
            c.u32()?;
            GroupType::Scatter { bubbles: c.u16()? & 1 != 0 }
        }
        // Pie of pie and bar of pie: drawn as a pie.
        BOP_POP => GroupType::Pie { donut: false },
        SURF => GroupType::Unsupported("surface"),
        _ => GroupType::Radar,
    })
}

/// Legend: the side its centre is nearest, along the way its entries run. Position and size are
/// in 1/4000 of the chart area.
fn legend_side(c: &mut Cursor<'_, '_>) -> Result<LegendPos, Skip> {
    let (x, y, dx, dy) = (c.u32()?, c.u32()?, c.u32()?, c.u32()?);
    c.u16()?;
    let vertical = c.u16()? & 0x0010 != 0;
    let (cx, cy) = (u64::from(x) + u64::from(dx) / 2, u64::from(y) + u64::from(dy) / 2);
    Ok(match (vertical, cx >= 2000, cy >= 2000) {
        (true, true, _) => LegendPos::Right,
        (true, false, _) => LegendPos::Left,
        (false, _, true) => LegendPos::Bottom,
        (false, _, false) => LegendPos::Top,
    })
}

/// The model kind of a chart group drawing `members`.
fn kind_of(g: &Group, members: &[&Ser]) -> ChartKind {
    match g.ty {
        Some(GroupType::Bar { transpose, stacked, percent }) => match (transpose, percent, stacked) {
            (true, true, _) => ChartKind::BarStacked100,
            (true, false, true) => ChartKind::BarStacked,
            (true, false, false) => ChartKind::BarClustered,
            (false, true, _) => ChartKind::ColumnStacked100,
            (false, false, true) => ChartKind::ColumnStacked,
            (false, false, false) => ChartKind::ColumnClustered,
        },
        Some(GroupType::Line { stacked, percent }) if stacked || percent => ChartKind::LineStacked,
        Some(GroupType::Line { .. }) if members.iter().any(|s| s.marker == Some(true)) => ChartKind::LineMarkers,
        Some(GroupType::Line { .. }) => ChartKind::Line,
        Some(GroupType::Area { percent: true, .. }) => ChartKind::AreaStacked100,
        Some(GroupType::Area { stacked: true, .. }) => ChartKind::AreaStacked,
        Some(GroupType::Area { .. }) => ChartKind::Area,
        Some(GroupType::Pie { donut: true }) => ChartKind::Doughnut,
        Some(GroupType::Pie { donut: false }) => ChartKind::Pie,
        Some(GroupType::Scatter { bubbles: true }) => ChartKind::Bubble,
        Some(GroupType::Scatter { .. }) if members.iter().any(|s| s.line.is_some_and(|(_, drawn)| drawn)) => ChartKind::ScatterLines,
        Some(GroupType::Scatter { .. }) => ChartKind::Scatter,
        Some(GroupType::Radar) => ChartKind::Radar,
        Some(GroupType::Unsupported(_)) | None => ChartKind::ColumnClustered,
    }
}

fn parse(records: &[Record<'_>], links: &Links, report: &mut ReadReport) -> Result<Chart, Skip> {
    let mut p = Parser::default();
    for rec in records {
        p.record(rec, links, report)?;
    }
    p.finish(report)
}

/// A chart sheet read from the file: its name, state, how many worksheets come before it, and
/// its chart.
pub(crate) struct ChartSheet {
    pub name: String,
    pub visibility: Visibility,
    pub before: usize,
    pub chart: Chart,
}

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const EMU_PER_PX: f64 = 9525.0;

/// Adds the chart sheets to the workbook: a printable copy of each, and the chart sheet, drawing
/// and chart parts that saving as `.xlsx` writes back in place.
pub(crate) fn add_chart_sheets(wb: &mut Workbook, sheets: Vec<ChartSheet>) {
    if sheets.is_empty() {
        return;
    }
    let mut kept = wb.preserved.as_deref().cloned().unwrap_or_default();
    let mut prints = Vec::with_capacity(sheets.len());
    for (i, cs) in sheets.into_iter().enumerate() {
        let n = i + 1;
        let sheet_part = format!("xl/chartsheets/sheet{n}.xml");
        let drawing_part = format!("xl/drawings/chartsheet_drawing{n}.xml");
        let chart_part = format!("xl/charts/chartsheet_chart{n}.xml");
        let chart_xml = crate::chart::write_chart(wb, 0, &cs.chart, "");
        let emu = |v: f32| (f64::from(v.max(1.0)) * EMU_PER_PX).round() as i64;
        let (cx, cy) = (emu(cs.chart.anchor.width), emu(cs.chart.anchor.height));
        let mut drawing = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<xdr:wsDr xmlns:xdr=\"http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">",
        );
        let _ = write!(
            drawing,
            "<xdr:absoluteAnchor><xdr:pos x=\"0\" y=\"0\"/><xdr:ext cx=\"{cx}\" cy=\"{cy}\"/><xdr:graphicFrame macro=\"\"><xdr:nvGraphicFramePr><xdr:cNvPr id=\"2\" name=\"Chart 1\"/><xdr:cNvGraphicFramePr><a:graphicFrameLocks noGrp=\"1\"/></xdr:cNvGraphicFramePr></xdr:nvGraphicFramePr><xdr:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></xdr:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"{REL}\" r:id=\"rId1\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:absoluteAnchor></xdr:wsDr>"
        );
        let sheet_xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<chartsheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"{REL}\"><sheetPr/><sheetViews><sheetView workbookViewId=\"0\"/></sheetViews><pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/><drawing r:id=\"rId1\"/></chartsheet>"
        );
        let rel =
            |ty: &str, target: &str| PreservedRel { id: "rId1".into(), rel_type: format!("{REL}/{ty}"), target: target.into(), external: false };
        kept.parts.push(PreservedPart {
            name: sheet_part.clone(),
            content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.chartsheet+xml".into(),
            data: Arc::from(sheet_xml.into_bytes()),
            rels: vec![rel("drawing", &drawing_part)],
        });
        kept.parts.push(PreservedPart {
            name: drawing_part,
            content_type: "application/vnd.openxmlformats-officedocument.drawing+xml".into(),
            data: Arc::from(drawing.into_bytes()),
            rels: vec![rel("chart", &chart_part)],
        });
        kept.parts.push(PreservedPart {
            name: chart_part,
            content_type: "application/vnd.openxmlformats-officedocument.drawingml.chart+xml".into(),
            data: Arc::from(chart_xml.into_bytes()),
            rels: vec![],
        });
        let rel_id = format!("rIdXlsChartSheet{n}");
        kept.rels.push(PreservedRel { id: rel_id.clone(), rel_type: format!("{REL}/chartsheet"), target: sheet_part, external: false });
        kept.sheets.push(PreservedSheet { name: cs.name.clone(), visibility: cs.visibility, rel_id, before: cs.before, sheet_id: 0 });
        prints.push(ChartSheetPrint {
            name: cs.name,
            visibility: cs.visibility,
            before: cs.before,
            chart: cs.chart,
            print: PrintSettings::default(),
        });
    }
    wb.preserved = Some(Arc::new(kept));
    wb.chart_sheets = Arc::new(prints);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_anchor_is_found_inside_containers_and_bounded() {
        // SpContainer (recVer 0xF) holding a ClientAnchor atom.
        let mut atom = vec![0x00, 0x00, 0x10, 0xF0, 18, 0, 0, 0];
        for v in [0u16, 1, 0, 2, 0, 5, 0, 12, 0] {
            atom.extend_from_slice(&v.to_le_bytes());
        }
        let mut data = vec![0x0F, 0x00, 0x04, 0xF0];
        data.extend_from_slice(&(atom.len() as u32).to_le_bytes());
        data.extend_from_slice(&atom);
        let (a, _) = client_anchor(&data).unwrap();
        assert_eq!((a.col_l, a.row_t, a.col_r, a.row_b), (1, 2, 5, 12));
        // A truncated atom and a huge length are ignored, not read past.
        assert_eq!(client_anchor(&data[..data.len() - 3]), None);
        let mut huge = vec![0x00, 0x00, 0x0B, 0xF0];
        huge.extend_from_slice(&u32::MAX.to_le_bytes());
        huge.extend_from_slice(&data);
        assert_eq!(client_anchor(&huge), None);
    }
}
