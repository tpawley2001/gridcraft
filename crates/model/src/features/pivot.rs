use super::{CellRef, RangeRef};
use crate::style::Style;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- pivot tables

/// How a PivotTable lays out its row fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotLayout {
    /// All row fields in one column, inner items indented ("Row Labels" header).
    #[default]
    Compact,
    /// One column per row field; each item on its own row.
    Outline,
    /// One column per row field; inner items start on their parent's row, totals below.
    Tabular,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotSort {
    #[default]
    Asc,
    Desc,
    /// Source order (first appearance).
    None,
}

/// Grouping of a date field (values are date serials).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotDateGroup {
    #[default]
    None,
    Years,
    Quarters,
    Months,
    Days,
}

/// A field placed in the Rows or Columns area.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotField {
    /// Source column header name.
    pub source_col: String,
    pub sort: PivotSort,
    pub date_group: PivotDateGroup,
    /// Item labels whose detail is hidden (collapsed).
    pub collapsed_items: Vec<String>,
}

/// Summarize-values-by function.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotFunc {
    #[default]
    Sum,
    Count,
    Average,
    Max,
    Min,
    Product,
    CountNumbers,
    StdDev,
    StdDevP,
    Var,
    VarP,
}

/// Show-values-as transform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotShowAs {
    #[default]
    Normal,
    PercentOfGrandTotal,
    PercentOfColumnTotal,
    PercentOfRowTotal,
    /// Running total in the first row field.
    RunningTotal,
    /// Rank largest to smallest among sibling row items.
    Rank,
}

/// A field in the Values area.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotValue {
    pub source_col: String,
    pub func: PivotFunc,
    /// Caption, e.g. "Sum of Sales".
    pub name: String,
    pub show_as: PivotShowAs,
    pub number_format: Option<String>,
}

/// A report filter (or, for a field also in Rows/Columns, its hidden-items filter).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotFilter {
    pub source_col: String,
    /// Item labels shown; `None` = all.
    pub selected: Option<Vec<String>>,
}

/// A PivotTable: its definition plus where its output was last written. The output itself is
/// static cell values and styles on the sheet, rewritten on every refresh.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotTable {
    pub id: u32,
    pub name: String,
    /// Source: range formula text (`Sheet1!$A$1:$E$200`), a table name or a defined name.
    pub source: String,
    /// Top-left of the output body on the pivot's sheet.
    pub anchor: CellRef,
    pub rows: Vec<PivotField>,
    pub columns: Vec<PivotField>,
    pub values: Vec<PivotValue>,
    pub filters: Vec<PivotFilter>,
    pub layout: PivotLayout,
    /// PivotTable Options › Data › Enable show details: Show Details lists a value cell's
    /// source records only while this is on.
    pub enable_show_details: bool,
    /// Grand total for each row (the "Grand Total" column), like Excel's "On for Rows".
    pub grand_totals_rows: bool,
    /// Grand total for each column (the "Grand Total" row), like Excel's "On for Columns".
    pub grand_totals_cols: bool,
    pub subtotals: bool,
    /// Subtotals at the top of each group (Compact/Outline); otherwise below it.
    pub subtotals_top: bool,
    pub style: String,
    pub show_headers: bool,
    /// Area written by the last refresh.
    pub last_range: Option<RangeRef>,
    /// Calculated fields, usable in the Values area by name.
    pub calculated_fields: Vec<PivotCalcField>,
    /// The Σ Values pseudo-field (it exists with two or more value fields) sits in Rows rather
    /// than Columns (Excel's "values on rows").
    pub values_on_rows: bool,
    /// Position of Σ Values among the row fields (`None` = after the last). Only used with
    /// `values_on_rows`: in Columns Σ Values always comes after the last column field.
    pub values_position: Option<usize>,
    /// The PivotTable's own formatting (`formats`, ECMA-376 Part 1 §18.10.1.40/41), in file order.
    pub formats: Vec<PivotFormat>,
}

/// One PivotTable format: a differential style applied to a pivot area.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotFormat {
    /// The differential format (`dxf`) it applies; `None` when the format names none.
    pub style: Option<Style>,
    /// `action`: `formatting` (the default) or `blank`.
    pub action: String,
    pub area: PivotArea,
}

impl Default for PivotFormat {
    fn default() -> Self {
        PivotFormat { style: None, action: "formatting".into(), area: PivotArea::default() }
    }
}

/// A field a pivot area names, kept by name so it survives a rebuilt cache.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotAreaField {
    /// The Σ Values (data) field (index `-2` in the file).
    Data,
    /// A cache field, by its name (the source column header for a database field).
    Name(String),
}

/// An item of a pivot area reference (`x v`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PivotAreaItem {
    /// A cache shared item, by its caption.
    Label(String),
    /// A position: into the value fields for the data field, or into the field's items for a
    /// `byPosition` reference.
    Index(u32),
}

/// A part of a PivotTable (`pivotArea`, ECMA-376 Part 1 §18.3.1.68).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotArea {
    pub field: Option<PivotAreaField>,
    /// `type`: `none`, `normal` (the default), `data`, `all`, `origin`, `button`, `topEnd`, `topRight`.
    pub kind: String,
    pub data_only: bool,
    pub label_only: bool,
    pub grand_row: bool,
    pub grand_col: bool,
    pub cache_index: bool,
    pub outline: bool,
    /// `offset`: a cell range relative to the area.
    pub offset: Option<String>,
    pub collapsed_levels_are_subtotals: bool,
    /// `axis`: `axisRow`, `axisCol`, `axisPage` or `axisValues`.
    pub axis: Option<String>,
    pub field_position: Option<u32>,
    pub references: Vec<PivotAreaReference>,
}

impl Default for PivotArea {
    fn default() -> Self {
        PivotArea {
            field: None,
            kind: "normal".into(),
            data_only: true,
            label_only: false,
            grand_row: false,
            grand_col: false,
            cache_index: false,
            outline: true,
            offset: None,
            collapsed_levels_are_subtotals: false,
            axis: None,
            field_position: None,
            references: vec![],
        }
    }
}

/// A `reference` of a pivot area (§18.3.1.75): the items of one field it covers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotAreaReference {
    pub field: Option<PivotAreaField>,
    pub items: Vec<PivotAreaItem>,
    pub selected: bool,
    pub by_position: bool,
    pub relative: bool,
    /// The subtotal flags set on it (`defaultSubtotal`, `sumSubtotal`, `countASubtotal`…), by
    /// attribute name.
    pub subtotals: Vec<String>,
}

impl Default for PivotAreaReference {
    fn default() -> Self {
        PivotAreaReference { field: None, items: vec![], selected: true, by_position: false, relative: false, subtotals: vec![] }
    }
}

/// A calculated field: a formula over other fields (`=Sales*0.1`, `='Unit Price'*Units`). As in
/// Excel, it is evaluated on the sums of the fields it names for each report cell, not per record.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PivotCalcField {
    pub name: String,
    /// Formula text, starting with `=`.
    pub formula: String,
}

impl Default for PivotTable {
    fn default() -> Self {
        PivotTable {
            id: 0,
            name: "PivotTable1".into(),
            source: String::new(),
            anchor: CellRef::default(),
            rows: vec![],
            columns: vec![],
            values: vec![],
            filters: vec![],
            layout: PivotLayout::Compact,
            enable_show_details: true,
            grand_totals_rows: true,
            grand_totals_cols: true,
            subtotals: true,
            subtotals_top: true,
            style: "PivotStyleLight16".into(),
            show_headers: true,
            last_range: None,
            calculated_fields: vec![],
            values_on_rows: false,
            values_position: None,
            formats: vec![],
        }
    }
}
