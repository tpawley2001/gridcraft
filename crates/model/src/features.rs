//! Sheet features beyond cells: tables, conditional formatting, data validation, comments,
//! hyperlinks, charts, images, autofilter, print settings.

use gridcraft_core::{CellRef, RangeRef};
use serde::{Deserialize, Serialize};

use crate::style::{Color, Style};

mod pivot;
mod preset_geometry;
mod print;
mod table_sort;

pub use pivot::{PivotCalcField, PivotDateGroup, PivotField, PivotFilter, PivotFunc, PivotLayout, PivotShowAs, PivotSort, PivotTable, PivotValue};
pub use preset_geometry::preset_paths;
pub use print::{
    ChartSheetPrint, EditRange, HfPage, LineInfo, Orientation, PageOrder, PrintComments, PrintErrors, PrintOutput, PrintSettings, SheetProtection,
    password_hash,
};
pub use table_sort::{MAX_CUSTOM_LIST_CHARS, MAX_SORT_CONDITIONS, SortBy, SortCondition, TableSort};

// ---------------------------------------------------------------- tables

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TotalsFn {
    #[default]
    None,
    Average,
    Count,
    CountNums,
    Max,
    Min,
    Sum,
    StdDev,
    Var,
    Custom,
}

impl TotalsFn {
    /// SUBTOTAL function number.
    pub fn subtotal_code(&self) -> Option<u32> {
        Some(match self {
            TotalsFn::Average => 101,
            TotalsFn::Count => 103,
            TotalsFn::CountNums => 102,
            TotalsFn::Max => 104,
            TotalsFn::Min => 105,
            TotalsFn::Sum => 109,
            TotalsFn::StdDev => 107,
            TotalsFn::Var => 110,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableColumn {
    pub name: String,
    pub totals: TotalsFn,
    /// Totals row label (first column) or custom formula.
    pub totals_label: Option<String>,
    /// Calculated column formula.
    pub formula: Option<String>,
    /// `uniqueName`: the name XML maps know the column by.
    #[serde(default)]
    pub unique_name: Option<String>,
    /// Binding to an element of an XML map. It moves with the column and goes when the column is
    /// deleted; inserted columns are unmapped.
    #[serde(default)]
    pub xml: Option<XmlColumnPr>,
    /// Differential formats (`headerRowDxfId`, `dataDxfId`, `totalsRowDxfId`) applied to this
    /// column's header cell, data cells and totals cell. They move with the column.
    #[serde(default)]
    pub header_row_dxf: Option<Style>,
    #[serde(default)]
    pub data_dxf: Option<Style>,
    #[serde(default)]
    pub totals_row_dxf: Option<Style>,
}

/// A table column's binding to an XML map (`<xmlColumnPr>`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct XmlColumnPr {
    pub map_id: u32,
    pub xpath: String,
    /// `xmlDataType` as written in the file (`string`, `boolean`, `integer`…).
    pub data_type: String,
    pub denormalized: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Table {
    pub id: u32,
    pub name: String,
    /// Whole table including header and totals rows.
    pub range: RangeRef,
    pub header_row: bool,
    pub totals_row: bool,
    pub columns: Vec<TableColumn>,
    pub style: String,
    pub banded_rows: bool,
    pub banded_cols: bool,
    pub first_col: bool,
    pub last_col: bool,
    pub filter_button: bool,
    /// Criteria of the table's AutoFilter (column offset within the table → criterion). With
    /// criteria present, hidden data rows are filtered out rather than hidden by hand, which
    /// SUBTOTAL distinguishes.
    #[serde(default)]
    pub filter: Vec<(u32, FilterCriterion)>,
    /// `tableType` when it isn't a plain worksheet table: `xml` for a table bound to an XML map.
    #[serde(default)]
    pub table_type: Option<String>,
    /// The connection (`xl/connections.xml`) an XML table is bound through.
    #[serde(default)]
    pub connection_id: Option<u32>,
    /// Differential formats of the header row, data area and totals row (`headerRowDxfId`,
    /// `dataDxfId`, `totalsRowDxfId`).
    #[serde(default)]
    pub header_row_dxf: Option<Style>,
    #[serde(default)]
    pub data_dxf: Option<Style>,
    #[serde(default)]
    pub totals_row_dxf: Option<Style>,
    /// Border formats of the header row, the whole table and the totals row
    /// (`headerRowBorderDxfId`, `tableBorderDxfId`, `totalsRowBorderDxfId`).
    #[serde(default)]
    pub header_row_border_dxf: Option<Style>,
    #[serde(default)]
    pub table_border_dxf: Option<Style>,
    #[serde(default)]
    pub totals_row_border_dxf: Option<Style>,
    /// Saved sort settings (`<sortState>`), kept for the round trip.
    #[serde(default)]
    pub sort: Option<TableSort>,
    /// The table's `comment` (at most 32767 characters), kept for the round trip.
    #[serde(default)]
    pub comment: Option<String>,
    /// `insertRow`, `insertRowShift` and `published` as saved; `None` when the file didn't say.
    #[serde(default)]
    pub insert_row: Option<bool>,
    #[serde(default)]
    pub insert_row_shift: Option<bool>,
    #[serde(default)]
    pub published: Option<bool>,
}

impl Table {
    pub fn data_range(&self) -> Option<RangeRef> {
        let r0 = self.range.start.row + self.header_row as u32;
        let r1 = self.range.end.row.checked_sub(self.totals_row as u32)?;
        if r1 < r0 {
            return None;
        }
        Some(RangeRef::new(CellRef::new(r0, self.range.start.col), CellRef::new(r1, self.range.end.col)))
    }
    pub fn header_range(&self) -> Option<RangeRef> {
        self.header_row.then(|| RangeRef::new(self.range.start, CellRef::new(self.range.start.row, self.range.end.col)))
    }
    pub fn totals_range(&self) -> Option<RangeRef> {
        self.totals_row.then(|| RangeRef::new(CellRef::new(self.range.end.row, self.range.start.col), self.range.end))
    }
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.name.eq_ignore_ascii_case(name))
    }
}

// ---------------------------------------------------------------- conditional formatting

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CfOperator {
    Between,
    NotBetween,
    Equal,
    NotEqual,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CfValueKind {
    Min,
    Max,
    Number(f64),
    Percent(f64),
    Percentile(f64),
    Formula(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CfRule {
    /// Cell value compared with one or two formulas.
    CellIs {
        op: CfOperator,
        a: String,
        b: Option<String>,
        style: Box<Style>,
    },
    /// Custom formula (relative to the top-left cell of the range).
    Expression {
        formula: String,
        style: Box<Style>,
    },
    ContainsText {
        text: String,
        style: Box<Style>,
    },
    NotContainsText {
        text: String,
        style: Box<Style>,
    },
    BeginsWith {
        text: String,
        style: Box<Style>,
    },
    EndsWith {
        text: String,
        style: Box<Style>,
    },
    Blanks {
        style: Box<Style>,
    },
    NoBlanks {
        style: Box<Style>,
    },
    Errors {
        style: Box<Style>,
    },
    NoErrors {
        style: Box<Style>,
    },
    Duplicate {
        style: Box<Style>,
    },
    Unique {
        style: Box<Style>,
    },
    /// Top/bottom N or N percent.
    Top10 {
        bottom: bool,
        percent: bool,
        rank: u32,
        style: Box<Style>,
    },
    AboveAverage {
        below: bool,
        equal: bool,
        std_dev: u8,
        style: Box<Style>,
    },
    /// `yesterday`, `today`, `tomorrow`, `last7Days`, `lastWeek`, `thisWeek`, `nextWeek`, `lastMonth`, `thisMonth`, `nextMonth`.
    TimePeriod {
        period: String,
        style: Box<Style>,
    },
    ColorScale {
        stops: Vec<(CfValueKind, Color)>,
    },
    DataBar {
        min: CfValueKind,
        max: CfValueKind,
        color: Color,
        gradient: bool,
        show_value: bool,
    },
    /// Icon set name (`3Arrows`, `3TrafficLights1`, `4Rating`, `5Quarters`…).
    IconSet {
        set: String,
        thresholds: Vec<CfValueKind>,
        reverse: bool,
        show_value: bool,
    },
}

impl CfRule {
    /// The differential style a matching rule applies; colour scales, data bars and icon sets have none.
    pub fn style(&self) -> Option<&Style> {
        match self {
            CfRule::CellIs { style, .. }
            | CfRule::Expression { style, .. }
            | CfRule::ContainsText { style, .. }
            | CfRule::NotContainsText { style, .. }
            | CfRule::BeginsWith { style, .. }
            | CfRule::EndsWith { style, .. }
            | CfRule::Blanks { style }
            | CfRule::NoBlanks { style }
            | CfRule::Errors { style }
            | CfRule::NoErrors { style }
            | CfRule::Duplicate { style }
            | CfRule::Unique { style }
            | CfRule::Top10 { style, .. }
            | CfRule::AboveAverage { style, .. }
            | CfRule::TimePeriod { style, .. } => Some(style),
            CfRule::ColorScale { .. } | CfRule::DataBar { .. } | CfRule::IconSet { .. } => None,
        }
    }
}

/// Number of icons in an icon set (`3Arrows` → 3, `5Quarters` → 5), always 3 to 5.
pub fn icon_count(set: &str) -> usize {
    set.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(3).clamp(3, 5) as usize
}

/// Excel's default icon-set thresholds: one per icon, the first being the lower bound,
/// then even percent steps (`0, 33, 67` for three icons).
pub fn default_icon_thresholds(set: &str) -> Vec<CfValueKind> {
    let k = icon_count(set);
    (0..k).map(|i| CfValueKind::Percent((i as f64 * 100.0 / k as f64).round())).collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CondFormat {
    pub ranges: Vec<RangeRef>,
    pub rule: CfRule,
    pub priority: u32,
    pub stop_if_true: bool,
}

// ---------------------------------------------------------------- data validation

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationKind {
    #[default]
    Any,
    Whole,
    Decimal,
    List,
    Date,
    Time,
    TextLength,
    Custom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorStyle {
    #[default]
    Stop,
    Warning,
    Information,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Validation {
    pub ranges: Vec<RangeRef>,
    pub kind: ValidationKind,
    pub op: CfOperator,
    /// Formula 1 (list source: `"a,b,c"` literal list or `=$A$1:$A$5`).
    pub f1: String,
    pub f2: Option<String>,
    pub allow_blank: bool,
    pub in_cell_dropdown: bool,
    pub input_title: String,
    pub input_message: String,
    pub show_input: bool,
    pub error_title: String,
    pub error_message: String,
    pub error_style: ErrorStyle,
    pub show_error: bool,
}

impl Default for Validation {
    fn default() -> Self {
        Validation {
            ranges: vec![],
            kind: ValidationKind::Any,
            op: CfOperator::Between,
            f1: String::new(),
            f2: None,
            allow_blank: true,
            in_cell_dropdown: true,
            input_title: String::new(),
            input_message: String::new(),
            show_input: true,
            error_title: String::new(),
            error_message: String::new(),
            error_style: ErrorStyle::Stop,
            show_error: true,
        }
    }
}

// ---------------------------------------------------------------- comments, links

/// A note (classic comment) or a threaded comment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Comment {
    pub author: String,
    pub text: String,
    /// Replies of a threaded comment (author, text).
    pub replies: Vec<(String, String)>,
    pub threaded: bool,
    pub resolved: bool,
    pub visible: bool,
    /// Threaded comments: when the thread (first entry) and each reply were written, ISO 8601 as
    /// read from the file (`dT`); empty when unknown, e.g. for comments made here.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dates: Vec<String>,
    /// Where the note's box sits and how big it is, as read from the file; `None` (a note made
    /// here, or a file without the shape) writes the default box next to the cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_box: Option<NoteBox>,
    /// A picture filling the note's box (VML `<v:fill o:relid>`), as read from the file; `None`
    /// writes the default plain fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture_fill: Option<NotePictureFill>,
    /// The note shape's VML colours, outline and placement flags, as read from the file; `None`
    /// (a note made here, or one with the default look) writes the default shape.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note_shape: Option<NoteShape>,
}

/// A note shape's VML appearance and placement, kept from the file so a save doesn't reset them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoteShape {
    /// `fillcolor` as six-digit `#rrggbb`; `None` writes the default note colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill_color: Option<String>,
    /// `strokecolor` as six-digit `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_color: Option<String>,
    /// `strokeweight` in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_weight_pt: Option<f64>,
    /// `filled`: `Some(false)` when the box has no fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filled: Option<bool>,
    /// `stroked`: `Some(false)` when the box has no outline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroked: Option<bool>,
    /// `<x:MoveWithCells>` is present and on. Despite its name, the element means the box does
    /// *not* move with the cells under it (as for form controls); `false` writes no element.
    #[serde(default, alias = "move_with_cells")]
    pub fixed_position: bool,
    /// `<x:SizeWithCells>` is present and on: the box does *not* resize with the cells under it;
    /// `false` writes no element.
    #[serde(default, alias = "size_with_cells")]
    pub fixed_size: bool,
}

impl NoteShape {
    /// The look and placement written for a note without kept metadata.
    pub const DEFAULT: NoteShape = NoteShape {
        fill_color: None,
        stroke_color: None,
        stroke_weight_pt: None,
        filled: None,
        stroked: None,
        fixed_position: true,
        fixed_size: true,
    };
}

/// A picture used as a note box's fill, with the VML `<v:fill>` settings that go with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NotePictureFill {
    /// Encoded picture bytes, base64 in JSON; shared so copies of the sheet don't copy them.
    #[serde(with = "crate::b64::shared")]
    pub data: std::sync::Arc<Vec<u8>>,
    /// MIME type from the picture part's name (empty when unknown: the bytes tell the format).
    pub mime: String,
    /// `<v:fill>` attributes kept as written: `o:title`, `type`, `recolor`, `color2`, `aspect`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attrs: Vec<(String, String)>,
}

/// A note box's place and size, from the note's VML shape: Excel's `<x:Anchor>` with its corners
/// stored relative to the note's cell, so the box moves with the note when rows or columns are
/// inserted or deleted, and the shape's width and height.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoteBox {
    /// Top-left corner: columns and rows from the note's cell (negative = left of or above it),
    /// then the pixel offset into that column and row.
    pub from: NoteCorner,
    /// Bottom-right corner, the same way.
    pub to: NoteCorner,
    /// The shape's width and height in points (the VML style's `width`/`height`), when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_pt: Option<(f64, f64)>,
}

/// One corner of a [`NoteBox`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteCorner {
    pub col: i64,
    pub col_px: u32,
    pub row: i64,
    pub row_px: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hyperlink {
    /// URL, `mailto:`, a file path (`Book.xlsx`, `Book.xlsx#Sheet1!A2`), or an in-workbook location
    /// like `Sheet2!A1`.
    pub target: String,
    pub tooltip: Option<String>,
    /// The target is outside this workbook (Excel's "Existing File or Web Page"): a relationship in
    /// the file, not a `location`. A bare file name can't be told from a defined name by its text.
    #[serde(default)]
    pub external: bool,
}

// ---------------------------------------------------------------- drawing objects

/// How a floating object is pinned to the grid when its cells move or resize. Mirrors Excel's
/// Format ▸ Size and Properties ▸ Properties, stored in the file as the anchor's `editAs`:
/// `twoCell` (absent `editAs`), `oneCell` and `absolute`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnchorMode {
    /// Move and size with cells (Excel's default; `<xdr:twoCellAnchor>`).
    #[default]
    MoveAndSize,
    /// Move but don't size with cells (`<xdr:twoCellAnchor editAs="oneCell">` / `<xdr:oneCellAnchor>`).
    MoveOnly,
    /// Don't move or size with cells (`editAs="absolute"` / `<xdr:absoluteAnchor>`).
    Absolute,
}

/// Position of a floating object: top-left cell plus offset, size in points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub cell: CellRef,
    pub dx: f32,
    pub dy: f32,
    pub width: f32,
    pub height: f32,
    /// How the object follows its anchor cell when rows/columns move (sort, insert, delete).
    #[serde(default)]
    pub mode: AnchorMode,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChartKind {
    #[default]
    ColumnClustered,
    ColumnStacked,
    ColumnStacked100,
    BarClustered,
    BarStacked,
    BarStacked100,
    Line,
    LineMarkers,
    LineStacked,
    Pie,
    Doughnut,
    /// Pie of pie (`c:ofPieChart` with `ofPieType val="pie"`): some points move to a second pie.
    PieOfPie,
    /// Bar of pie (`c:ofPieChart` with `ofPieType val="bar"`): some points move to a stacked bar.
    BarOfPie,
    Area,
    AreaStacked,
    AreaStacked100,
    Scatter,
    ScatterLines,
    Bubble,
    Radar,
    /// Filled radar (`c:radarChart` with `radarStyle val="filled"`): each series is a filled polygon.
    RadarFilled,
    Histogram,
    Waterfall,
    Funnel,
    Treemap,
    Sunburst,
    BoxWhisker,
    Stock,
    Combo,
    /// 3-D Surface (`c:surface3DChart`, filled).
    Surface,
    /// Wireframe 3-D Surface (`c:surface3DChart` with `c:wireframe`).
    SurfaceWireframe,
    /// Contour: a surface seen from above (`c:surfaceChart`, filled).
    Contour,
    /// Wireframe Contour (`c:surfaceChart` with `c:wireframe`).
    ContourWireframe,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LegendPos {
    None,
    #[default]
    Bottom,
    Top,
    Left,
    Right,
}

/// Quartile calculation for a box & whisker series.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuartileMethod {
    #[default]
    Exclusive,
    Inclusive,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Series {
    /// Formula text for the series name (`Sheet1!$B$1`) or a literal.
    pub name: Option<String>,
    pub categories: Option<String>,
    pub values: String,
    /// X values for scatter / bubble sizes.
    pub bubble_sizes: Option<String>,
    pub color: Option<Color>,
    /// Combo charts: this series drawn as a line on the secondary axis.
    pub secondary: bool,
    pub kind: Option<ChartKind>,
    /// Draw line segments as a smoothed (spline) curve.
    #[serde(default)]
    pub smooth: bool,
    /// Box & whisker quartiles; Excel defaults to exclusive.
    #[serde(default)]
    pub quartiles: QuartileMethod,
    /// Trendlines fitted to this series (Excel allows several per series).
    #[serde(default)]
    pub trendlines: Vec<Trendline>,
    /// Pie and doughnut: every slice pulled out from the centre by this percent of the radius
    /// (`c:explosion`, 0 = closed, at most [`Series::MAX_EXPLOSION`]).
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub explosion: u32,
    /// Pie and doughnut: `(point index, percent)` for slices pulled out on their own
    /// (`c:dPt`/`c:explosion`); replaces [`Series::explosion`] for that slice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub point_explosions: Vec<(u32, u32)>,
    /// 3-D column/bar: this series' own bar shape (`c:ser/c:shape`): box, cylinder, cone,
    /// coneToMax, pyramid or pyramidToMax; `None` uses the group's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_shape: Option<String>,
}

fn is_zero_u32(v: &u32) -> bool {
    *v == 0
}

impl Series {
    /// Largest explosion percent kept (the most Excel's Format Data Series pane accepts).
    pub const MAX_EXPLOSION: u32 = 400;

    /// Explosion percent of slice `point`: its own value if it has one, else the series value.
    /// Clamped to [`Series::MAX_EXPLOSION`].
    pub fn explosion_of(&self, point: usize) -> u32 {
        self.point_explosions
            .iter()
            .rev()
            .find(|(i, _)| usize::try_from(*i).is_ok_and(|i| i == point))
            .map_or(self.explosion, |(_, e)| *e)
            .min(Self::MAX_EXPLOSION)
    }

    /// Largest explosion of any slice (clamped), for sizing the pie.
    pub fn max_explosion(&self) -> u32 {
        self.point_explosions.iter().map(|(_, e)| *e).chain([self.explosion]).max().unwrap_or(0).min(Self::MAX_EXPLOSION)
    }
}

/// Regression a trendline fits (`c:trendlineType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendlineKind {
    #[default]
    Linear,
    /// `y = c·e^(b·x)`.
    Exponential,
    /// `y = c·ln(x) + b`.
    Logarithmic,
    /// Polynomial of this order (2..=6).
    Polynomial(u8),
    /// `y = c·x^b`.
    Power,
    /// Average of this many consecutive points (2..=255).
    MovingAverage(u32),
}

/// DrawingML preset line dash (`a:prstDash`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineDash {
    #[default]
    Solid,
    Dot,
    Dash,
    LgDash,
    DashDot,
    LgDashDot,
    LgDashDotDot,
    SysDash,
    SysDot,
    SysDashDot,
    SysDashDotDot,
}

/// A chart trendline (Add Chart Element ▸ Trendline, Format Trendline pane).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Trendline {
    pub kind: TrendlineKind,
    /// Custom name shown in the legend; `None` = automatic ("Linear (Sales)").
    pub name: Option<String>,
    /// Forecast past the last point, in x units (categories on a category axis).
    pub forward: f64,
    /// Forecast before the first point, in x units.
    pub backward: f64,
    /// Set Intercept: fixed y at x = 0 (linear, exponential, polynomial).
    pub intercept: Option<f64>,
    pub display_equation: bool,
    pub display_r_squared: bool,
    /// Line colour; `None` = the series colour.
    pub color: Option<Color>,
    /// Line width in points; `None` = default.
    pub width: Option<f32>,
    /// Line dash; `None` = default.
    pub dash: Option<LineDash>,
}

impl Trendline {
    /// Longest forward / backward forecast kept from a file or a command, in x units. Excel's own
    /// limit is unconfirmed; the renderer additionally bounds the forecast by the plot's slot budget.
    pub const MAX_FORECAST: f64 = 100_000.0;

    /// A forecast distance as stored: finite, not negative (NaN and infinity become 0) and at most
    /// [`Self::MAX_FORECAST`].
    pub fn clamp_forecast(v: f64) -> f64 {
        if v.is_finite() && v > 0.0 { v.min(Self::MAX_FORECAST) } else { 0.0 }
    }
}

/// Charts saved before `hole_size` existed were drawn with a 50% hole. New charts and charts read
/// from XLSX set 75 explicitly (Excel's default), so only a missing JSON field keeps the old look.
fn default_hole_size() -> u8 {
    50
}

/// Which points of a pie of pie or bar of pie go to the second plot (`c:splitType`,
/// ECMA-376 Part 1 §21.2.2.179, with `c:splitPos` and `c:custSplit`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum OfPieSplit {
    /// Excel's automatic split: the last third of the points (rounded up).
    #[default]
    Auto,
    /// The last `n` points.
    Position(f64),
    /// Points whose value is less than this.
    Value(f64),
    /// Points whose share of the total is less than this percentage.
    Percent(f64),
    /// These zero-based point indexes.
    Custom(Vec<u32>),
}

/// The second plot of a pie of pie or bar of pie (`c:ofPieChart`, §21.2.2.126).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OfPie {
    /// Gap between the pie and the second plot, as a percentage of the pie's radius (0–500).
    pub gap_width: u16,
    pub split: OfPieSplit,
    /// Size of the second plot as a percentage of the pie's (5–200).
    pub second_size: u16,
    /// Series lines from the combined slice to the second plot (`c:serLines`).
    pub series_lines: bool,
}

impl Default for OfPie {
    /// Excel's settings for a new chart and for a file that leaves them out.
    fn default() -> Self {
        Self { gap_width: 100, split: OfPieSplit::Auto, second_size: 75, series_lines: true }
    }
}

/// A chart's data table (`c:dTable`), independent of the chart legend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChartDataTable {
    pub show_keys: bool,
    pub horizontal_border: bool,
    pub vertical_border: bool,
    pub outline: bool,
}

impl Default for ChartDataTable {
    fn default() -> Self {
        Self { show_keys: true, horizontal_border: true, vertical_border: true, outline: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    /// Opaque DrawingML theme override copied with this chart; preserved without rendering.
    #[serde(default, with = "crate::b64::opt", skip_serializing_if = "Option::is_none")]
    pub theme_override: Option<std::sync::Arc<Vec<u8>>>,
    /// Hidden drawing objects stay in the workbook but are not displayed or printed.
    #[serde(default)]
    pub hidden: bool,
    /// "Print object" turned off (`xdr:clientData fPrintsWithSheet="0"`): the object still shows
    /// on screen but is left out of print, print preview and PDF export.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_print: bool,
    /// Rounded chart-area corners (`c:roundedCorners`, ECMA-376 Part 1 §21.2.2.156). A chart part
    /// without the element has rounded corners in Excel; new charts are square-cornered.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rounded_corners: bool,
    /// The chart's editing language (`c:lang`, §21.2.2.82, e.g. `de-DE`), kept on save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Alternative text (`xdr:cNvPr descr`); empty when the chart has none.
    #[serde(default)]
    pub alt: String,
    pub id: u32,
    pub kind: ChartKind,
    pub anchor: Anchor,
    pub title: Option<String>,
    pub series: Vec<Series>,
    pub legend: LegendPos,
    pub data_labels: bool,
    #[serde(default)]
    pub data_table: Option<ChartDataTable>,
    pub gridlines: bool,
    pub style: u32,
    /// Doughnut hole size as a percentage of the plot radius (Excel's 10–90 range). New charts and
    /// XLSX imports use Excel's 75; a document saved before this field existed reads as 50.
    #[serde(default = "default_hole_size")]
    pub hole_size: u8,
    /// Pie/doughnut first slice angle in degrees clockwise from 12 o'clock (0–359).
    #[serde(default)]
    pub first_slice_angle: u16,
    pub x_title: Option<String>,
    pub y_title: Option<String>,
    /// Source range when created from a selection (for Switch Row/Column).
    pub source: Option<String>,
    pub by_rows: bool,
    /// A PivotChart's PivotTable (`Sheet2!PivotTable1`): its series are rebuilt from the report
    /// whenever the PivotTable is refreshed or its layout changes.
    #[serde(default)]
    pub pivot: Option<String>,
    /// Fixed axis bounds and units (Format Axis ▸ Bounds/Units); automatic when unset.
    #[serde(default, skip_serializing_if = "ChartScale::is_auto")]
    pub scale: ChartScale,
    /// Pie of pie / bar of pie second-plot settings; `None` uses [`OfPie::default`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub of_pie: Option<OfPie>,
    /// A 3-D column, bar, line, area or pie chart's settings (`c:view3D`, the `c:*3DChart` group's
    /// `gapDepth` and `shape`, series in depth); `None` for a flat chart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_3d: Option<Chart3d>,
}

/// A 3-D chart's view (`c:view3D`) and group settings. Each view value is kept exactly as the file
/// had it: `None` when the file left it out, so it stays out on save.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chart3d {
    /// X rotation (elevation) in degrees, -90..=90 (`rotX`).
    pub rot_x: Option<i16>,
    /// Height as a percentage of the chart width, 5..=500 (`hPercent`).
    pub h_percent: Option<u16>,
    /// Y rotation in degrees, 0..=359 (`rotY`).
    pub rot_y: Option<u16>,
    /// Depth as a percentage of the chart width, 20..=2000 (`depthPercent`).
    pub depth_percent: Option<u16>,
    /// Right-angle axes (`rAngAx`).
    pub right_angle_axes: Option<bool>,
    /// Field of view in degrees, 0..=240 (`perspective`).
    pub perspective: Option<u8>,
    /// Gap between series in depth as a percentage of the column width, 0..=500 (`gapDepth`).
    pub gap_depth: Option<u16>,
    /// The bar shape (`shape`): box, cylinder, cone, coneToMax, pyramid or pyramidToMax.
    pub shape: Option<String>,
    /// The series stand one behind the other (standard grouping with a series axis).
    pub series_in_depth: bool,
}

/// One value axis's scaling: `c:scaling` (min, max, log base, orientation) and the major/minor
/// units. `None` is Excel's "Auto".
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AxisScale {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major_unit: Option<f64>,
    pub minor_unit: Option<f64>,
    pub log_base: Option<f64>,
    /// Values in reverse order (`orientation maxMin`).
    pub reverse: bool,
    /// Display units of a value axis (Format Axis ▸ Display units); `None` shows full values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_units: Option<DisplayUnits>,
    /// Where this axis crosses the perpendicular axis, in that axis's units (`c:crosses` /
    /// `c:crossesAt`): a category number on a category chart's value axis, an X value on a
    /// scatter chart's Y axis, a Y value on its X axis.
    #[serde(default, skip_serializing_if = "AxisCrosses::is_auto")]
    pub crosses: AxisCrosses,
}

impl AxisScale {
    pub fn is_auto(&self) -> bool {
        *self == AxisScale::default()
    }
}

/// Where an axis crosses its perpendicular axis (ECMA-376 §21.2.2 `crosses` / `crossesAt`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum AxisCrosses {
    /// At zero (or the nearest end when zero is outside the axis), at the first category.
    #[default]
    AutoZero,
    /// At the perpendicular axis's minimum.
    Min,
    /// At the perpendicular axis's maximum.
    Max,
    /// At this value (category number) of the perpendicular axis.
    At(f64),
}

impl AxisCrosses {
    pub fn is_auto(&self) -> bool {
        *self == AxisCrosses::AutoZero
    }
}

/// A value axis's display unit (`c:builtInUnit` / `c:custUnit`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum DisplayUnit {
    Hundreds,
    Thousands,
    TenThousands,
    HundredThousands,
    Millions,
    TenMillions,
    HundredMillions,
    Billions,
    Trillions,
    Custom(f64),
}

impl DisplayUnit {
    /// Built-in units with their `ST_BuiltInUnit` names, smallest first.
    pub const BUILT_IN: [(DisplayUnit, &'static str); 9] = [
        (DisplayUnit::Hundreds, "hundreds"),
        (DisplayUnit::Thousands, "thousands"),
        (DisplayUnit::TenThousands, "tenThousands"),
        (DisplayUnit::HundredThousands, "hundredThousands"),
        (DisplayUnit::Millions, "millions"),
        (DisplayUnit::TenMillions, "tenMillions"),
        (DisplayUnit::HundredMillions, "hundredMillions"),
        (DisplayUnit::Billions, "billions"),
        (DisplayUnit::Trillions, "trillions"),
    ];

    /// What axis values are divided by; 1 for an unusable custom unit (not finite or not positive).
    pub fn factor(&self) -> f64 {
        match self {
            DisplayUnit::Hundreds => 1e2,
            DisplayUnit::Thousands => 1e3,
            DisplayUnit::TenThousands => 1e4,
            DisplayUnit::HundredThousands => 1e5,
            DisplayUnit::Millions => 1e6,
            DisplayUnit::TenMillions => 1e7,
            DisplayUnit::HundredMillions => 1e8,
            DisplayUnit::Billions => 1e9,
            DisplayUnit::Trillions => 1e12,
            DisplayUnit::Custom(f) if f.is_finite() && *f > 0.0 => *f,
            DisplayUnit::Custom(_) => 1.0,
        }
    }
}

/// A value axis's display units and whether their label is shown (`c:dispUnitsLbl`).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DisplayUnits {
    pub unit: DisplayUnit,
    #[serde(default)]
    pub show_label: bool,
}

/// The scaling of a chart's value axes: `y` is the primary value axis; `x` is the horizontal value
/// axis of a scatter or bubble chart (category axes have no bounds).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChartScale {
    #[serde(default)]
    pub x: AxisScale,
    #[serde(default)]
    pub y: AxisScale,
    /// Where a category chart's category axis crosses the value axis (a value).
    #[serde(default, skip_serializing_if = "AxisCrosses::is_auto")]
    pub cat_crosses: AxisCrosses,
    /// The primary category axis is a date axis (`c:dateAx`, ECMA-376 Part 1 §21.2.2) with these
    /// options; `None` for a text (category) axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_axis: Option<DateAxis>,
}

impl ChartScale {
    pub fn is_auto(&self) -> bool {
        self.x.is_auto() && self.y.is_auto() && self.cat_crosses.is_auto() && self.date_axis.is_none()
    }
}

/// A time unit of a date axis (ST_TimeUnit, ECMA-376 Part 1 §21.2.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeUnit {
    Days,
    Months,
    Years,
}

impl TimeUnit {
    /// The ST_TimeUnit token (`days`, `months`, `years`).
    pub fn token(self) -> &'static str {
        match self {
            TimeUnit::Days => "days",
            TimeUnit::Months => "months",
            TimeUnit::Years => "years",
        }
    }

    /// Parses an ST_TimeUnit token; anything else is `None`.
    pub fn from_token(s: &str) -> Option<TimeUnit> {
        match s.trim() {
            "days" => Some(TimeUnit::Days),
            "months" => Some(TimeUnit::Months),
            "years" => Some(TimeUnit::Years),
            _ => None,
        }
    }
}

/// A date axis's options (CT_DateAx): automatic axis type, base/major/minor time units, its major
/// and minor units (positive) and its bounds (date serials, `min` below `max`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DateAxis {
    /// `c:auto`: Excel picks the axis type from the data.
    #[serde(default)]
    pub auto: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_time_unit: Option<TimeUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_unit: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_time_unit: Option<TimeUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_unit: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minor_time_unit: Option<TimeUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    /// The file's axis ids (`c:axId` of the date axis, and of the value axis it crosses), kept on
    /// save as Excel does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_ids: Option<(u32, u32)>,
}

/// A drawing object's click hyperlink (DrawingML `a:hlinkClick`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrawingHyperlink {
    /// External URL, or a workbook location prefixed with `#`.
    pub target: String,
    #[serde(default)]
    pub tooltip: Option<String>,
}

impl Chart {
    /// Data tables belong to category-axis column, bar, line and area charts.
    pub fn supports_data_table(&self) -> bool {
        let eligible = |kind| {
            matches!(
                kind,
                ChartKind::ColumnClustered
                    | ChartKind::ColumnStacked
                    | ChartKind::ColumnStacked100
                    | ChartKind::BarClustered
                    | ChartKind::BarStacked
                    | ChartKind::BarStacked100
                    | ChartKind::Line
                    | ChartKind::LineMarkers
                    | ChartKind::LineStacked
                    | ChartKind::Area
                    | ChartKind::AreaStacked
                    | ChartKind::AreaStacked100
            )
        };
        eligible(self.kind) || (self.kind == ChartKind::Combo && self.series.iter().all(|s| eligible(s.kind.unwrap_or(ChartKind::Line))))
    }
}

/// Picture editing locks (`a:picLocks`, ECMA-376 Part 1 §20.1.2.2.31).
/// Each flag prohibits the named operation; omitted flags default to false.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PictureLocks {
    pub no_grp: bool,
    pub no_select: bool,
    pub no_rot: bool,
    pub no_change_aspect: bool,
    pub no_move: bool,
    pub no_resize: bool,
    pub no_edit_points: bool,
    pub no_adjust_handles: bool,
    pub no_change_arrowheads: bool,
    pub no_change_shape_type: bool,
    pub no_crop: bool,
}

/// Camera picture metadata linking a saved snapshot to its range and legacy VML shape.
/// Kept for file round-trips; GridCraft does not refresh the snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CameraTool {
    pub cell_range: String,
    pub spid: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Image {
    /// Imported `a14:cameraTool` metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub camera: Option<CameraTool>,
    /// Imported picture locks. `None` uses the inserted-picture default aspect lock on save;
    /// `Some(PictureLocks::default())` explicitly leaves every lock off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locks: Option<PictureLocks>,
    /// Hidden drawing objects stay in the workbook but are not displayed or printed.
    #[serde(default)]
    pub hidden: bool,
    /// "Print object" turned off (`xdr:clientData fPrintsWithSheet="0"`): the object still shows
    /// on screen but is left out of print, print preview and PDF export.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_print: bool,
    #[serde(default)]
    pub hyperlink: Option<DrawingHyperlink>,
    /// Imported drawing object name (`xdr:cNvPr name`).
    #[serde(default)]
    pub name: Option<String>,
    pub id: u32,
    pub anchor: Anchor,
    /// Encoded bytes (PNG/JPEG), base64 in JSON. Shared, so a copy of the sheet (an undo step)
    /// doesn't copy the picture.
    #[serde(with = "crate::b64::shared")]
    pub data: std::sync::Arc<Vec<u8>>,
    pub mime: String,
    pub alt: String,
    /// The external target of a linked picture (`a:blip r:link`, ECMA-376 Part 1 20.1.8.13), a
    /// URL or file path, verbatim. `data` is empty for a picture that is only linked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Extensions of the picture's `a:blip` GridCraft does not model (artistic effects and
    /// background removal with their HD Photo layer, the local-dpi flag), written back on save.
    /// A picture inserted in GridCraft has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blip_ext: Vec<KeptExtension>,
}

impl Image {
    /// Whether "Lock aspect ratio" is on (`noChangeAspect`). `None` is the inserted-picture
    /// default, which saves `noChangeAspect="1"`.
    pub fn aspect_locked(&self) -> bool {
        self.locks.as_ref().is_none_or(|locks| locks.no_change_aspect)
    }
}

/// One file-format extension element GridCraft does not model (an `a:ext`), kept verbatim with
/// the namespace declarations it needs and the package parts its relationship ids name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeptExtension {
    pub xml: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<KeptExtensionPart>,
}

/// A package part a [`KeptExtension`] refers to by relationship id.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct KeptExtensionPart {
    /// The relationship id the extension XML uses for it (rewritten on save).
    pub id: String,
    /// The full relationship type URI.
    pub rel_type: String,
    pub content_type: String,
    /// The part's file extension (`wdp`), without the dot.
    pub ext: String,
    /// The part's bytes, base64 in JSON, shared like [`Image::data`].
    #[serde(with = "crate::b64::shared")]
    pub data: std::sync::Arc<Vec<u8>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeKind {
    #[default]
    Rectangle,
    RoundedRectangle,
    Ellipse,
    Triangle,
    Line,
    Arrow,
    TextBox,
    /// An icon from GridCraft's own icon set; `Shape::text` holds the icon name.
    Icon,
    /// A freehand ink stroke; `Shape::text` holds the points as `x,y;x,y;…` relative to the
    /// anchor (sheet points), `Shape::line` the ink colour and `fill` unused.
    Ink,
    /// An equation: `Shape::text` holds its LaTeX source, `Shape::line` its ink colour
    /// (`Color::Auto` for the sheet's text colour) and `Shape::math` its cached layout.
    Equation,
    /// A freeform: DrawingML custom geometry (`a:custGeom`, ECMA-376 Part 1 §20.1.9.8) whose
    /// outline is `Shape::geometry`'s `paths`, or an Excel 5.0/95 polygon read as one.
    Freeform,
}

/// A DrawingML preset geometry (`a:prstGeom`, ECMA-376 Part 1 §20.1.9.18).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeGeometry {
    /// The preset's name (`prst`, an ST_ShapeType value such as `star5` or `wedgeRectCallout`).
    pub preset: String,
    /// The adjust-value guides (`a:avLst/a:gd`): each `name` and `fmla`, in document order.
    #[serde(default)]
    pub adjust: Vec<(String, String)>,
    /// A freeform's custom geometry paths (`a:custGeom/a:pathLst`), in document order; empty for a
    /// preset, whose `preset` is then the geometry.
    #[serde(default)]
    pub paths: Vec<GeometryPath>,
}

/// One path of a custom geometry (`a:path`, ECMA-376 Part 1 §20.1.9.15): its own coordinate
/// space `w` × `h` (stretched to the shape's box) and its commands, coordinates as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeometryPath {
    pub w: i64,
    pub h: i64,
    /// `fill` is not `none`.
    pub fill: bool,
    /// `stroke` is not false.
    pub stroke: bool,
    /// How the fill is shaded (`fill` = `lighten`, `darken`, ...); `Norm` for a plain fill.
    #[serde(default)]
    pub shade: PathShade,
    pub commands: Vec<PathCommand>,
}

/// The shading of a path's fill (ST_PathFillMode, ECMA-376 Part 1 §20.1.10.37), apart from `none`,
/// which is [`GeometryPath::fill`] false.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathShade {
    #[default]
    Norm,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

impl PathShade {
    /// `rgb` shaded for this mode. ECMA-376 gives no factors; these are an approximation, not
    /// measured from Excel: darken ×0.60, darkenLess ×0.80, lighten 40% and lightenLess 20% of the
    /// way to white, per channel, rounded.
    pub fn apply(self, rgb: [u8; 3]) -> [u8; 3] {
        let f = |c: u8| -> u8 {
            let c = f64::from(c);
            let v = match self {
                PathShade::Norm => c,
                PathShade::Darken => c * 0.60,
                PathShade::DarkenLess => c * 0.80,
                PathShade::Lighten => c + (255.0 - c) * 0.40,
                PathShade::LightenLess => c + (255.0 - c) * 0.20,
            };
            v.round().clamp(0.0, 255.0) as u8
        };
        [f(rgb[0]), f(rgb[1]), f(rgb[2])]
    }
}

/// A custom geometry path command (`a:moveTo`, `a:lnTo`, `a:cubicBezTo`, `a:quadBezTo`, `a:arcTo`,
/// `a:close`). Angles are in 60000ths of a degree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PathCommand {
    MoveTo(i64, i64),
    LineTo(i64, i64),
    CubicBezTo([(i64, i64); 3]),
    QuadBezTo([(i64, i64); 2]),
    ArcTo { w_r: i64, h_r: i64, st_ang: i64, sw_ang: i64 },
    Close,
}

/// An `a:arcTo`'s start and sweep as ellipse parameters, in radians, for flattening it as
/// `(cx + w_r cos t, cy + h_r sin t)`. DrawingML angles are the visual angle of the point from the
/// ellipse's centre (the presets compute them with `at2`), not the parameter; the two differ when
/// `w_r` ≠ `h_r`. The sweep keeps its direction and is at most one turn.
pub fn arc_parameters(w_r: i64, h_r: i64, st_ang: i64, sw_ang: i64) -> (f64, f64) {
    let (wr, hr) = (w_r as f64, h_r as f64);
    let st = (st_ang as f64 / 60_000.0).to_radians();
    let sw_deg = (sw_ang as f64 / 60_000.0).clamp(-360.0, 360.0);
    let param = |a: f64| (wr * a.sin()).atan2(hr * a.cos());
    let t0 = param(st);
    if sw_deg.abs() >= 360.0 || wr <= 0.0 || hr <= 0.0 {
        return (if wr > 0.0 && hr > 0.0 { t0 } else { st }, sw_deg.to_radians());
    }
    let tau = std::f64::consts::TAU;
    let mut dt = (param(st + sw_deg.to_radians()) - t0).rem_euclid(tau);
    if sw_deg < 0.0 && dt > 0.0 {
        dt -= tau;
    }
    (t0, dt)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    /// Hidden drawing objects stay in the workbook but are not displayed or printed.
    #[serde(default)]
    pub hidden: bool,
    /// "Print object" turned off (`xdr:clientData fPrintsWithSheet="0"`): the object still shows
    /// on screen but is left out of print, print preview and PDF export.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_print: bool,
    #[serde(default)]
    pub hyperlink: Option<DrawingHyperlink>,
    /// Imported drawing object name (`xdr:cNvPr name`).
    #[serde(default)]
    pub name: Option<String>,
    /// Alternative text (`xdr:cNvPr descr`); empty when the shape has none.
    #[serde(default)]
    pub alt: String,
    pub id: u32,
    pub kind: ShapeKind,
    /// The DrawingML preset geometry the shape was read with, when `kind` alone doesn't say it (a
    /// star, a callout, a flowchart box) or the preset carries adjust values. `kind` stays the
    /// closest kind GridCraft draws, so a shape whose preset isn't drawn yet draws as `kind`.
    #[serde(default)]
    pub geometry: Option<ShapeGeometry>,
    pub anchor: Anchor,
    pub fill: Color,
    pub line: Color,
    pub text: String,
    /// Outline visibility and text formatting (DrawingML `a:ln`, `a:bodyPr`, `a:pPr`, `a:rPr`).
    #[serde(default)]
    pub format: ShapeFormat,
    /// An equation's layout, rebuilt from `text` and never saved.
    #[serde(skip)]
    pub math: EquationCache,
}

/// The laid-out form of an equation's LaTeX source. It is derived data: rebuilt by
/// [`EquationCache::refresh`] whenever the source changes, never saved, and ignored when shapes
/// are compared. Painters read it and never lay equations out themselves, so nothing in a paint
/// path can reach the layout engine.
///
/// An entry answers only for the source it was built from, compared in full: a hash match alone
/// is not enough, so two sources whose hashes collide never share a layout. A source over
/// [`gridcraft_math::MAX_SOURCE_BYTES`] never lays out, so [`EquationCache::get`] answers for it
/// by its length alone, with [`gridcraft_math::SOURCE_TOO_LONG`], and an entry built from one holds
/// neither the source nor its hash: a painter asking every frame about a source of megabytes read
/// from a file does no work on it.
#[derive(Clone, Debug, Default)]
pub struct EquationCache {
    /// A hash of the source the entry was built from (a quick first check).
    key: u64,
    /// The source's length in bytes.
    len: usize,
    /// The source itself, when it is within the length cap.
    source: Option<std::sync::Arc<str>>,
    layout: Option<Result<std::sync::Arc<gridcraft_math::MathList>, gridcraft_math::MathError>>,
}

impl PartialEq for EquationCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

fn source_key(src: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    #[cfg(test)]
    equation_cache_tests::HASHES.with(|h| h.set(h.get() + 1));
    let mut h = std::collections::hash_map::DefaultHasher::new();
    src.hash(&mut h);
    h.finish()
}

impl EquationCache {
    /// Whether this entry was built from `src`. Over the cap the length is the match (an entry for
    /// such a source is always the same error); within it, the hash and then the source itself.
    fn holds(&self, src: &str) -> bool {
        if self.layout.is_none() || self.len != src.len() {
            return false;
        }
        if src.len() > gridcraft_math::MAX_SOURCE_BYTES {
            return self.source.is_none();
        }
        self.key == source_key(src) && self.source.as_deref() == Some(src)
    }

    /// Makes this the entry for `src`, with `layout`. A source over the cap is neither copied nor
    /// hashed.
    fn store(&mut self, src: &str, layout: Result<std::sync::Arc<gridcraft_math::MathList>, gridcraft_math::MathError>) {
        let within = src.len() <= gridcraft_math::MAX_SOURCE_BYTES;
        self.key = if within { source_key(src) } else { 0 };
        self.len = src.len();
        self.source = within.then(|| std::sync::Arc::from(src));
        self.layout = Some(layout);
    }

    /// The layout of `src`, or why it has none; `None` until [`EquationCache::refresh`] has run
    /// for this source. A source over [`gridcraft_math::MAX_SOURCE_BYTES`] is answered at once,
    /// whatever the entry holds, by its length alone.
    pub fn get(&self, src: &str) -> Option<Result<&gridcraft_math::MathList, &gridcraft_math::MathError>> {
        if src.len() > gridcraft_math::MAX_SOURCE_BYTES {
            return Some(Err(&*gridcraft_math::SOURCE_TOO_LONG));
        }
        if !self.holds(src) {
            return None;
        }
        self.layout.as_ref().map(|r| r.as_ref().map(|l| l.as_ref()))
    }

    /// Lays `src` out again unless the cache already holds its layout. True when it did.
    pub fn refresh(&mut self, src: &str) -> bool {
        if self.holds(src) {
            return false;
        }
        self.store(src, gridcraft_math::layout(src).map(std::sync::Arc::new));
        true
    }

    /// Records that `src` was not laid out, and why, without running the layout engine.
    pub fn refuse(&mut self, src: &str, message: &str) {
        self.store(src, Err(gridcraft_math::MathError { span: None, message: message.to_string() }));
    }
}

/// What a form control on the sheet is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ControlKind {
    #[default]
    Button,
    Slider,
    Spin,
    CheckBox,
    OptionButton,
    ListBox,
    DropDown,
    GroupBox,
    Label,
}

impl ControlKind {
    /// The kind's name in the window: Button, Slider, Spin Button, Check Box, Option Button, List
    /// Box, Drop-down, Group Box, Label.
    pub fn label(self) -> &'static str {
        match self {
            ControlKind::Button => "Button",
            ControlKind::Slider => "Slider",
            ControlKind::Spin => "Spin Button",
            ControlKind::CheckBox => "Check Box",
            ControlKind::OptionButton => "Option Button",
            ControlKind::ListBox => "List Box",
            ControlKind::DropDown => "Drop-down",
            ControlKind::GroupBox => "Group Box",
            ControlKind::Label => "Label",
        }
    }

    /// Whether the kind picks one item of its input range (a list box or drop-down); its value is
    /// the 1-based item (0 = none).
    pub fn is_list(self) -> bool {
        matches!(self, ControlKind::ListBox | ControlKind::DropDown)
    }

    /// Whether the kind holds a number in a range (a slider or spin button), with min, max, step
    /// and page.
    pub fn is_number(self) -> bool {
        matches!(self, ControlKind::Slider | ControlKind::Spin)
    }

    /// The largest state of a check box (2 = mixed) or option button (1 = selected); `None` for
    /// the other kinds.
    pub fn max_state(self) -> Option<u32> {
        match self {
            ControlKind::CheckBox => Some(2),
            ControlKind::OptionButton => Some(1),
            _ => None,
        }
    }
}

/// The largest value a slider or spin button takes (Excel's scroll bar limit). Values are whole
/// numbers from 0 to this.
pub const CONTROL_MAX: u32 = 30_000;

/// What a button runs when it is pressed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ControlAction {
    /// Runs a macro of the workbook on `input`, optionally sending its output to `output`.
    Macro {
        name: String,
        #[serde(default)]
        input: MacroInput,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output: Option<String>,
    },
    /// Runs one GridCraft command with fixed parameters.
    Command {
        id: String,
        #[serde(default)]
        params: serde_json::Value,
    },
}

/// The range a button's macro runs on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MacroInput {
    /// The selection when the button is pressed.
    #[default]
    Selection,
    /// Always this range (`B2:D9` or `Data!B2:D9`).
    Fixed(String),
}

/// How many items of a list box can be selected at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ListSelection {
    /// One item; the linked cell holds its 1-based position.
    #[default]
    Single,
    /// Each click adds or removes an item.
    Multi,
    /// Shift extends the selection over a block of items.
    Extend,
}

/// The rows a new drop-down shows when it is open.
pub const DEFAULT_DROP_LINES: u32 = 8;

/// A form control (button, slider, spin button, check box, option button, list box, drop-down,
/// group box, label) floating over the sheet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FormControl {
    pub id: u32,
    pub kind: ControlKind,
    pub anchor: Anchor,
    /// The name shown in the Selection Pane (`Button 3`); empty means the kind and id.
    pub name: String,
    /// The caption of a button, check box, option button, group box or label.
    pub text: String,
    /// The linked cell as reference text: `$B$2` on the control's own sheet, or `Sheet2!$B$2`.
    /// Structural edits rewrite it like a formula reference.
    pub link: Option<String>,
    /// The current value of a slider or spin button (the linked cell holds it too); the state of
    /// a check box (0 unchecked, 1 checked, 2 mixed) or option button (1 selected); the 1-based
    /// selected item of a list box or drop-down (0 = none).
    pub value: u32,
    pub min: u32,
    pub max: u32,
    /// Incremental change: one arrow click or arrow key.
    pub step: u32,
    /// Page change (sliders): a click on the track, or Page Up / Page Down.
    pub page: u32,
    /// A slider lies horizontally (false: vertically).
    pub horizontal: bool,
    /// What a button runs.
    pub action: Option<ControlAction>,
    /// Printed with the sheet.
    pub print: bool,
    /// Locked when the sheet is protected.
    pub locked: bool,
    /// Drawn with 3-D shading (check boxes and option buttons).
    pub three_d: bool,
    /// The items of a list box or drop-down as reference text (`$E$1:$E$4`, `Lists!$A$1:$A$3`).
    /// Structural edits rewrite it like `link`.
    pub input_range: Option<String>,
    /// The rows a drop-down shows when it is open.
    pub drop_lines: u32,
    /// How many items of a list box can be selected.
    pub selection: ListSelection,
}

impl Default for FormControl {
    fn default() -> Self {
        FormControl {
            id: 0,
            kind: ControlKind::default(),
            anchor: Anchor::default(),
            name: String::new(),
            text: String::new(),
            link: None,
            value: 0,
            min: 0,
            max: 100,
            step: 1,
            page: 10,
            horizontal: true,
            action: None,
            print: true,
            locked: true,
            three_d: true,
            input_range: None,
            drop_lines: DEFAULT_DROP_LINES,
            selection: ListSelection::Single,
        }
    }
}

impl FormControl {
    /// The name shown for the control: its own, else `Button 3`, `Slider 4`, `Spin Button 5`.
    pub fn display_name(&self) -> String {
        if self.name.trim().is_empty() { format!("{} {}", self.kind.label(), self.id) } else { self.name.clone() }
    }

    /// Checks the numbers of a slider or spin button: 0 ≤ min < max ≤ 30000, 1 ≤ step ≤ max − min,
    /// and (sliders) page ≥ step. Each rule has its own message. A check box's state is 0..=2 and
    /// an option button's 0..=1; their other numbers, and those of the other kinds, are ignored.
    pub fn check_numbers(&self) -> Result<(), String> {
        if let Some(top) = self.kind.max_state() {
            if self.value > top {
                return Err(format!("{} state must be from 0 to {top}.", self.kind.label()));
            }
            return Ok(());
        }
        if !self.kind.is_number() {
            return Ok(());
        }
        if self.max > CONTROL_MAX {
            return Err(format!("Maximum value can be at most {CONTROL_MAX}."));
        }
        if self.min > CONTROL_MAX {
            return Err(format!("Minimum value must be a whole number from 0 to {CONTROL_MAX}."));
        }
        if self.min >= self.max {
            return Err(format!("Maximum value ({}) must be greater than Minimum value ({}).", self.max, self.min));
        }
        if self.step < 1 {
            return Err("Incremental change must be at least 1.".into());
        }
        let span = self.max - self.min;
        if self.step > span {
            return Err(format!("Incremental change ({}) can be at most Maximum minus Minimum ({span}).", self.step));
        }
        if self.kind == ControlKind::Slider {
            if self.page < self.step {
                return Err(format!("Page change ({}) must be at least the incremental change ({}).", self.page, self.step));
            }
            if self.page > CONTROL_MAX {
                return Err(format!("Page change can be at most {CONTROL_MAX}."));
            }
        }
        Ok(())
    }

    /// `v` clamped to min..max and snapped to the nearest step from min (max stays reachable).
    pub fn snap(&self, v: f64) -> u32 {
        let (min, max) = (self.min.min(CONTROL_MAX), self.max.min(CONTROL_MAX).max(self.min.min(CONTROL_MAX)));
        if !v.is_finite() || v <= min as f64 {
            return min;
        }
        if v >= max as f64 {
            return max;
        }
        let step = self.step.max(1) as f64;
        let snapped = (min as f64 + ((v - min as f64) / step).round() * step).clamp(min as f64, max as f64);
        // When the step doesn't divide the range, the maximum is a stop of its own.
        if (max as f64 - v).abs() < (snapped - v).abs() { max } else { snapped as u32 }
    }
}

/// The kinds of floating object a sheet holds. Commands name an object by kind and
/// workbook-unique id (`{kind: "chart", id}`); [`ObjKind::name`] is that parameter's spelling.
/// Equations are stored as shapes ([`ShapeKind::Equation`]) but select, open and delete as their
/// own kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjKind {
    Chart,
    Image,
    Shape,
    Slicer,
    Equation,
    Control,
}

impl ObjKind {
    pub const ALL: [ObjKind; 6] = [ObjKind::Chart, ObjKind::Image, ObjKind::Shape, ObjKind::Slicer, ObjKind::Equation, ObjKind::Control];

    /// The name used in command parameters.
    pub fn name(self) -> &'static str {
        match self {
            ObjKind::Chart => "chart",
            ObjKind::Image => "image",
            ObjKind::Shape => "shape",
            ObjKind::Slicer => "slicer",
            ObjKind::Equation => "equation",
            ObjKind::Control => "control",
        }
    }

    /// The kind named `s` (any letter case).
    pub fn parse(s: &str) -> Option<ObjKind> {
        ObjKind::ALL.into_iter().find(|k| k.name().eq_ignore_ascii_case(s))
    }
}

/// One floating object on a sheet: its kind and its workbook-unique id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObjRef {
    pub kind: ObjKind,
    pub id: u32,
}

impl ObjRef {
    pub fn new(kind: ObjKind, id: u32) -> ObjRef {
        ObjRef { kind, id }
    }
}

/// Horizontal alignment of a shape's text (`a:pPr algn`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeTextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical anchor of a shape's text in its box (`a:bodyPr anchor`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeTextAnchor {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// How a shape's outline and text look. The text properties apply to the whole text (the first
/// paragraph's and first run's properties when a file has several).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShapeFormat {
    /// No outline (`a:ln/a:noFill`), whatever `Shape::line` says.
    pub no_line: bool,
    /// No fill (`a:noFill` in `xdr:spPr`), whatever `Shape::fill` says.
    pub no_fill: bool,
    /// Text size in points (`a:rPr sz` / 100); `None` is the default size.
    pub font_size: Option<f32>,
    pub bold: bool,
    pub italic: bool,
    /// Typeface (`a:latin typeface`); `None` is the default font.
    pub font: Option<String>,
    /// Text fill colour; `None` is the default (black in text boxes).
    pub text_color: Option<Color>,
    /// Text outline (`a:rPr/a:ln`): colour and width in points.
    pub text_outline: Option<(Color, f32)>,
    /// `None`: the shape's default (left for text boxes, centred for other shapes).
    pub align: Option<ShapeTextAlign>,
    /// `None`: the shape's default (top for text boxes, middle for other shapes).
    pub anchor: Option<ShapeTextAnchor>,
    /// Paragraphs run right to left (`a:pPr rtl`).
    pub rtl: bool,
    /// Text insets in EMU (`a:bodyPr lIns`, `tIns`, `rIns`, `bIns`), each `None` when the file
    /// leaves it to the default.
    pub inset_left: Option<i32>,
    pub inset_top: Option<i32>,
    pub inset_right: Option<i32>,
    pub inset_bottom: Option<i32>,
    /// Text wrapping (`a:bodyPr wrap`); `None` writes the usual square wrapping.
    pub wrap: Option<ShapeTextWrap>,
    /// Text direction (`a:bodyPr vert`); `None` is the default (horizontal).
    pub vert: Option<ShapeTextVert>,
    /// Rotation of the text in its box (`a:bodyPr rot`), in 60000ths of a degree.
    pub text_rotation: Option<i32>,
}

/// How a shape's text wraps (`a:bodyPr wrap`, ST_TextWrappingType).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeTextWrap {
    None,
    Square,
}

/// A shape's text direction (`a:bodyPr vert`, ST_TextVerticalType).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapeTextVert {
    Horizontal,
    Vertical,
    Vertical270,
    WordArtVertical,
    EastAsianVertical,
    MongolianVertical,
    WordArtVerticalRtl,
}

impl ShapeTextVert {
    /// The `vert` attribute value.
    pub fn xml(self) -> &'static str {
        match self {
            Self::Horizontal => "horz",
            Self::Vertical => "vert",
            Self::Vertical270 => "vert270",
            Self::WordArtVertical => "wordArtVert",
            Self::EastAsianVertical => "eaVert",
            Self::MongolianVertical => "mongolianVert",
            Self::WordArtVerticalRtl => "wordArtVertRtl",
        }
    }

    /// The value for a `vert` attribute; `None` for anything outside the schema's list.
    pub fn from_xml(s: &str) -> Option<Self> {
        Some(match s {
            "horz" => Self::Horizontal,
            "vert" => Self::Vertical,
            "vert270" => Self::Vertical270,
            "wordArtVert" => Self::WordArtVertical,
            "eaVert" => Self::EastAsianVertical,
            "mongolianVert" => Self::MongolianVertical,
            "wordArtVertRtl" => Self::WordArtVerticalRtl,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SparklineKind {
    #[default]
    Line,
    Column,
    WinLoss,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sparkline {
    pub cell: CellRef,
    pub source: String,
    pub kind: SparklineKind,
    pub color: Color,
    pub markers: bool,
}

// ---------------------------------------------------------------- filter & sort

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FilterCriterion {
    /// Show rows whose display text is one of these (case-insensitive); `blanks` includes empty.
    Values {
        values: Vec<String>,
        blanks: bool,
    },
    /// Custom: one or two comparisons (`op` like `>=`, `=*abc*`), joined by and/or.
    Custom {
        a: (String, String),
        b: Option<(String, String)>,
        and: bool,
    },
    Top10 {
        bottom: bool,
        percent: bool,
        count: u32,
    },
    AboveAverage(bool),
    FillColor(Color),
    FontColor(Color),
    /// Rows whose cell shows icon `id` (0-based; `None` = no icon) of conditional-format icon set
    /// `set` (`iconFilter`). Kept from files; re-applying the filter leaves those rows as they are.
    Icon {
        set: String,
        id: Option<u32>,
    },
    /// Values plus Excel's date-group ticks (`filters` with `dateGroupItem`).
    DateGroups {
        values: Vec<String>,
        blanks: bool,
        groups: Vec<DateGroup>,
    },
    /// A dynamic date filter other than above/below average (`dynamicFilter`), kept from files.
    /// Re-applying the filter leaves those rows as they are.
    DynamicDate {
        kind: String,
        val: Option<f64>,
        max_val: Option<f64>,
        val_iso: Option<String>,
        max_val_iso: Option<String>,
    },
}

/// One date-group tick of a values filter (`dateGroupItem`): the date fields down to `grouping`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DateGroup {
    pub year: u16,
    pub month: Option<u8>,
    pub day: Option<u8>,
    pub hour: Option<u8>,
    pub minute: Option<u8>,
    pub second: Option<u8>,
    pub grouping: DateGrouping,
}

/// How deep a [`DateGroup`] matches (`ST_DateTimeGrouping`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateGrouping {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    Second,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AutoFilter {
    pub range: RangeRef,
    /// Column offset within `range` → criterion.
    pub criteria: Vec<(u32, FilterCriterion)>,
}

/// A slicer: an on-sheet filter widget bound to one column of a table. Clicking its value tiles
/// sets that column's `FilterCriterion::Values` in the sheet's [`AutoFilter`] and re-applies it,
/// so a slicer is just a visual front-end over the existing filter machinery.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slicer {
    pub id: u32,
    /// Name of the table this slicer filters.
    pub table: String,
    /// Header name of the filtered column within the table.
    pub column: String,
    /// Caption shown in the slicer header (defaults to the column name).
    pub caption: String,
    pub anchor: Anchor,
    /// Selected display values. `None` means every value (no filter from this slicer); an empty
    /// list means nothing is selected (all rows hidden), matching Excel's "clear all buttons".
    pub selected: Option<Vec<String>>,
    /// Number of button columns in the layout (Excel default 1).
    pub columns: u32,
}

#[cfg(test)]
mod trendline_forecast_tests {
    use super::Trendline;

    #[test]
    fn clamp_forecast_bounds_hostile_values() {
        assert_eq!(Trendline::clamp_forecast(2.5), 2.5);
        assert_eq!(Trendline::clamp_forecast(0.0), 0.0);
        assert_eq!(Trendline::clamp_forecast(-4.0), 0.0);
        assert_eq!(Trendline::clamp_forecast(f64::NAN), 0.0);
        assert_eq!(Trendline::clamp_forecast(f64::INFINITY), 0.0);
        assert_eq!(Trendline::clamp_forecast(f64::NEG_INFINITY), 0.0);
        assert_eq!(Trendline::clamp_forecast(1e300), Trendline::MAX_FORECAST);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_box_and_option_button_states_are_checked_and_old_json_reads_shaded() {
        let c = |kind, value| FormControl { kind, value, min: 5, max: 1, ..FormControl::default() };
        for v in 0..=2 {
            assert_eq!(c(ControlKind::CheckBox, v).check_numbers(), Ok(()), "{v}");
        }
        assert!(c(ControlKind::CheckBox, 3).check_numbers().is_err());
        assert_eq!(c(ControlKind::OptionButton, 1).check_numbers(), Ok(()));
        assert!(c(ControlKind::OptionButton, 2).check_numbers().is_err());
        let old: FormControl = serde_json::from_str(r#"{"id":1,"kind":"button"}"#).unwrap();
        assert!(old.three_d);
        let check: FormControl = serde_json::from_str(r#"{"kind":"checkBox","value":2,"three_d":false}"#).unwrap();
        assert_eq!((check.kind, check.value, check.three_d), (ControlKind::CheckBox, 2, false));
        assert_eq!((ControlKind::CheckBox.label(), ControlKind::OptionButton.label()), ("Check Box", "Option Button"));
    }

    #[test]
    fn table_json_without_metadata_still_reads() {
        let json = r#"{"id":1,"name":"T","range":{"start":{"row":0,"col":0},"end":{"row":1,"col":0}},"header_row":true,"totals_row":false,"columns":[],"style":"","banded_rows":false,"banded_cols":false,"first_col":false,"last_col":false,"filter_button":false}"#;
        let t: Table = serde_json::from_str(json).expect("old table JSON");
        assert_eq!((t.comment, t.insert_row, t.insert_row_shift, t.published), (None, None, None, None));
    }

    #[test]
    fn a_comment_saved_before_note_shapes_still_reads() {
        let json = r#"{"author":"A","text":"t","replies":[],"threaded":false,"resolved":false,"visible":false}"#;
        let c: Comment = serde_json::from_str(json).unwrap();
        assert_eq!(c.note_shape, None);
        let s: NoteShape = serde_json::from_str("{}").unwrap();
        assert!(!s.fixed_position && !s.fixed_size && s.fill_color.is_none());
        let old: NoteShape = serde_json::from_str(r#"{"move_with_cells":true,"size_with_cells":false}"#).unwrap();
        assert!(old.fixed_position && !old.fixed_size);
    }

    #[test]
    fn chart_without_hole_size_keeps_the_legacy_hole() {
        let chart = Chart {
            rounded_corners: false,
            lang: None,
            of_pie: None,
            alt: String::new(),
            theme_override: None,
            hidden: false,
            no_print: false,
            id: 1,
            kind: ChartKind::Doughnut,
            anchor: Anchor::default(),
            title: None,
            series: vec![],
            legend: LegendPos::Right,
            data_labels: false,
            data_table: None,
            gridlines: false,
            style: 1,
            hole_size: 75,
            first_slice_angle: 0,
            x_title: None,
            y_title: None,
            source: None,
            by_rows: false,
            pivot: None,
            scale: Default::default(),
            view_3d: None,
        };
        let mut json = serde_json::to_value(&chart).expect("serialize chart");
        json.as_object_mut().expect("chart object").remove("hole_size");
        let legacy: Chart = serde_json::from_value(json).expect("deserialize legacy chart");
        assert_eq!(legacy.hole_size, 50, "documents saved before hole_size existed keep the old 50% hole");
        assert_eq!(chart.hole_size, 75, "new charts keep Excel's 75% default");
    }

    #[test]
    fn shape_text_body_properties_survive_json_and_old_json_reads() {
        let f = ShapeFormat {
            inset_left: Some(0),
            inset_bottom: Some(-45720),
            wrap: Some(ShapeTextWrap::None),
            vert: Some(ShapeTextVert::EastAsianVertical),
            text_rotation: Some(i32::MIN),
            ..ShapeFormat::default()
        };
        let json = serde_json::to_string(&f).expect("serialize format");
        assert_eq!(serde_json::from_str::<ShapeFormat>(&json).expect("deserialize format"), f);
        let old: ShapeFormat = serde_json::from_str(r#"{"bold":true,"rtl":true}"#).expect("old JSON");
        assert!(old.bold && old.rtl);
        assert_eq!((old.inset_left, old.wrap, old.vert, old.text_rotation), (None, None, None, None));
    }

    #[test]
    fn shape_text_vert_names_round_trip_and_reject_unknown_values() {
        for v in ["horz", "vert", "vert270", "wordArtVert", "eaVert", "mongolianVert", "wordArtVertRtl"] {
            assert_eq!(ShapeTextVert::from_xml(v).map(ShapeTextVert::xml), Some(v));
        }
        assert_eq!(ShapeTextVert::from_xml("Vert"), None);
        assert_eq!(ShapeTextVert::from_xml(""), None);
    }
}

#[cfg(test)]
mod equation_cache_tests {
    use super::{EquationCache, source_key};

    thread_local! {
        /// How many sources `source_key` has hashed on this thread.
        pub(super) static HASHES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn glyphs(c: &EquationCache, src: &str) -> Option<String> {
        let list = c.get(src)?.ok()?;
        Some(list.items.iter().filter_map(|i| if let gridcraft_math::Item::Glyph(g) = i { Some(g.ch) } else { None }).collect())
    }

    #[test]
    fn a_hash_collision_never_returns_another_sources_layout() {
        let (a, b) = ("x^2", "y^3");
        let mut c = EquationCache::default();
        assert!(c.refresh(a));
        assert_eq!(glyphs(&c, a).as_deref(), Some("x2"));
        // Forge a collision: the entry claims `b`'s hash; `b` has `a`'s length, so only the
        // full comparison tells them apart.
        c.key = source_key(b);
        assert!(c.get(b).is_none(), "`b` is a miss, not `a`'s layout");
        assert!(c.refresh(b), "`b` is laid out afresh");
        assert_eq!(glyphs(&c, b).as_deref(), Some("y3"));
        assert!(c.get(a).is_none(), "the entry is `b`'s now");
        // A refusal does not answer for a colliding source either.
        c.refuse(a, "not laid out");
        c.key = source_key(b);
        assert!(c.get(b).is_none());
    }

    #[test]
    fn sources_over_the_cap_are_not_copied_hashed_or_laid_out() {
        let long = "x".repeat(gridcraft_math::MAX_SOURCE_BYTES + 1);
        let mut c = EquationCache::default();
        HASHES.with(|h| h.set(0));
        assert!(matches!(c.get(&long), Some(Err(e)) if e.message.contains("4096")), "answered before any refresh");
        assert!(c.refresh(&long));
        assert!(c.source.is_none(), "an over-cap source is not kept");
        assert_eq!(c.key, 0, "nor hashed");
        assert!(matches!(c.get(&long), Some(Err(_))));
        assert!(!c.refresh(&long), "still cached");
        let other = "y".repeat(gridcraft_math::MAX_SOURCE_BYTES + 1);
        assert!(matches!(c.get(&other), Some(Err(_))), "another over-cap source gets the same answer");
        assert_eq!(HASHES.with(|h| h.get()), 0, "nothing over the cap is hashed");
        // Within the cap the lookup is unchanged: a hash, then the source compared in full.
        assert!(c.refresh("x"));
        assert!(c.get("x").is_some_and(|r| r.is_ok()));
        assert!(HASHES.with(|h| h.get()) >= 2);
    }

    #[test]
    #[allow(clippy::disallowed_methods)] // tests run only off wasm, where the clock exists
    fn a_source_of_a_megabyte_is_looked_up_without_being_read() {
        // The sheet painter asks the cache every frame and the PDF export every draw; a source
        // read from a file can be megabytes, and the lookup must not scan it each time.
        let big = "l".repeat(1 << 20);
        let mut c = EquationCache::default();
        c.refuse(&big, "budget");
        HASHES.with(|h| h.set(0));
        let start = std::time::Instant::now();
        for _ in 0..1000 {
            assert!(matches!(c.get(&big), Some(Err(_))));
        }
        let took = start.elapsed();
        assert_eq!(HASHES.with(|h| h.get()), 0, "a thousand lookups read nothing");
        assert!(took < std::time::Duration::from_millis(500), "{took:?}");
    }
}
