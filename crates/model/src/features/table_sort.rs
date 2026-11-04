//! A table's saved sort settings (`<sortState>`, ECMA-376 Part 1 §18.3.1.92). They're kept as
//! read so a file round-trips them; GridCraft doesn't evaluate them.

use gridcraft_core::RangeRef;
use serde::{Deserialize, Serialize};

use crate::style::Style;

/// Most sort conditions kept from a file (Excel allows 64 sort levels).
pub const MAX_SORT_CONDITIONS: usize = 64;
/// Longest custom list (characters) kept from a file.
pub const MAX_CUSTOM_LIST_CHARS: usize = 32_767;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TableSort {
    /// The range the sort applies to, as saved (it may extend past the table).
    pub range: RangeRef,
    /// The table's range when the sort was read: once the table is resized the sort is stale
    /// and isn't written back.
    pub table_range: RangeRef,
    /// Whether `<sortState>` sat inside the table's `<autoFilter>` rather than directly under
    /// `<table>`.
    pub in_autofilter: bool,
    pub column_sort: bool,
    pub case_sensitive: bool,
    /// `sortMethod` (`stroke`, `pinYin`); `None` for the default.
    pub method: Option<String>,
    /// Sort keys, in order.
    pub conditions: Vec<SortCondition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SortCondition {
    pub range: RangeRef,
    pub descending: bool,
    pub by: SortBy,
    /// Comma-separated custom sort order.
    pub custom_list: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum SortBy {
    Value,
    /// Cell or font colour; the format holding the colour (`dxfId`).
    CellColor(Option<Style>),
    FontColor(Option<Style>),
    /// `iconSet` (as written) and `iconId`.
    Icon {
        set: Option<String>,
        id: Option<u32>,
    },
}
