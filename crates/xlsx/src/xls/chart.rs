//! Charts in `.xls` files ([MS-XLS] 2.1.7.20.1, 2.2.3): an embedded chart is a chart substream
//! (a BOF with `dt` = 0x0020) right after its worksheet's chart Obj record (2.4.181), placed by
//! the client anchor of the drawing shape before it (MsoDrawing, 2.4.168); a chart sheet is a
//! BoundSheet8 whose substream is a chart substream. Both become model charts whose series are
//! references, so their values follow the cells.
//!
//! Read: the chart group type (Bar, Line, Pie, Area, Scatter, Radar, RadarArea and their stacked,
//! 100%, doughnut, bubble and filled forms), pie/doughnut first slice angles and doughnut hole sizes (Pie),
//! each series' name, values, categories and bubble sizes (BRAI
//! references, or the literal values the substream caches), the chart and axis titles (Text +
//! ObjectLink + SeriesText), the legend's side, value axis bounds (ValueRange), major gridlines,
//! series colours that aren't automatic (LineFormat, AreaFormat) and value data labels
//! (AttachedLabel `fShowValue` in a whole-series DataFormat). A chart whose group type
//! has no model kind (surface) is skipped with a warning.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_core::color::INDEXED;
use gridcraft_formula::{Expr, RefKind, Reference, SheetSel};
use gridcraft_model::{
    Anchor, AnchorMode, AxisScale, Chart, ChartKind, ChartScale, ChartSheetPrint, Color, ControlKind, FormControl, GeometryPath, Image, LegendPos,
    ListSelection, Orientation, PathCommand, PreservedPart, PreservedRel, PreservedSheet, PrintSettings, Series, Shape, ShapeFormat, ShapeGeometry,
    ShapeKind, Sheet, Visibility, Workbook,
};

use super::drawing::{self, BStore, Blip, ShapeProps};
use super::formula::{Biff5, Extern, ExternKind, Links, Mode, decode_version};
use super::png;
use super::records::{Cursor, Record, Records};
use super::textbox::{self, Text};
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
/// EXTERNSHEET entries read per BIFF5 chart at most.
const MAX_EXTERNS: usize = 4096;
/// Begin/End nesting followed at most.
const MAX_DEPTH: usize = 256;
/// OfficeArt records walked per MsoDrawing record at most.
const MAX_ART_RECORDS: usize = 100_000;

// Chart substream record types ([MS-XLS] 2.3).
const EXTERN_SHEET: u16 = 0x0017;
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

/// Obj `ot` values ([MS-XLS] 2.5.152 FtCmo): a chart, a text box, a picture and a note.
const OT_CHART: u16 = 0x0005;
const OT_TEXT_BOX: u16 = 0x0006;
const OT_PICTURE: u16 = 0x0008;
const OT_NOTE: u16 = 0x0019;
/// A line, rectangle, oval and Office Art object (the drawn shapes that are imported).
const OT_LINE: u16 = 0x0001;
const OT_RECTANGLE: u16 = 0x0002;
const OT_OVAL: u16 = 0x0003;
const OT_OFFICE_ART: u16 = 0x001E;
/// A BIFF5 polygon: its points are the COORDLIST record right after its Obj.
const OT_POLYGON: u16 = 0x0009;
/// A BIFF5 polygon's `wPolyFlags` bit for a closed outline.
const POLY_CLOSED: u16 = 0x0100;
/// COORDLIST: a BIFF5 polygon's points, in 1/16384 of its box.
pub(super) const COORDLIST: u16 = 0x00A9;
/// The coordinate space of a COORDLIST point.
const POLY_UNIT: i64 = 0x4000;
/// Form controls: button, check box, option button, edit box, label, spinner, scroll bar, list,
/// group box and drop-down (an AutoFilter arrow is a drop-down too).
const OT_FORM_CONTROLS: [u16; 10] = [0x0007, 0x000B, 0x000C, 0x000D, 0x000E, 0x0010, 0x0011, 0x0012, 0x0013, 0x0014];
/// The form controls imported ([MS-XLS] 2.5.143 FtCmo `ot`): button, check box, option button,
/// spin button and scroll bar.
const OT_BUTTON: u16 = 0x0007;
const OT_CHECK_BOX: u16 = 0x000B;
const OT_OPTION_BUTTON: u16 = 0x000C;
const OT_SPIN: u16 = 0x0010;
const OT_SCROLL_BAR: u16 = 0x0011;
const OT_LABEL: u16 = 0x000E;
const OT_LIST: u16 = 0x0012;
const OT_GROUP_BOX: u16 = 0x0013;
const OT_DROP_DOWN: u16 = 0x0014;
/// FtCmo `fPrint` ([MS-XLS] 2.5.143).
const F_PRINT: u16 = 0x0010;
/// FtCmo `fUIObj` ([MS-XLS] 2.5.143): an object the application created itself, such as the
/// drop-down arrow of an AutoFilter or of a list validation. It is not a user form control.
const F_UI_OBJ: u16 = 0x0100;
/// FtCmo `fLocked` ([MS-XLS] 2.5.143).
const F_LOCKED: u16 = 0x0001;
/// A group Obj's `ot`.
const OT_GROUP: u16 = 0x0000;
/// The Office Art shape type of a text box (msosptTextBox, [MS-ODRAW] 2.4.24).
const SPT_TEXT_BOX: u16 = 202;
/// OfficeArtClientAnchorSheet record type ([MS-XLS] 2.5.193).
const CLIENT_ANCHOR: u16 = 0xF010;
/// OfficeArt record types ([MS-ODRAW] 2.2): a group's container, a shape's container, a group's
/// frame and a grouped shape's anchor.
const SPGR_CONTAINER: u16 = 0xF003;
const SP_CONTAINER: u16 = 0xF004;
const FSPGR: u16 = 0xF009;
const CHILD_ANCHOR: u16 = 0xF00F;

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

/// A complete OfficeArt header and payload distinguish drawing data after an Obj from a
/// genuine continuation of the Obj's subrecords ([MS-XLS] 2.1.7.20, [MS-ODRAW] 2.2.1).
fn starts_office_art(data: &[u8]) -> bool {
    let (Some(ty), Some(len)) = (u16_at(data, 2), u32_at(data, 4)) else { return false };
    (0xF000..=0xF122).contains(&ty) && usize::try_from(len).is_ok_and(|len| len <= data.len().saturating_sub(8))
}

/// Where a drawn object goes: a client anchor (cells under its corners), or a part of a group's
/// rectangle (the outermost group's client anchor, and the fractions of its width and height the
/// shape's left, top, width and height are: its child anchors mapped through each group frame).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Cells(ClientAnchor),
    Group(ClientAnchor, [f64; 4]),
}

/// What one MsoDrawing record's last anchor belongs to.
#[derive(Debug, PartialEq)]
enum Found {
    /// A shape, with its place (none: a child anchor outside a placed group) and properties.
    Shape(Option<Place>, ShapeProps),
    /// A group's own shape: its Obj (a group) imports nothing.
    Group,
}

/// An OfficeArtSpgrContainer being walked ([MS-ODRAW] 2.2.16): its first SpContainer is the
/// group's own shape (frame and anchor), the others its children.
struct ArtGroup {
    /// Drawing-stream offset where the container ends.
    end: u64,
    /// Where the group's own shape's SpContainer ends, once it has started.
    shape_end: Option<u64>,
    /// The group's coordinate frame (OfficeArtFSPGR, [MS-ODRAW] 2.2.38): left, top, right, bottom.
    frame: Option<[i32; 4]>,
    /// The group's rectangle, once its anchor has been read (none for the drawing's root group).
    place: Option<(ClientAnchor, [f64; 4])>,
}

/// A grouped shape's position and size in points at most.
const MAX_POINTS: f64 = 1.0e7;
/// Group nesting followed at most; deeper groups' shapes aren't placed.
const MAX_GROUP_DEPTH: usize = 16;

/// The walk through a sheet's drawing data: the concatenation of its MsoDrawing records
/// ([MS-XLS] 2.4.168), with Obj and TxO records in between. Groups span several MsoDrawing
/// records, so their ends are tracked by byte counts.
#[derive(Default)]
struct ArtWalk {
    /// Drawing-stream offset of the next MsoDrawing record's first byte.
    offset: u64,
    groups: Vec<ArtGroup>,
    /// The end of a group nested too deeply: until then, child anchors place nothing.
    too_deep: Option<u64>,
}

/// `rect` (left, top, right, bottom in `frame`) as fractions of `outer` (fractions of a root
/// rectangle): a degenerate or missing frame gives the whole of `outer`.
fn map_child(outer: [f64; 4], frame: Option<[i32; 4]>, rect: [i32; 4]) -> [f64; 4] {
    let Some([fl, ft, fr, fb]) = frame else { return outer };
    if fr <= fl || fb <= ft {
        return outer;
    }
    let (fw, fh) = (f64::from(fr) - f64::from(fl), f64::from(fb) - f64::from(ft));
    let [l, t, r, b] = rect.map(f64::from);
    let [ox, oy, ow, oh] = outer;
    [ox + (l - f64::from(fl)) / fw * ow, oy + (t - f64::from(ft)) / fh * oh, (r - l) / fw * ow, (b - t) / fh * oh]
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    u32_at(b, at).map(|v| i32::from_le_bytes(v.to_le_bytes()))
}

fn rect_at(b: &[u8], at: usize) -> Option<[i32; 4]> {
    Some([i32_at(b, at)?, i32_at(b, at.checked_add(4)?)?, i32_at(b, at.checked_add(8)?)?, i32_at(b, at.checked_add(12)?)?])
}

impl ArtWalk {
    /// One MsoDrawing record's OfficeArt records: the last anchor in it, with the properties of
    /// its shape (the FOPTs since the shape's FSP). Containers (`recVer` 0xF) are entered, atoms
    /// skipped by their length.
    fn record(&mut self, data: &[u8]) -> Option<Found> {
        let base = self.offset;
        self.offset = base.saturating_add(data.len() as u64);
        let mut pos = 0usize;
        let mut found = None;
        let mut props = ShapeProps::default();
        for _ in 0..MAX_ART_RECORDS {
            let (Some(ver), Some(ty), Some(len)) = (u16_at(data, pos), u16_at(data, pos.saturating_add(2)), u32_at(data, pos.saturating_add(4)))
            else {
                break;
            };
            let at = base.saturating_add(pos as u64);
            while self.groups.last().is_some_and(|g| g.end <= at) {
                self.groups.pop();
            }
            if self.too_deep.is_some_and(|end| end <= at) {
                self.too_deep = None;
            }
            let body = pos.saturating_add(8);
            let end = at.saturating_add(8).saturating_add(u64::from(len));
            if ver & 0x000F == 0x000F {
                if ty == SPGR_CONTAINER && self.too_deep.is_none() {
                    if self.groups.len() < MAX_GROUP_DEPTH {
                        self.groups.push(ArtGroup { end, shape_end: None, frame: None, place: None });
                    } else {
                        self.too_deep = Some(end);
                    }
                } else if ty == SP_CONTAINER
                    && self.too_deep.is_none()
                    && let Some(g) = self.groups.last_mut()
                    && g.shape_end.is_none()
                {
                    g.shape_end = Some(end);
                }
                pos = body;
                continue;
            }
            let atom = data.get(body..body.saturating_add(usize::try_from(len).unwrap_or(usize::MAX)).min(data.len())).unwrap_or_default();
            // Whether this atom is the innermost group's own shape's.
            let group_shape = self.too_deep.is_none() && self.groups.last().and_then(|g| g.shape_end).is_some_and(|e| at < e);
            if ty == drawing::FSP {
                // The instance is the shape type ([MS-ODRAW] 2.2.40); the properties that follow it
                // are its shape's.
                props = ShapeProps { shape_type: ver >> 4, ..ShapeProps::default() };
            } else if ty == drawing::FOPT || ty == drawing::TERTIARY_FOPT {
                drawing::fopt(ver >> 4, atom, &mut props);
            } else if ty == FSPGR {
                if group_shape && let (Some(g), Some(r)) = (self.groups.last_mut(), rect_at(atom, 0)) {
                    g.frame = Some(r);
                }
            } else if ty == CLIENT_ANCHOR {
                // flags, then colL, dxL, rwT, dyT, colR, dxR, rwB, dyB.
                let f = |i: usize| u16_at(atom, 2 + 2 * i);
                if let (Some(col_l), Some(dx_l), Some(row_t), Some(dy_t), Some(col_r), Some(dx_r), Some(row_b), Some(dy_b)) =
                    (f(0), f(1), f(2), f(3), f(4), f(5), f(6), f(7))
                {
                    let a = ClientAnchor { col_l, dx_l, row_t, dy_t, col_r, dx_r, row_b, dy_b };
                    found = Some(if group_shape {
                        if let Some(g) = self.groups.last_mut() {
                            g.place = Some((a, [0.0, 0.0, 1.0, 1.0]));
                        }
                        Found::Group
                    } else {
                        Found::Shape(Some(Place::Cells(a)), props.clone())
                    });
                }
            } else if ty == CHILD_ANCHOR
                && let Some(r) = rect_at(atom, 0)
            {
                found = Some(self.child_anchor(group_shape, r, &props));
            }
            pos = body.saturating_add(usize::try_from(len).unwrap_or(usize::MAX));
        }
        found
    }

    /// An OfficeArtChildAnchor ([MS-ODRAW] 2.2.39): a rectangle in the enclosing group's frame.
    fn child_anchor(&mut self, group_shape: bool, r: [i32; 4], props: &ShapeProps) -> Found {
        if self.too_deep.is_some() {
            return if group_shape { Found::Group } else { Found::Shape(None, props.clone()) };
        }
        // A group's own shape is placed in its parent group's frame, any other shape in its group's.
        let n = self.groups.len();
        let parent = if group_shape { n.checked_sub(2) } else { n.checked_sub(1) };
        let placed = parent.and_then(|i| self.groups.get(i)).and_then(|g| g.place.map(|(root, outer)| (root, map_child(outer, g.frame, r))));
        if group_shape {
            if let Some(g) = self.groups.last_mut() {
                g.place = placed;
            }
            Found::Group
        } else {
            Found::Shape(placed.map(|(root, rel)| Place::Group(root, rel)), props.clone())
        }
    }
}

/// Shape text takes the font of the TxO run beginning at character zero: the model has one
/// font per shape.
fn apply_text_font(format: &mut ShapeFormat, text: &Text, fonts: &[(gridcraft_model::Font, u16)], table: &[u32; 64]) {
    let Some(ifnt) = text.font else { return };
    // FontIndex skips 4; invalid references must not substitute another font.
    let index = match ifnt {
        0..=3 => usize::from(ifnt),
        4 => return,
        _ => usize::from(ifnt) - 1,
    };
    let Some((font, icv)) = fonts.get(index) else { return };
    format.font = Some(font.name.clone());
    format.font_size = Some(font.size);
    format.bold = font.bold;
    format.italic = font.italic;
    format.text_color = match icv {
        8..=63 => table.get(usize::from(*icv)).copied().map(Color::Rgb),
        _ => None,
    };
}

/// The last client anchor in one MsoDrawing record's OfficeArt records, with its shape's
/// properties.
#[cfg(test)]
fn client_anchor(data: &[u8]) -> Option<(ClientAnchor, ShapeProps)> {
    match ArtWalk::default().record(data) {
        Some(Found::Shape(Some(Place::Cells(a)), props)) => Some((a, props)),
        _ => None,
    }
}

/// A text box's shape, before its TxO record has been read.
struct PendingTextBox {
    anchor: Place,
    props: ShapeProps,
}

/// A drawn shape, before its TxO record (if any) has been read.
struct PendingShape {
    anchor: Place,
    props: ShapeProps,
    kind: ShapeKind,
}

/// A BIFF5 polygon, before its COORDLIST record has been read: its shape without the outline,
/// whether the outline is closed and the number of points.
struct PendingPolygon {
    anchor: ClientAnchor,
    shape: Shape,
    closed: bool,
    cpt: u16,
}

/// A form control, before its TxO record (if any) has been read. Its linked cell and a list's
/// input range are resolved against the sheet's name when the sheet is finished.
struct PendingButton {
    anchor: Place,
    control: FormControl,
    link: Option<Reference>,
    input: Option<Reference>,
}

/// The DrawingML preset (ECMA-376 Part 1 §20.1.10.56 ST_ShapeType) of each Office Art shape type
/// ([MS-ODRAW] 2.4.24 MSOSPT) that has one, beyond the types `shape_kind` maps to a kind of its
/// own. The [MS-ODRAW] name is in the comment; a type missing here (freeforms, text effects, the
/// `90` callouts) has no preset and isn't imported.
const MSOSPT_PRESETS: &[(u16, &str)] = &[
    (4, "diamond"),                      // msosptDiamond
    (6, "rtTriangle"),                   // msosptRightTriangle
    (7, "parallelogram"),                // msosptParallelogram
    (8, "trapezoid"),                    // msosptTrapezoid
    (9, "hexagon"),                      // msosptHexagon
    (10, "octagon"),                     // msosptOctagon
    (11, "plus"),                        // msosptPlus
    (12, "star5"),                       // msosptStar
    (13, "rightArrow"),                  // msosptArrow
    (15, "homePlate"),                   // msosptHomePlate
    (16, "cube"),                        // msosptCube
    (19, "arc"),                         // msosptArc
    (21, "plaque"),                      // msosptPlaque
    (22, "can"),                         // msosptCan
    (23, "donut"),                       // msosptDonut
    (33, "bentConnector2"),              // msosptBentConnector2
    (34, "bentConnector3"),              // msosptBentConnector3
    (35, "bentConnector4"),              // msosptBentConnector4
    (36, "bentConnector5"),              // msosptBentConnector5
    (37, "curvedConnector2"),            // msosptCurvedConnector2
    (38, "curvedConnector3"),            // msosptCurvedConnector3
    (39, "curvedConnector4"),            // msosptCurvedConnector4
    (40, "curvedConnector5"),            // msosptCurvedConnector5
    (41, "callout1"),                    // msosptCallout1
    (42, "callout2"),                    // msosptCallout2
    (43, "callout3"),                    // msosptCallout3
    (44, "accentCallout1"),              // msosptAccentCallout1
    (45, "accentCallout2"),              // msosptAccentCallout2
    (46, "accentCallout3"),              // msosptAccentCallout3
    (47, "borderCallout1"),              // msosptBorderCallout1
    (48, "borderCallout2"),              // msosptBorderCallout2
    (49, "borderCallout3"),              // msosptBorderCallout3
    (50, "accentBorderCallout1"),        // msosptAccentBorderCallout1
    (51, "accentBorderCallout2"),        // msosptAccentBorderCallout2
    (52, "accentBorderCallout3"),        // msosptAccentBorderCallout3
    (53, "ribbon"),                      // msosptRibbon
    (54, "ribbon2"),                     // msosptRibbon2
    (55, "chevron"),                     // msosptChevron
    (56, "pentagon"),                    // msosptPentagon
    (57, "noSmoking"),                   // msosptNoSmoking
    (58, "star8"),                       // msosptSeal8
    (59, "star16"),                      // msosptSeal16
    (60, "star32"),                      // msosptSeal32
    (61, "wedgeRectCallout"),            // msosptWedgeRectCallout
    (62, "wedgeRoundRectCallout"),       // msosptWedgeRRectCallout
    (63, "wedgeEllipseCallout"),         // msosptWedgeEllipseCallout
    (64, "wave"),                        // msosptWave
    (65, "foldedCorner"),                // msosptFoldedCorner
    (66, "leftArrow"),                   // msosptLeftArrow
    (67, "downArrow"),                   // msosptDownArrow
    (68, "upArrow"),                     // msosptUpArrow
    (69, "leftRightArrow"),              // msosptLeftRightArrow
    (70, "upDownArrow"),                 // msosptUpDownArrow
    (71, "irregularSeal1"),              // msosptIrregularSeal1
    (72, "irregularSeal2"),              // msosptIrregularSeal2
    (73, "lightningBolt"),               // msosptLightningBolt
    (74, "heart"),                       // msosptHeart
    (76, "quadArrow"),                   // msosptQuadArrow
    (77, "leftArrowCallout"),            // msosptLeftArrowCallout
    (78, "rightArrowCallout"),           // msosptRightArrowCallout
    (79, "upArrowCallout"),              // msosptUpArrowCallout
    (80, "downArrowCallout"),            // msosptDownArrowCallout
    (81, "leftRightArrowCallout"),       // msosptLeftRightArrowCallout
    (82, "upDownArrowCallout"),          // msosptUpDownArrowCallout
    (83, "quadArrowCallout"),            // msosptQuadArrowCallout
    (84, "bevel"),                       // msosptBevel
    (85, "leftBracket"),                 // msosptLeftBracket
    (86, "rightBracket"),                // msosptRightBracket
    (87, "leftBrace"),                   // msosptLeftBrace
    (88, "rightBrace"),                  // msosptRightBrace
    (89, "leftUpArrow"),                 // msosptLeftUpArrow
    (90, "bentUpArrow"),                 // msosptBentUpArrow
    (91, "bentArrow"),                   // msosptBentArrow
    (92, "star24"),                      // msosptSeal24
    (93, "stripedRightArrow"),           // msosptStripedRightArrow
    (94, "notchedRightArrow"),           // msosptNotchedRightArrow
    (95, "blockArc"),                    // msosptBlockArc
    (96, "smileyFace"),                  // msosptSmileyFace
    (97, "verticalScroll"),              // msosptVerticalScroll
    (98, "horizontalScroll"),            // msosptHorizontalScroll
    (99, "circularArrow"),               // msosptCircularArrow
    (101, "uturnArrow"),                 // msosptUturnArrow
    (102, "curvedRightArrow"),           // msosptCurvedRightArrow
    (103, "curvedLeftArrow"),            // msosptCurvedLeftArrow
    (104, "curvedUpArrow"),              // msosptCurvedUpArrow
    (105, "curvedDownArrow"),            // msosptCurvedDownArrow
    (106, "cloudCallout"),               // msosptCloudCallout
    (107, "ellipseRibbon"),              // msosptEllipseRibbon
    (108, "ellipseRibbon2"),             // msosptEllipseRibbon2
    (109, "flowChartProcess"),           // msosptFlowChartProcess
    (110, "flowChartDecision"),          // msosptFlowChartDecision
    (111, "flowChartInputOutput"),       // msosptFlowChartInputOutput
    (112, "flowChartPredefinedProcess"), // msosptFlowChartPredefinedProcess
    (113, "flowChartInternalStorage"),   // msosptFlowChartInternalStorage
    (114, "flowChartDocument"),          // msosptFlowChartDocument
    (115, "flowChartMultidocument"),     // msosptFlowChartMultidocument
    (116, "flowChartTerminator"),        // msosptFlowChartTerminator
    (117, "flowChartPreparation"),       // msosptFlowChartPreparation
    (118, "flowChartManualInput"),       // msosptFlowChartManualInput
    (119, "flowChartManualOperation"),   // msosptFlowChartManualOperation
    (120, "flowChartConnector"),         // msosptFlowChartConnector
    (121, "flowChartPunchedCard"),       // msosptFlowChartPunchedCard
    (122, "flowChartPunchedTape"),       // msosptFlowChartPunchedTape
    (123, "flowChartSummingJunction"),   // msosptFlowChartSummingJunction
    (124, "flowChartOr"),                // msosptFlowChartOr
    (125, "flowChartCollate"),           // msosptFlowChartCollate
    (126, "flowChartSort"),              // msosptFlowChartSort
    (127, "flowChartExtract"),           // msosptFlowChartExtract
    (128, "flowChartMerge"),             // msosptFlowChartMerge
    (129, "flowChartOfflineStorage"),    // msosptFlowChartOfflineStorage
    (130, "flowChartOnlineStorage"),     // msosptFlowChartOnlineStorage
    (131, "flowChartMagneticTape"),      // msosptFlowChartMagneticTape
    (132, "flowChartMagneticDisk"),      // msosptFlowChartMagneticDisk
    (133, "flowChartMagneticDrum"),      // msosptFlowChartMagneticDrum
    (134, "flowChartDisplay"),           // msosptFlowChartDisplay
    (135, "flowChartDelay"),             // msosptFlowChartDelay
    (176, "flowChartAlternateProcess"),  // msosptFlowChartAlternateProcess
    (177, "flowChartOffpageConnector"),  // msosptFlowChartOffpageConnector
    (182, "leftRightUpArrow"),           // msosptLeftRightUpArrow
    (183, "sun"),                        // msosptSun
    (184, "moon"),                       // msosptMoon
    (185, "bracketPair"),                // msosptBracketPair
    (186, "bracePair"),                  // msosptBracePair
    (187, "star4"),                      // msosptSeal4
    (188, "doubleWave"),                 // msosptDoubleWave
    (189, "actionButtonBlank"),          // msosptActionButtonBlank
    (190, "actionButtonHome"),           // msosptActionButtonHome
    (191, "actionButtonHelp"),           // msosptActionButtonHelp
    (192, "actionButtonInformation"),    // msosptActionButtonInformation
    (193, "actionButtonForwardNext"),    // msosptActionButtonForwardNext
    (194, "actionButtonBackPrevious"),   // msosptActionButtonBackPrevious
    (195, "actionButtonEnd"),            // msosptActionButtonEnd
    (196, "actionButtonBeginning"),      // msosptActionButtonBeginning
    (197, "actionButtonReturn"),         // msosptActionButtonReturn
    (198, "actionButtonDocument"),       // msosptActionButtonDocument
    (199, "actionButtonSound"),          // msosptActionButtonSound
    (200, "actionButtonMovie"),          // msosptActionButtonMovie
];

/// The DrawingML preset of an Office Art shape type, when it has one ([`MSOSPT_PRESETS`]).
fn msospt_preset(shape_type: u16) -> Option<&'static str> {
    MSOSPT_PRESETS.iter().find(|(t, _)| *t == shape_type).map(|(_, p)| *p)
}

/// The model kind of a drawn shape: the Obj `ot` for the BIFF8 kinds, the OfficeArtFSP instance
/// (the shape type) for an Office Art object. `None` is a kind that isn't imported.
fn shape_kind(ot: u16, props: &ShapeProps) -> Option<ShapeKind> {
    // A line with an arrowhead is an arrow; a straight connector (32) is a line.
    let line = if props.arrow { ShapeKind::Arrow } else { ShapeKind::Line };
    match ot {
        OT_LINE => Some(line),
        OT_RECTANGLE => Some(match props.shape_type {
            2 => ShapeKind::RoundedRectangle,
            _ => ShapeKind::Rectangle,
        }),
        OT_OVAL => Some(ShapeKind::Ellipse),
        OT_OFFICE_ART => match props.shape_type {
            1 => Some(ShapeKind::Rectangle),
            2 => Some(ShapeKind::RoundedRectangle),
            3 => Some(ShapeKind::Ellipse),
            5 => Some(ShapeKind::Triangle),
            20 => Some(line),
            32 => Some(line),
            // Any other type with a DrawingML preset is kept as that preset (see `msospt_preset`).
            t => msospt_preset(t).map(|_| ShapeKind::Rectangle),
        },
        _ => None,
    }
}

/// A worksheet's embedded charts, pictures, text boxes and drawn shapes, collected while its
/// substream is read.
#[derive(Default)]
pub(super) struct Embedded {
    /// The last drawing shape's anchor and properties, until an Obj record claims them.
    anchor: Option<(Place, ShapeProps)>,
    /// The walk through the sheet's drawing data.
    art: ArtWalk,
    /// The anchor of the chart Obj whose substream comes next.
    pending: Option<Place>,
    /// Whether that chart's Obj has FtCmo `fPrint` clear.
    pending_no_print: bool,
    /// The shape of the text box Obj whose TxO record comes next.
    pending_text: Option<PendingTextBox>,
    /// The drawn shape whose TxO record (if any) comes next.
    pending_shape: Option<PendingShape>,
    /// The button whose TxO record (if any) comes next.
    pending_button: Option<PendingButton>,
    /// The BIFF5 polygon whose COORDLIST record comes next.
    pending_polygon: Option<PendingPolygon>,
    /// BIFF5 polygons with their outline, the anchor placed when the sheet is finished.
    freeforms: Vec<(Place, Shape)>,
    charts: Vec<(Option<Place>, Chart)>,
    pictures: Vec<(Place, ShapeProps)>,
    text_boxes: Vec<(Place, ShapeProps, Text)>,
    shapes: Vec<(Place, ShapeProps, ShapeKind, Option<Text>)>,
    buttons: Vec<PendingButton>,
    /// The anchor and name of the BIFF5 picture Obj whose IMDATA record comes next.
    pending_biff5_picture: Option<(ClientAnchor, Option<String>)>,
    /// BIFF5 pictures: anchor, name and PNG bytes.
    biff5_pictures: Vec<(ClientAnchor, Option<String>, Vec<u8>)>,
    /// Whether a BIFF5 picture's image was in a format that isn't imported.
    biff5_skipped: bool,
    /// The workbook's colour table, for the shapes' palette colours.
    palette: Palette,
    fonts: Vec<(gridcraft_model::Font, u16)>,
}

/// A workbook's 64-entry colour table: the built-in one unless the file's PALETTE record replaces
/// its entries.
#[derive(Clone, Copy)]
struct Palette([u32; 64]);

impl Default for Palette {
    fn default() -> Self {
        Palette(INDEXED)
    }
}

impl Embedded {
    /// An `Embedded` with the workbook's colour table (the palette the cell-format reader uses
    /// for icv: the defaults or the PALETTE record) and parsed FONT records.
    pub(super) fn new(palette: [u32; 64], fonts: Vec<(gridcraft_model::Font, u16)>) -> Self {
        Embedded { palette: Palette(palette), fonts, ..Embedded::default() }
    }
    /// An MsoDrawing record: remembers its shape's anchor and properties. Only a record that
    /// starts a new shape (it carries an anchor) ends the pending drawn shape: the one holding a
    /// shape's text-box client data comes between its Obj and its TxO. A group's own shape, or a
    /// grouped shape that can't be placed, leaves no anchor for its Obj.
    pub(super) fn drawing(&mut self, rec: &Record<'_>) {
        if let Some(found) = self.art.record(&rec.parts.concat()) {
            self.flush_shape();
            self.anchor = match found {
                Found::Shape(Some(place), props) => Some((place, props)),
                Found::Shape(None, _) | Found::Group => None,
            };
        }
    }

    /// Any record but COORDLIST ends a pending polygon, which then has no points and is skipped.
    pub(super) fn next_record(&mut self, id: u16, report: &mut ReadReport) {
        if id != COORDLIST && self.pending_polygon.take().is_some() {
            report.warn("Shapes in .xls files are not imported yet.");
        }
    }

    /// A BIFF5 COORDLIST record: the pending polygon's `cpt` points (x, y in 1/16384 of its box),
    /// a freeform with one path. A list shorter than `cpt` skips the polygon.
    pub(super) fn coordlist(&mut self, rec: &Record<'_>, report: &mut ReadReport) {
        let Some(PendingPolygon { anchor, mut shape, closed, cpt }) = self.pending_polygon.take() else { return };
        let data = rec.parts.concat();
        let points: Option<Vec<(i64, i64)>> =
            (0..usize::from(cpt)).map(|i| Some((i64::from(u16_at(&data, 4 * i)?), i64::from(u16_at(&data, 4 * i + 2)?)))).collect();
        let Some(points) = points.filter(|p| !p.is_empty()) else {
            report.warn("Shapes in .xls files are not imported yet.");
            return;
        };
        let mut commands: Vec<PathCommand> =
            points.iter().enumerate().map(|(i, &(x, y))| if i == 0 { PathCommand::MoveTo(x, y) } else { PathCommand::LineTo(x, y) }).collect();
        if closed {
            commands.push(PathCommand::Close);
        }
        let path = GeometryPath { w: POLY_UNIT, h: POLY_UNIT, fill: true, stroke: true, shade: Default::default(), commands };
        shape.geometry = Some(ShapeGeometry { paths: vec![path], ..ShapeGeometry::default() });
        self.freeforms.push((Place::Cells(anchor), shape));
    }

    /// A drawn shape or button whose Obj record had no TxO after it: imported with empty text.
    fn flush_shape(&mut self) {
        if let Some(PendingShape { anchor, props, kind }) = self.pending_shape.take() {
            self.shapes.push((anchor, props, kind, None));
        }
        if let Some(button) = self.pending_button.take() {
            self.buttons.push(button);
        }
    }

    /// An Obj record: a chart, picture, text box, button or drawn shape takes the shape's anchor;
    /// other form controls and objects are reported. An object the application created itself
    /// (fUIObj: an AutoFilter or validation drop-down arrow) is skipped silently. A BIFF5
    /// workbook's Obj records carry their own anchor and text (`biff5` is its code page).
    pub(super) fn obj(&mut self, rec: &Record<'_>, biff5: Option<&Biff5>, links: &Links, report: &mut ReadReport) {
        self.flush_shape();
        if let Some(b5) = biff5 {
            self.obj_biff5(&rec.parts.concat(), b5, report);
            return;
        }
        if let Some((head, more)) = rec.parts.split_first()
            && more.first().is_some_and(|data| starts_office_art(data))
        {
            // These Continue records belong to the next shape. Consume this Obj's anchor
            // before letting the drawing walker remember the next one.
            self.obj_biff8(&Record { id: rec.id, parts: vec![head] }, links, report);
            self.drawing(&Record { id: 0x00EC, parts: more.to_vec() });
        } else {
            self.obj_biff8(rec, links, report);
        }
    }

    fn obj_biff8(&mut self, rec: &Record<'_>, links: &Links, report: &mut ReadReport) {
        let head = rec.parts.first().copied().unwrap_or_default();
        let ot = u16_at(head, 4);
        // FtCmo: ft, cb, ot, id, then the flags.
        let flags = u16_at(head, 8).unwrap_or(0);
        let mut anchor = self.anchor.take();
        if flags & F_UI_OBJ != 0 {
            return;
        }
        // fPrint clear: "Print object" is off, so the object is left out of print and PDF.
        let no_print = flags & F_PRINT == 0;
        if let Some((_, props)) = anchor.as_mut() {
            props.no_print = no_print;
        }
        match ot {
            Some(OT_CHART) => {
                self.pending = anchor.map(|(a, _)| a);
                self.pending_no_print = no_print;
            }
            Some(OT_PICTURE) => {
                if let Some(a) = anchor {
                    self.pictures.push(a);
                }
            }
            Some(OT_TEXT_BOX) => {
                // A text box without its drawing shape has no anchor to place it with.
                self.pending_text = anchor.map(|(anchor, props)| PendingTextBox { anchor, props });
            }
            // An Office Art text box (shape type 202) is a text box.
            Some(OT_OFFICE_ART) if anchor.as_ref().is_some_and(|(_, p)| p.shape_type == SPT_TEXT_BOX) => {
                self.pending_text = anchor.map(|(anchor, props)| PendingTextBox { anchor, props });
            }
            Some(OT_BUTTON) => {
                // A button without its drawing shape has no anchor to place it with.
                if let Some((anchor, _)) = anchor {
                    let control = FormControl { kind: ControlKind::Button, locked: flags & F_LOCKED != 0, ..FormControl::default() };
                    self.pending_button = Some(PendingButton { anchor, control, link: None, input: None });
                } else {
                    report.warn("Form controls in .xls files are not imported yet.");
                }
            }
            Some(ot @ (OT_CHECK_BOX | OT_OPTION_BUTTON | OT_SPIN | OT_SCROLL_BAR | OT_LABEL | OT_LIST | OT_GROUP_BOX | OT_DROP_DOWN)) => {
                // A control without its drawing shape has no anchor to place it with.
                if let Some((anchor, _)) = anchor {
                    let data = rec.parts.concat();
                    let (control, link, input) = form_control(ot, flags, &data, links);
                    self.pending_button = Some(PendingButton { anchor, control, link, input });
                } else {
                    report.warn("Form controls in .xls files are not imported yet.");
                }
            }
            // A group's shapes are read from their own Obj records.
            Some(OT_NOTE | OT_GROUP) => {}
            Some(ot) if OT_FORM_CONTROLS.contains(&ot) => report.warn("Form controls in .xls files are not imported yet."),
            Some(ot) => {
                // A shape without its drawing shape has no anchor to place it with.
                if let Some((anchor, props)) = anchor
                    && let Some(kind) = shape_kind(ot, &props)
                {
                    self.pending_shape = Some(PendingShape { anchor, props, kind });
                } else {
                    report.warn("Shapes in .xls files are not imported yet.");
                }
            }
            None => report.warn("Shapes in .xls files are not imported yet."),
        }
    }

    /// A BIFF5 Obj record: its fixed 34-byte header carries the object type and the anchor, and a
    /// text box's text follows its own fields. Text boxes, rectangles, lines and pictures (whose
    /// image is in the IMDATA record that follows) are imported; every other kind keeps the report
    /// the BIFF8 path gives an object without a drawing shape.
    fn obj_biff5(&mut self, data: &[u8], b5: &Biff5, report: &mut ReadReport) {
        // Only the IMDATA record right after a picture Obj holds its image.
        self.pending_biff5_picture = None;
        let Some(ot) = u16_at(data, 4) else {
            report.warn("Shapes in .xls files are not imported yet.");
            return;
        };
        match ot {
            OT_LINE => {
                let Some(anchor) = biff5_anchor(data) else {
                    report.warn("Shapes in .xls files are not imported yet.");
                    return;
                };
                // icv, lns, lnw, fAuto, the arrow word and iqu follow the header.
                let Some([icv, lns, _lnw, f_auto, arrow_lo, arrow_hi, _iqu, _]) = data.get(34..42).and_then(|f| <[u8; 8]>::try_from(f).ok()) else {
                    report.warn("Shapes in .xls files are not imported yet.");
                    return;
                };
                let mut props = ShapeProps::default();
                // A palette colour (icv 8..=63) unless the line format is automatic.
                if f_auto & 1 == 0 && (8..=63).contains(&icv) {
                    props.line_color = Some(0x0800_0000 | u32::from(icv));
                }
                props.no_line = lns == 5;
                let arrow_word = u16::from_le_bytes([arrow_lo, arrow_hi]);
                let kind = if arrow_word & 0x000F != 0 { ShapeKind::Arrow } else { ShapeKind::Line };
                // iqu (the direction quadrant) maps to the shape's flips once the model has them.
                self.shapes.push((Place::Cells(anchor), props, kind, None));
            }
            OT_TEXT_BOX => {
                let Some(anchor) = biff5_anchor(data) else {
                    report.warn("Shapes in .xls files are not imported yet.");
                    return;
                };
                let Some(text) = biff5_text(data, b5, report) else { return };
                self.text_boxes.push((Place::Cells(anchor), ShapeProps::default(), Text { text, ..Text::default() }));
            }
            OT_RECTANGLE => {
                let Some(anchor) = biff5_anchor(data) else {
                    report.warn("Shapes in .xls files are not imported yet.");
                    return;
                };
                self.shapes.push((Place::Cells(anchor), ShapeProps::default(), ShapeKind::Rectangle, None));
            }
            // The chart substream that follows is placed where the Obj says.
            OT_CHART => {
                self.pending = biff5_anchor(data).map(Place::Cells);
                self.pending_no_print = false;
            }
            // The picture's image is in the IMDATA record that follows its Obj.
            OT_PICTURE => {
                if let Some(anchor) = biff5_anchor(data) {
                    self.pending_biff5_picture = Some((anchor, biff5_picture_name(data, b5)));
                }
            }
            OT_POLYGON => match biff5_polygon(data, &self.palette.0) {
                Some(p) => self.pending_polygon = Some(p),
                None => report.warn("Shapes in .xls files are not imported yet."),
            },
            // A note or group imports nothing.
            OT_NOTE | OT_GROUP => {}
            ot if OT_FORM_CONTROLS.contains(&ot) => report.warn("Form controls in .xls files are not imported yet."),
            _ => report.warn("Shapes in .xls files are not imported yet."),
        }
    }

    /// A BIFF5 IMDATA record ([MS-XLS] 2.4.140): `cf`, `env`, `lcb`, then `lcb` bytes of image
    /// data, the image of the picture Obj before it. A 24-bit Windows bitmap is imported as a PNG;
    /// any other format is skipped with a warning when the sheet is finished. An IMDATA without a
    /// picture Obj before it, or in a BIFF8 workbook (`biff5` false), is ignored.
    pub(super) fn imdata(&mut self, rec: &Record<'_>, biff5: bool) {
        if !biff5 {
            return;
        }
        let Some((anchor, name)) = self.pending_biff5_picture.take() else { return };
        let data = rec.parts.concat();
        let png = (|| {
            // cf 0x0009: a bitmap; env 1: Windows.
            if u16_at(&data, 0)? != 0x0009 || u16_at(&data, 2)? != 1 {
                return None;
            }
            let lcb = usize::try_from(u32_at(&data, 4)?).ok()?;
            png::bitmap_to_png(data.get(8..8usize.checked_add(lcb)?)?)
        })();
        match png {
            Some(png) => self.biff5_pictures.push((anchor, name, png)),
            None => self.biff5_skipped = true,
        }
    }

    /// A TxO record: the text of the text box, drawn shape or button whose Obj record came before
    /// it. A TxO without one is another form control's, which isn't imported.
    pub(super) fn txo(&mut self, rec: &Record<'_>, report: &mut ReadReport) {
        let text = textbox::read(&rec.parts, report);
        if let Some(PendingTextBox { anchor, props }) = self.pending_text.take() {
            if let Some(text) = text {
                self.text_boxes.push((anchor, props, text));
            }
        } else if let Some(PendingShape { anchor, props, kind }) = self.pending_shape.take() {
            // A line has no text of its own; its TxO, if any, is ignored.
            self.shapes.push((anchor, props, kind, if kind == ShapeKind::Line || kind == ShapeKind::Arrow { None } else { text }));
        } else if let Some(mut button) = self.pending_button.take() {
            // Spin buttons, scroll bars, list boxes and drop-downs have no caption.
            if matches!(
                button.control.kind,
                ControlKind::Button | ControlKind::CheckBox | ControlKind::OptionButton | ControlKind::Label | ControlKind::GroupBox
            ) {
                button.control.text = text.map_or(String::new(), |t| t.text);
            }
            self.buttons.push(button);
        }
    }

    /// A chart substream (its BOF just read): read to its EOF. Returns `false` when the stream
    /// ends inside it (cut off), so nothing more of the sheet can be read. `biff5` is set for an
    /// Excel 5.0/95 workbook, whose chart records use the BIFF5 layouts.
    pub(super) fn chart(
        &mut self,
        recs: &mut Records<'_>,
        budget: &mut Budget,
        links: &Links,
        biff5: Option<&Biff5>,
        report: &mut ReadReport,
    ) -> Result<bool, IoError> {
        let anchor = self.pending.take();
        let no_print = std::mem::take(&mut self.pending_no_print);
        match read(recs, budget, links, None, biff5, report)? {
            Read::Chart(mut ch) => {
                ch.no_print = no_print;
                self.charts.push((anchor, *ch));
            }
            Read::Skipped => {}
            Read::CutOff => return Ok(false),
        }
        Ok(true)
    }

    /// Places the charts, pictures, text boxes and drawn shapes on the sheet (its column widths
    /// and row heights are known by now). Pictures whose blip isn't a PNG or JPEG in the store are
    /// skipped with one warning.
    pub(super) fn finish(mut self, sheet: &mut Sheet, store: &BStore, report: &mut ReadReport) {
        self.flush_shape();
        self.next_record(EOF, report);
        for (anchor, mut ch) in self.charts {
            if let Some(a) = anchor {
                ch.anchor = place_at(sheet, &a);
            }
            sheet.charts.push(ch);
        }
        let mut skipped = false;
        for (a, props) in self.pictures {
            let Some(Blip::Picture { data, mime }) = props.pib.and_then(|pib| store.get(pib)) else {
                skipped = true;
                continue;
            };
            let anchor = place_at(sheet, &a);
            sheet.images.push(Image {
                camera: None,
                locks: None,
                hidden: props.hidden,
                no_print: props.no_print,
                hyperlink: None,
                name: props.name,
                id: 0,
                anchor,
                data: Arc::clone(data),
                mime: (*mime).to_string(),
                alt: props.alt.unwrap_or_default(),
                blip_ext: vec![],
                link: None,
            });
        }
        // A damaged store was reported when it was read.
        for (a, name, data) in self.biff5_pictures {
            sheet.images.push(Image {
                camera: None,
                locks: None,
                hidden: false,
                no_print: false,
                hyperlink: None,
                name,
                id: 0,
                anchor: place(sheet, &a),
                data: Arc::new(data),
                mime: "image/png".to_string(),
                alt: String::new(),
                blip_ext: vec![],
                link: None,
            });
        }
        if (skipped && !store.damaged) || self.biff5_skipped {
            report.warn("Some pictures in .xls files use a format that isn't imported (only PNG and JPEG are).");
        }
        let table = self.palette.0;
        let mut rotated = false;
        for (a, props, text) in self.text_boxes {
            let mut format =
                ShapeFormat { align: text.align, anchor: text.anchor, no_fill: props.no_fill, no_line: props.no_line, ..ShapeFormat::default() };
            apply_text_font(&mut format, &text, &self.fonts, &table);
            // `rot` 1 is stacked text and 2 and 3 turn the box; the model has no vertical-text
            // field, so the text stays upright and horizontal (never right-to-left).
            if text.rotation != 0 {
                rotated = true;
            }
            sheet.shapes.push(Shape {
                alt: String::new(),
                hidden: props.hidden,
                no_print: props.no_print,
                hyperlink: None,
                name: props.name,
                id: 0,
                kind: ShapeKind::TextBox,
                geometry: None,
                anchor: place_at(sheet, &a),
                fill: drawing::art_color(props.fill_color, &table),
                line: drawing::art_color(props.line_color, &table),
                text: text.text,
                format,
                math: Default::default(),
            });
        }
        for (a, props, kind, text) in self.shapes {
            let (fill, line) = (drawing::art_color(props.fill_color, &table), drawing::art_color(props.line_color, &table));
            // A line or arrow is never filled, whatever its fill properties say.
            let no_fill = matches!(kind, ShapeKind::Line | ShapeKind::Arrow) || props.no_fill;
            let mut format = match &text {
                Some(t) => ShapeFormat { align: t.align, anchor: t.anchor, no_fill, no_line: props.no_line, ..ShapeFormat::default() },
                None => ShapeFormat { no_fill, no_line: props.no_line, ..ShapeFormat::default() },
            };
            if let Some(text) = &text {
                apply_text_font(&mut format, text, &self.fonts, &table);
            }
            if text.as_ref().is_some_and(|t| t.rotation != 0) {
                rotated = true;
            }
            sheet.shapes.push(Shape {
                alt: String::new(),
                hidden: props.hidden,
                no_print: props.no_print,
                hyperlink: None,
                name: props.name,
                id: 0,
                kind,
                // An Office Art shape type with a DrawingML preset of its own is kept as that preset.
                geometry: if kind == ShapeKind::Rectangle {
                    msospt_preset(props.shape_type).map(|p| ShapeGeometry { preset: p.to_string(), ..ShapeGeometry::default() })
                } else {
                    None
                },
                anchor: place_at(sheet, &a),
                fill,
                line,
                text: text.map_or(String::new(), |t| t.text),
                format,
                math: Default::default(),
            });
        }
        for (a, shape) in self.freeforms {
            sheet.shapes.push(Shape { anchor: place_at(sheet, &a), ..shape });
        }
        if rotated {
            report.warn("The rotation of text boxes in .xls files is not imported; their text was kept upright.");
        }
        // Ids follow the controls the sheet already has (a kept `.xlsx` control's id is reused).
        let first = sheet.controls.iter().map(|c| c.id).max().unwrap_or(0);
        for (i, PendingButton { anchor, control, link, input }) in self.buttons.into_iter().enumerate() {
            let text = |mut r: Reference| {
                // A reference to the control's own sheet is written without the sheet.
                if r.sheet_name() == Some(sheet.name.as_str()) {
                    r.sheet = SheetSel::Current;
                }
                gridcraft_formula::print(&Expr::Ref(r))
            };
            let (link, input_range) = (link.map(text), input.map(text));
            sheet.controls.push(FormControl {
                id: first.saturating_add(u32::try_from(i).unwrap_or(u32::MAX)).saturating_add(1),
                anchor: place_at(sheet, &anchor),
                link,
                input_range,
                ..control
            });
        }
    }
}

/// Obj subrecord types ([MS-XLS] 2.4.181): FtEnd, FtSbs, the scroll bar's ObjLinkFmla, FtGboData,
/// FtCblsData, FtLbsData and the check box's ObjLinkFmla.
const FT_END: u16 = 0x0000;
const FT_SBS: u16 = 0x000C;
const FT_SBS_FMLA: u16 = 0x000E;
const FT_GBO_DATA: u16 = 0x000F;
const FT_CBLS_DATA: u16 = 0x0012;
const FT_LBS_DATA: u16 = 0x0013;
const FT_CBLS_FMLA: u16 = 0x0014;
/// FtSbs `fNo3d` ([MS-XLS] 2.5.163).
const SBS_NO_3D: u16 = 0x0008;

/// A BIFF8 check box, option button, spin button, scroll bar, label, list box, group box or
/// drop-down Obj (`data`, starting with its FtCmo) as a form control (without its anchor and
/// caption), its linked cell and a list's input range. A field the record doesn't hold keeps
/// the default.
fn form_control(ot: u16, flags: u16, data: &[u8], links: &Links) -> (FormControl, Option<Reference>, Option<Reference>) {
    let kind = match ot {
        OT_CHECK_BOX => ControlKind::CheckBox,
        OT_OPTION_BUTTON => ControlKind::OptionButton,
        OT_SPIN => ControlKind::Spin,
        OT_LABEL => ControlKind::Label,
        OT_LIST => ControlKind::ListBox,
        OT_GROUP_BOX => ControlKind::GroupBox,
        OT_DROP_DOWN => ControlKind::DropDown,
        _ => ControlKind::Slider,
    };
    let is_list = matches!(kind, ControlKind::ListBox | ControlKind::DropDown);
    let mut c = FormControl { kind, locked: flags & F_LOCKED != 0, print: flags & F_PRINT != 0, ..FormControl::default() };
    let (mut link, mut input) = (None, None);
    // Subrecords follow FtCmo (4 + 18 bytes): ft, cb, then cb bytes.
    let mut at = 22usize;
    while let (Some(ft), Some(cb)) = (u16_at(data, at), u16_at(data, at.saturating_add(2))) {
        if ft == FT_END {
            break;
        }
        // FtLbsData's cb isn't reliable: its fields are walked, and nothing after it is read.
        if ft == FT_LBS_DATA {
            if is_list {
                input = lbs_data(data.get(at.saturating_add(4)..).unwrap_or_default(), ot == OT_DROP_DOWN, &mut c, links);
            }
            break;
        }
        let start = at.saturating_add(4);
        let end = start.saturating_add(usize::from(cb));
        let body = data.get(start..end.min(data.len())).unwrap_or_default();
        let field = |i: usize| u16_at(body, i);
        let positive = |i: usize| field(i).map(|v| u32::try_from(v as i16).unwrap_or(0));
        match ft {
            // A list's own scroll state isn't its value: that is FtLbsData's selected item.
            FT_SBS if is_list => {}
            FT_SBS => {
                // unused (4), iVal, iMin, iMax, dInc, dPage, fHoriz, dxScroll, flags.
                let set = |slot: &mut u32, v: Option<u32>| {
                    if let Some(v) = v {
                        *slot = v;
                    }
                };
                set(&mut c.value, positive(4));
                set(&mut c.min, positive(6));
                set(&mut c.max, positive(8));
                set(&mut c.step, positive(10));
                set(&mut c.page, positive(12));
                if let Some(h) = field(14) {
                    c.horizontal = h == 1;
                }
                if let Some(f) = field(18) {
                    c.three_d = f & SBS_NO_3D == 0;
                }
            }
            FT_SBS_FMLA | FT_CBLS_FMLA => link = linked_cell(body, links),
            FT_CBLS_DATA => {
                if let Some(checked) = field(0) {
                    c.value = match kind {
                        ControlKind::OptionButton => u32::from(checked == 1),
                        _ => u32::from(checked),
                    };
                }
                if let Some(f) = field(6) {
                    c.three_d = f & 0x0001 == 0;
                }
            }
            // FtGboData: accel, reserved, flags (bit 0 fNo3d).
            FT_GBO_DATA if kind == ControlKind::GroupBox => {
                if let Some(f) = field(4) {
                    c.three_d = f & 0x0001 == 0;
                }
            }
            _ => {}
        }
        at = end;
    }
    (c, link, input)
}

/// FtLbsData ([MS-XLS] 2.5.149) `body` (after ft and cb): fills in a list box's or drop-down's
/// selected item, selection type, shading and (for a drop-down, `drop`) its drop lines, and
/// returns its input range. Fields the body is too short for keep their defaults.
fn lbs_data(body: &[u8], drop: bool, c: &mut FormControl, links: &Links) -> Option<Reference> {
    // fmla: an ObjFmla, cbFmla then cbFmla bytes.
    let cb_fmla = u16_at(body, 0)?;
    let fmla_end = 2usize.saturating_add(usize::from(cb_fmla));
    let input = if cb_fmla == 0 { None } else { body.get(2..fmla_end).and_then(|f| input_area(f, links)) };
    // cLines, iSel, flags, idEdit.
    let field = |i: usize| u16_at(body, fmla_end.saturating_add(i));
    if let Some(sel) = field(2) {
        c.value = u32::from(sel);
    }
    if let Some(f) = field(4) {
        c.three_d = f & 0x0008 == 0;
        c.selection = match (f >> 4) & 0x0003 {
            1 => ListSelection::Multi,
            2 => ListSelection::Extend,
            _ => ListSelection::Single,
        };
    }
    // LbsDropData ([MS-XLS] 2.5.146): flags, cLine, dxMin, str. Only cLine is read.
    if let Some(lines) = field(10).filter(|_| drop) {
        c.drop_lines = u32::from(lines);
    }
    input
}

/// A list's input range: an ObjectParsedFormula (cce in the low 15 bits, four unused bytes,
/// rgce) holding one PtgArea or PtgArea3d. Anything else is no range.
fn input_area(fmla: &[u8], links: &Links) -> Option<Reference> {
    let cce = usize::from(u16_at(fmla, 0)? & 0x7FFF);
    let rgce = fmla.get(6..6usize.checked_add(cce)?)?;
    if !matches!(rgce.first(), Some(0x25 | 0x45 | 0x65 | 0x3B | 0x5B | 0x7B)) {
        return None;
    }
    match decode_version(rgce, &[], CellRef::new(0, 0), Mode::Cell, links, None).ok()? {
        Expr::Ref(r) if !matches!(r.kind, RefKind::Cell(_)) && !matches!(r.sheet, SheetSel::Span(..)) => Some(r),
        _ => None,
    }
}

/// The cell an ObjLinkFmla ([MS-XLS] 2.5.200) links to. `body` is its ObjFmla after cbFmla (the
/// subrecord's size): an ObjectParsedFormula (cce in the low 15 bits, four unused bytes, rgce)
/// holding one PtgRef or PtgRef3d. Anything else links nowhere.
fn linked_cell(body: &[u8], links: &Links) -> Option<Reference> {
    let cce = usize::from(u16_at(body, 0)? & 0x7FFF);
    let rgce = body.get(6..6usize.checked_add(cce)?)?;
    if !matches!(rgce.first(), Some(0x24 | 0x44 | 0x64 | 0x3A | 0x5A | 0x7A)) {
        return None;
    }
    match decode_version(rgce, &[], CellRef::new(0, 0), Mode::Cell, links, None).ok()? {
        Expr::Ref(r) if matches!(r.kind, RefKind::Cell(_)) && !matches!(r.sheet, SheetSel::Span(..)) => Some(r),
        _ => None,
    }
}

/// A BIFF5 Obj's anchor: the same fields, in the same units, as an OfficeArtClientAnchorSheet
/// ([MS-XLS] 2.5.193), at offsets 10..26 of the record. `None` when the record is shorter than
/// its fixed header.
fn biff5_anchor(data: &[u8]) -> Option<ClientAnchor> {
    let f = |i: usize| u16_at(data, 10 + 2 * i);
    Some(ClientAnchor { col_l: f(0)?, dx_l: f(1)?, row_t: f(2)?, dy_t: f(3)?, col_r: f(4)?, dx_r: f(5)?, row_b: f(6)?, dy_b: f(7)? })
}

/// A BIFF5 picture Obj's name: `cch` 8-bit characters in the workbook's code page at offset 61,
/// after the picture fields. `None` when the record is too short or the name is empty.
fn biff5_picture_name(data: &[u8], b5: &Biff5) -> Option<String> {
    let cch = usize::from(*data.get(60)?);
    let name = b5.text(data.get(61..61usize.checked_add(cch)?)?);
    (!name.is_empty()).then_some(name)
}

/// A BIFF5 polygon Obj (66 bytes with no macro): the 34-byte header with its anchor, then the
/// fill and line fields at 34 (icvBack, icvFore, fls, fAutoFill, icv, lns, lnw, fAutoB), frs and
/// wPolyFlags at 42 and the point count `cpt` at 56. `None` when the record is too short or has no
/// points.
fn biff5_polygon(data: &[u8], table: &[u32; 64]) -> Option<PendingPolygon> {
    let anchor = biff5_anchor(data)?;
    let f = data.get(34..42)?;
    let (icv_fore, fls, icv, lns, auto_line) = (f.get(1)?, f.get(2)?, f.get(4)?, f.get(5)?, f.get(7)?);
    let flags = u16_at(data, 44)?;
    let cpt = u16_at(data, 56).filter(|&n| n > 0)?;
    // icv 0..64 is the colour table; 0x40 and up are automatic (system) colours.
    let color = |icv: u8| table.get(usize::from(icv)).map_or(Color::Auto, |c| Color::Rgb(*c));
    let fill = if *fls == 0 { Color::Auto } else { color(*icv_fore) };
    let line = if auto_line & 1 != 0 { Color::Auto } else { color(*icv) };
    let shape = Shape {
        alt: String::new(),
        hidden: false,
        no_print: false,
        hyperlink: None,
        name: None,
        id: 0,
        kind: ShapeKind::Freeform,
        geometry: None,
        anchor: Anchor::default(),
        fill,
        line,
        text: String::new(),
        format: ShapeFormat { no_fill: *fls == 0, no_line: *lns == 5, ..ShapeFormat::default() },
        math: Default::default(),
    };
    Some(PendingPolygon { anchor, shape, closed: flags & POLY_CLOSED != 0, cpt })
}

/// A BIFF5 text box Obj's text: `cchText` 8-bit characters in the workbook's code page at
/// `70 + cbMacro`, cut at the record end. `None` when the record is too short to hold the text
/// fields (the object is skipped).
fn biff5_text(data: &[u8], b5: &Biff5, report: &mut ReadReport) -> Option<String> {
    let cch = usize::from(u16_at(data, 44)?);
    let cb_macro = usize::from(u16_at(data, 26)?);
    let start = 70usize.checked_add(cb_macro)?;
    let end = start.checked_add(cch)?;
    let bytes = data.get(start..end);
    if bytes.is_none() {
        report.warn("A text box cut off by the end of its record kept the text that was read.");
    }
    let bytes = data.get(start..end.min(data.len())).unwrap_or_default();
    Some(b5.text(bytes))
}

/// A drawn object's anchor in sheet units: a grouped shape is anchored to the cell under its
/// top-left corner, at its place in its group's rectangle.
fn place_at(sheet: &Sheet, p: &Place) -> Anchor {
    let (root, [fx, fy, fw, fh]) = match p {
        Place::Cells(a) => return place(sheet, a),
        Place::Group(root, rel) => (root, rel),
    };
    let g = place(sheet, root);
    let (gx, gy) = (sheet.col_left(g.cell.col) + f64::from(g.dx), sheet.row_top(g.cell.row) + f64::from(g.dy));
    let (gw, gh) = (f64::from(g.width), f64::from(g.height));
    // Hostile frames can map far off the sheet: keep the numbers finite.
    let clamp = |v: f64| v.clamp(0.0, MAX_POINTS);
    let (x, y) = (clamp(gx + fx * gw), clamp(gy + fy * gh));
    let cell = CellRef::new(sheet.row_at(y), sheet.col_at(x));
    Anchor {
        cell,
        dx: (x - sheet.col_left(cell.col)).max(0.0) as f32,
        dy: (y - sheet.row_top(cell.row)).max(0.0) as f32,
        width: clamp(fw * gw) as f32,
        height: clamp(fh * gh) as f32,
        mode: AnchorMode::MoveAndSize,
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

/// Reads the chart substream of a chart sheet at `pos`, and its page setup records (Setup, the
/// margins, Header/Footer: [MS-XLS] 2.4.257, 2.4.136, 2.4.123). A substream that isn't a
/// readable chart gives `None` with a warning.
pub(super) fn chart_sheet(
    stream: &[u8],
    pos: usize,
    budget: &mut Budget,
    links: &Links,
    biff5: Option<&Biff5>,
    report: &mut ReadReport,
) -> Result<Option<(Chart, PrintSettings)>, IoError> {
    let mut recs = Records::new(stream, pos);
    let first = match recs.next_record() {
        Ok(Some(r)) => r,
        Ok(None) | Err(IoError::Format(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    if pos >= stream.len() || !is_chart_bof(&first) {
        return Ok(None);
    }
    // Excel prints a chart sheet without a Setup record (or one without an orientation) in
    // landscape; Setup sets portrait explicitly.
    let mut print = PrintSettings { orientation: Orientation::Landscape, ..PrintSettings::default() };
    Ok(match read(&mut recs, budget, links, Some(&mut print), biff5, report)? {
        Read::Chart(ch) => Some((*ch, chart_sheet_print(&print))),
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

/// Reads a chart substream up to its EOF (its BOF already read), and, for a chart sheet, its page
/// setup records into `print`. A chart that can't be imported is skipped with a warning; only
/// resource limits are errors.
fn read(
    recs: &mut Records<'_>,
    budget: &mut Budget,
    links: &Links,
    mut print: Option<&mut PrintSettings>,
    biff5: Option<&Biff5>,
    report: &mut ReadReport,
) -> Result<Read, IoError> {
    let mut records = vec![];
    let (mut tab, mut page) = (None, super::page_setup::PageState::default());
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
            _ if depth == 1 => {
                if let Some(print) = print.as_deref_mut() {
                    super::page_setup::read(&rec, print, &mut tab, &mut page, biff5, report);
                }
                records.push(rec);
            }
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
    match parse(&records, links, biff5, report) {
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

/// A chart group type record. `Radar { filled: true }` is RadarArea (0x1040), Excel's filled radar.
#[derive(Clone, Copy)]
enum GroupType {
    Bar { transpose: bool, stacked: bool, percent: bool },
    Line { stacked: bool, percent: bool },
    Pie { angle: u16, donut: u16 },
    Area { stacked: bool, percent: bool },
    Scatter { bubbles: bool },
    Radar { filled: bool },
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
    /// A chart without any Series record opens empty (BIFF5), as Excel shows its frame.
    empty_ok: bool,
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

    fn record(&mut self, rec: &Record<'_>, links: &Links, biff5: Option<&Biff5>, report: &mut ReadReport) -> Result<(), Skip> {
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
                self.brai(&mut c, links, biff5)?;
                None
            }
            SERIES_TEXT => {
                self.series_text(&mut c, biff5)?;
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
                let v = match biff5 {
                    _ if rec.id == NUMBER => Lit::Num(c.f64()?),
                    // A BIFF5 Label: a 2-byte count, then 8-bit characters in the code page.
                    Some(b5) => {
                        let cch = usize::from(c.u16()?);
                        Lit::Str(b5.text(&c.bytes(cch)?))
                    }
                    None => Lit::Str(c.string()?),
                };
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

    /// BRAI: a series' name, values, categories or bubble sizes as a reference (BIFF5 tokens when
    /// `biff5` is set).
    fn brai(&mut self, c: &mut Cursor<'_, '_>, links: &Links, biff5: Option<&Biff5>) -> Result<(), Skip> {
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
        let Ok(e) = decode_version(rgce, &[], CellRef::new(0, 0), Mode::Cell, links, biff5) else {
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

    /// SeriesText: a literal series name, or a title's text. BIFF5 has no flags byte: its `cch`
    /// 8-bit characters are in the workbook's code page.
    fn series_text(&mut self, c: &mut Cursor<'_, '_>, biff5: Option<&Biff5>) -> Result<(), Skip> {
        c.u16()?;
        let text = match biff5 {
            Some(b5) => {
                let cch = usize::from(c.u8()?);
                b5.text(&c.bytes(cch)?)
            }
            None => c.short_string()?,
        };
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
        if out.is_empty() && !(self.empty_ok && self.series_seen == 0) {
            return Err(Skip::Why("it has no data series that can be imported".into()));
        }
        let text = |link: u16| self.texts.iter().find(|t| t.link == Some(link)).and_then(|t| t.text.clone()).filter(|t| !t.is_empty());
        let kind = if combo { ChartKind::Combo } else { used.first().and_then(|g| kinds.get(*g)).or(kinds.first()).copied().unwrap_or_default() };
        let pie = if combo {
            used.iter().filter_map(|gi| self.groups.get(*gi)).find_map(|g| match g.ty {
                Some(GroupType::Pie { angle, donut }) => Some((angle, donut)),
                _ => None,
            })
        } else {
            used.first().and_then(|gi| self.groups.get(*gi)).and_then(|g| match g.ty {
                Some(GroupType::Pie { angle, donut }) => Some((angle, donut)),
                _ => None,
            })
        };
        let (first_slice_angle, hole_size) =
            pie.map_or((0, 75), |(angle, donut)| (angle % 360, if donut == 0 { 75 } else { donut.clamp(10, 90) as u8 }));
        let (width, height) = self.frame.filter(|(w, h)| *w > 0.0 && *h > 0.0).unwrap_or((480.0, 288.0));
        if !matches!(kind, ChartKind::Scatter | ChartKind::ScatterLines | ChartKind::Bubble) {
            self.scale.x = AxisScale::default();
        }
        Ok(Chart {
            of_pie: None,
            alt: String::new(),
            theme_override: None,
            hidden: false,
            no_print: false,
            id: 0,
            kind,
            anchor: Anchor { width, height, ..Anchor::default() },
            title: text(1),
            series: out,
            legend: self.legend.unwrap_or(LegendPos::None),
            data_labels: self.value_labels,
            gridlines: self.gridlines,
            style: 2,
            hole_size,
            first_slice_angle,
            x_title: text(3),
            y_title: text(2),
            source: None,
            by_rows: false,
            pivot: None,
            scale: self.scale,
            view_3d: None,
            data_table: None,
        })
    }
}

/// A BIFF5 chart substream's own EXTERNSHEET entries. An encoded file path ending in
/// `[BOOK.XLS]Sheet` whose sheet is one of this workbook's is the workbook referring to itself
/// under an earlier file name, and reads as that sheet; other workbooks stay external.
fn chart_externs(records: &[Record<'_>], b5: &Biff5, links: &Links) -> Biff5 {
    let mut externs = vec![];
    for rec in records.iter().filter(|r| r.id == EXTERN_SHEET).take(MAX_EXTERNS) {
        let mut c = Cursor::new(&rec.parts);
        let raw = c.u8().and_then(|cch| c.bytes(usize::from(cch).min(c.remaining()))).unwrap_or_default();
        let kind = match b5.extern_kind(&raw) {
            ExternKind::Other => own_sheet(&raw, b5, links).map_or(ExternKind::Other, ExternKind::Sheet),
            kind => kind,
        };
        externs.push(Extern { kind, names: vec![] });
    }
    Biff5 { codepage: b5.codepage, externs }
}

/// The sheet of this workbook named after the `]` of an encoded file path (first byte 0x01).
fn own_sheet(raw: &[u8], b5: &Biff5, links: &Links) -> Option<String> {
    let (0x01, path) = raw.split_first()? else { return None };
    let path = b5.text(path);
    let (_, name) = path.rsplit_once(']')?;
    links.sheets.iter().find(|s| !name.is_empty() && s.eq_ignore_ascii_case(name)).cloned()
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
        PIE => GroupType::Pie { angle: c.u16()?, donut: c.u16()? },
        SCATTER => {
            c.u32()?;
            GroupType::Scatter { bubbles: c.u16()? & 1 != 0 }
        }
        // Pie of pie and bar of pie: drawn as a pie.
        BOP_POP => GroupType::Pie { angle: 0, donut: 0 },
        SURF => GroupType::Unsupported("surface"),
        // Radar and RadarArea: flags (axis labels, shadow) then an unused u16; neither changes the kind.
        RADAR | RADAR_AREA => GroupType::Radar { filled: id == RADAR_AREA },
        _ => GroupType::Unsupported("unknown"),
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
        Some(GroupType::Pie { donut: 0, .. }) => ChartKind::Pie,
        Some(GroupType::Pie { .. }) => ChartKind::Doughnut,
        Some(GroupType::Scatter { bubbles: true }) => ChartKind::Bubble,
        Some(GroupType::Scatter { .. }) if members.iter().any(|s| s.line.is_some_and(|(_, drawn)| drawn)) => ChartKind::ScatterLines,
        Some(GroupType::Scatter { .. }) => ChartKind::Scatter,
        Some(GroupType::Radar { filled: true }) => ChartKind::RadarFilled,
        Some(GroupType::Radar { filled: false }) => ChartKind::Radar,
        Some(GroupType::Unsupported(_)) | None => ChartKind::ColumnClustered,
    }
}

fn parse(records: &[Record<'_>], links: &Links, biff5: Option<&Biff5>, report: &mut ReadReport) -> Result<Chart, Skip> {
    // A BIFF5 chart's 3-D references (ixals > 0) index its own EXTERNSHEET list, not the workbook's.
    let local = biff5.map(|b5| chart_externs(records, b5, links));
    let biff5 = local.as_ref();
    let mut p = Parser { empty_ok: biff5.is_some(), ..Parser::default() };
    for rec in records {
        p.record(rec, links, biff5, report)?;
    }
    p.finish(report)
}

/// A chart sheet read from the file: its name, state, how many worksheets come before it, its
/// chart and its page setup.
pub(crate) struct ChartSheet {
    pub name: String,
    pub visibility: Visibility,
    pub before: usize,
    pub chart: Chart,
    pub print: PrintSettings,
}

/// The part of a sheet's print settings a chart sheet keeps (as the `.xlsx` chart sheet reader
/// does): orientation, paper, margins and header/footer.
pub(crate) fn chart_sheet_print(p: &PrintSettings) -> PrintSettings {
    PrintSettings {
        orientation: p.orientation,
        paper: p.paper.clone(),
        margins: p.margins,
        header: p.header.clone(),
        footer: p.footer.clone(),
        different_first: p.different_first,
        different_odd_even: p.different_odd_even,
        first_header: p.first_header.clone(),
        first_footer: p.first_footer.clone(),
        even_header: p.even_header.clone(),
        even_footer: p.even_footer.clone(),
        hf_scale_with_doc: p.hf_scale_with_doc,
        hf_align_with_margins: p.hf_align_with_margins,
        ..PrintSettings::default()
    }
}

/// The chart sheet XML (ECMA-376 Part 1 §18.3.1.12) in CT_Chartsheet order: the page margins, a
/// `<pageSetup>` (§18.3.1.64) for a paper or orientation that isn't the default (a chart sheet
/// without an orientation is landscape, so portrait is written out), and a
/// `<headerFooter>` (§18.3.1.46) when there is one, before the drawing.
fn chart_sheet_xml(p: &PrintSettings) -> String {
    let mut s = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<chartsheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"{REL}\"><sheetPr/><sheetViews><sheetView workbookViewId=\"0\"/></sheetViews>"
    );
    crate::sheet_write::write_page_margins(&mut s, p);
    let paper = crate::tables::paper_id(&p.paper).filter(|&id| id != 1);
    let portrait = p.orientation == Orientation::Portrait;
    if paper.is_some() || portrait {
        s.push_str("<pageSetup");
        if let Some(id) = paper {
            let _ = write!(s, " paperSize=\"{id}\"");
        }
        s.push_str(if portrait { " orientation=\"portrait\"" } else { " orientation=\"landscape\"" });
        s.push_str("/>");
    }
    crate::sheet_write::write_header_footer(&mut s, p);
    s.push_str("<drawing r:id=\"rId1\"/></chartsheet>");
    s
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
        let sheet_xml = chart_sheet_xml(&cs.print);
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
        let mut chart_rels = Vec::new();
        for (id, name, content_type, data, rel_type) in [
            ("rId1", format!("xl/charts/chartsheet_style{n}.xml"), crate::chartex::CT_STYLE, crate::chartex::style_xml(), crate::chartex::REL_STYLE),
            (
                "rId2",
                format!("xl/charts/chartsheet_colors{n}.xml"),
                crate::chartex::CT_COLORS,
                crate::chartex::colors_xml(),
                crate::chartex::REL_COLORS,
            ),
        ] {
            chart_rels.push(PreservedRel { id: id.into(), rel_type: rel_type.into(), target: name.clone(), external: false });
            kept.parts.push(PreservedPart { name, content_type: content_type.into(), data: Arc::from(data.into_bytes()), rels: vec![] });
        }
        if let Some(bytes) = &cs.chart.theme_override {
            let name = format!("xl/theme/chartsheet_themeOverride{n}.xml");
            chart_rels.push(PreservedRel { id: "rId3".into(), rel_type: format!("{REL}/themeOverride"), target: name.clone(), external: false });
            kept.parts.push(PreservedPart {
                name,
                content_type: "application/vnd.openxmlformats-officedocument.themeOverride+xml".into(),
                data: Arc::from(bytes.as_slice()),
                rels: vec![],
            });
        }
        kept.parts.push(PreservedPart {
            name: chart_part,
            content_type: "application/vnd.openxmlformats-officedocument.drawingml.chart+xml".into(),
            data: Arc::from(chart_xml.into_bytes()),
            rels: chart_rels,
        });
        let rel_id = format!("rIdXlsChartSheet{n}");
        kept.rels.push(PreservedRel { id: rel_id.clone(), rel_type: format!("{REL}/chartsheet"), target: sheet_part, external: false });
        kept.sheets.push(PreservedSheet { name: cs.name.clone(), visibility: cs.visibility, rel_id, before: cs.before, sheet_id: 0 });
        prints.push(ChartSheetPrint { name: cs.name, visibility: cs.visibility, before: cs.before, chart: cs.chart, print: cs.print });
    }
    wb.preserved = Some(Arc::new(kept));
    wb.chart_sheets = Arc::new(prints);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_art_continuation_requires_a_complete_record_in_the_expected_type_range() {
        let valid = art(0xF, SP_CONTAINER, &[0; 12]);
        assert!(starts_office_art(&valid));
        for end in 0..valid.len() {
            assert!(!starts_office_art(&valid[..end]));
        }
        assert!(starts_office_art(&art(0, 0xF000, &[])));
        assert!(starts_office_art(&art(0, 0xF122, &[])));
        assert!(!starts_office_art(&art(0, 0xEFFF, &[])));
        assert!(!starts_office_art(&art(0, 0xF123, &[])));
        assert!(!starts_office_art(&[0x0F, 0, 4, 0xF0, 0xFF, 0xFF, 0xFF, 0xFF]));
        // Ordinary Obj subrecords remain part of the object, even with a complete header.
        assert!(!starts_office_art(&art(0, FT_SBS, &[0; 20])));
    }

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

    fn art(ver: u16, ty: u16, body: &[u8]) -> Vec<u8> {
        let mut v = [ver.to_le_bytes(), ty.to_le_bytes()].concat();
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn deeply_nested_groups_are_bounded_and_extreme_frames_stay_finite() {
        let rect = |r: [i32; 4]| r.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        let mut anchor = vec![0u8; 18];
        anchor[2] = 1;
        anchor[10] = 5;
        anchor[14] = 9;
        // A top-level group placed by cells, then 40 nested groups with extreme frames, each
        // in its own record; the innermost child is past the depth limit, so it has no place.
        let mut inner = art(0xF, SP_CONTAINER, &art(0, CHILD_ANCHOR, &rect([i32::MIN, i32::MIN, i32::MAX, i32::MAX])));
        for _ in 0..40 {
            let shape =
                art(0xF, SP_CONTAINER, &[art(1, FSPGR, &rect([0, 0, 1, 1])), art(0, CHILD_ANCHOR, &rect([i32::MIN, 0, i32::MAX, 1]))].concat());
            inner = art(0xF, SPGR_CONTAINER, &[shape, inner].concat());
        }
        let shape = art(0xF, SP_CONTAINER, &[art(1, FSPGR, &rect([0, 0, 1, 1])), art(0, CLIENT_ANCHOR, &anchor)].concat());
        let data = art(0xF, SPGR_CONTAINER, &[shape, inner].concat());
        let mut walk = ArtWalk::default();
        assert!(matches!(walk.record(&data), Some(Found::Shape(None, _))));
        assert!(walk.groups.len() <= MAX_GROUP_DEPTH);
        // Two levels deep, the extreme frames map to finite placements on the sheet.
        let mut walk = ArtWalk::default();
        let two = art(
            0xF,
            SPGR_CONTAINER,
            &[
                art(0xF, SP_CONTAINER, &[art(1, FSPGR, &rect([0, 0, 1, 1])), art(0, CLIENT_ANCHOR, &anchor)].concat()),
                art(0xF, SP_CONTAINER, &art(0, CHILD_ANCHOR, &rect([i32::MIN, i32::MIN, i32::MAX, i32::MAX]))),
            ]
            .concat(),
        );
        let Some(Found::Shape(Some(place), _)) = walk.record(&two) else { panic!("child not placed") };
        let a = place_at(&Sheet::new("S"), &place);
        assert!(a.dx.is_finite() && a.dy.is_finite() && a.width.is_finite() && a.height.is_finite());
    }
}
