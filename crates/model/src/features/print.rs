use super::{CellRef, RangeRef};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrintSettings {
    pub orientation: Orientation,
    /// Paper size name (`Letter`, `A4`, `Legal`…).
    pub paper: String,
    /// Margins in inches: left, right, top, bottom, header, footer.
    pub margins: [f32; 6],
    /// The print area, or the first of its ranges when it has several.
    pub print_area: Option<RangeRef>,
    /// The print area's ranges after the first, in file order (Excel prints each range of a
    /// print area starting on a new page). Empty for a single-range print area.
    #[serde(default)]
    pub print_area_more: Vec<RangeRef>,
    pub title_rows: Option<(u32, u32)>,
    pub title_cols: Option<(u32, u32)>,
    /// Percent scale, or fit to pages (wide, tall).
    pub scale: u16,
    pub fit_to: Option<(u16, u16)>,
    pub gridlines: bool,
    pub headings: bool,
    pub center_h: bool,
    pub center_v: bool,
    /// The header and footer of odd pages, and of every page unless the variants below apply.
    pub header: String,
    pub footer: String,
    pub row_breaks: Vec<u32>,
    pub col_breaks: Vec<u32>,
    /// Different First Page: the sheet's first printed page uses `first_header`/`first_footer`.
    #[serde(default)]
    pub different_first: bool,
    /// Different Odd & Even Pages: even-numbered pages use `even_header`/`even_footer`.
    #[serde(default)]
    pub different_odd_even: bool,
    #[serde(default)]
    pub first_header: String,
    #[serde(default)]
    pub first_footer: String,
    #[serde(default)]
    pub even_header: String,
    #[serde(default)]
    pub even_footer: String,
    /// Scale with Document / Align with Page Margins (both on unless a file turned them off).
    #[serde(default = "yes")]
    pub hf_scale_with_doc: bool,
    #[serde(default = "yes")]
    pub hf_align_with_margins: bool,
    /// Page Setup › Sheet › Page order: the sequence pages are numbered and printed in when the
    /// sheet is more than one page wide and tall.
    #[serde(default)]
    pub page_order: PageOrder,
    /// Page Setup output options kept from a file (print-in-black-and-white, draft, comments and
    /// errors policy, first page number, printer resolution, copies). Each is unset unless stated.
    #[serde(default, skip_serializing_if = "PrintOutput::is_empty")]
    pub output: PrintOutput,
}

/// Supplemental `pageSetup` attributes (ECMA-376 `CT_PageSetup`). Every field is `None` unless
/// the file stated it, so explicit `false` and `0` survive a save as written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrintOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub black_and_white: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_comments: Option<PrintComments>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<PrintErrors>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_page_number: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_first_page_number: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_dpi: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_dpi: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub copies: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub use_printer_defaults: Option<bool>,
}

impl PrintOutput {
    pub fn is_empty(&self) -> bool {
        *self == PrintOutput::default()
    }
}

/// How cell comments print (`ST_CellComments`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PrintComments {
    None,
    AsDisplayed,
    AtEnd,
}

impl PrintComments {
    pub fn id(self) -> &'static str {
        match self {
            PrintComments::None => "none",
            PrintComments::AsDisplayed => "asDisplayed",
            PrintComments::AtEnd => "atEnd",
        }
    }
    pub fn parse(id: &str) -> Option<PrintComments> {
        match id {
            "none" => Some(PrintComments::None),
            "asDisplayed" => Some(PrintComments::AsDisplayed),
            "atEnd" => Some(PrintComments::AtEnd),
            _ => None,
        }
    }
}

/// How error values print (`ST_PrintError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PrintErrors {
    #[serde(rename = "displayed")]
    Displayed,
    #[serde(rename = "blank")]
    Blank,
    #[serde(rename = "dash")]
    Dash,
    #[serde(rename = "NA")]
    Na,
}

impl PrintErrors {
    pub fn id(self) -> &'static str {
        match self {
            PrintErrors::Displayed => "displayed",
            PrintErrors::Blank => "blank",
            PrintErrors::Dash => "dash",
            PrintErrors::Na => "NA",
        }
    }
    pub fn parse(id: &str) -> Option<PrintErrors> {
        match id {
            "displayed" => Some(PrintErrors::Displayed),
            "blank" => Some(PrintErrors::Blank),
            "dash" => Some(PrintErrors::Dash),
            "NA" => Some(PrintErrors::Na),
            _ => None,
        }
    }
}

fn yes() -> bool {
    true
}

/// The sequence of a sheet's pages (XLSX `pageSetup pageOrder`, ECMA-376 `ST_PageOrder`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PageOrder {
    /// Down the first column of pages, then down the next one (Excel's default).
    #[default]
    DownThenOver,
    /// Across the first row of pages, then across the next one.
    OverThenDown,
}

impl PageOrder {
    /// The id `pageLayout.pageSetup` takes and XLSX writes.
    pub fn id(self) -> &'static str {
        match self {
            PageOrder::DownThenOver => "downThenOver",
            PageOrder::OverThenDown => "overThenDown",
        }
    }
    pub fn parse(id: &str) -> Option<PageOrder> {
        match id {
            "downThenOver" => Some(PageOrder::DownThenOver),
            "overThenDown" => Some(PageOrder::OverThenDown),
            _ => None,
        }
    }
    /// The 0-based place in the printed sequence of the page in page row `row` and page column
    /// `col`, on a sheet `rows` pages tall and `cols` pages wide.
    pub fn index(self, row: usize, col: usize, rows: usize, cols: usize) -> usize {
        match self {
            PageOrder::DownThenOver => col.saturating_mul(rows).saturating_add(row),
            PageOrder::OverThenDown => row.saturating_mul(cols).saturating_add(col),
        }
    }
}

/// Which header/footer pair a printed page uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HfPage {
    Odd,
    Even,
    First,
}

impl HfPage {
    pub fn id(self) -> &'static str {
        match self {
            HfPage::Odd => "odd",
            HfPage::Even => "even",
            HfPage::First => "first",
        }
    }
    pub fn parse(id: &str) -> Option<HfPage> {
        match id {
            "odd" | "all" => Some(HfPage::Odd),
            "even" => Some(HfPage::Even),
            "first" => Some(HfPage::First),
            _ => None,
        }
    }
}

impl PrintSettings {
    /// The pair used by page `number` (1-based, as `&P` prints it); `first` is the sheet's first page.
    pub fn hf_page(&self, first: bool, number: usize) -> HfPage {
        if self.different_first && first {
            HfPage::First
        } else if self.different_odd_even && number.is_multiple_of(2) {
            HfPage::Even
        } else {
            HfPage::Odd
        }
    }
    /// The header (`footer` false) or footer of a page kind.
    pub fn hf(&self, page: HfPage, footer: bool) -> &String {
        match (page, footer) {
            (HfPage::Odd, false) => &self.header,
            (HfPage::Odd, true) => &self.footer,
            (HfPage::Even, false) => &self.even_header,
            (HfPage::Even, true) => &self.even_footer,
            (HfPage::First, false) => &self.first_header,
            (HfPage::First, true) => &self.first_footer,
        }
    }
    pub fn hf_mut(&mut self, page: HfPage, footer: bool) -> &mut String {
        match (page, footer) {
            (HfPage::Odd, false) => &mut self.header,
            (HfPage::Odd, true) => &mut self.footer,
            (HfPage::Even, false) => &mut self.even_header,
            (HfPage::Even, true) => &mut self.even_footer,
            (HfPage::First, false) => &mut self.first_header,
            (HfPage::First, true) => &mut self.first_footer,
        }
    }
    /// Any header or footer text at all (on any kind of page).
    pub fn has_header_footer(&self) -> bool {
        [&self.header, &self.footer, &self.first_header, &self.first_footer, &self.even_header, &self.even_footer].iter().any(|t| !t.is_empty())
    }
    /// Every range of the print area in order (empty when there is none).
    pub fn print_areas(&self) -> Vec<RangeRef> {
        self.print_area.iter().chain(self.print_area_more.iter()).copied().collect()
    }
    /// Sets the print area to `areas` in order: the first is `print_area`, the rest
    /// `print_area_more`. An empty list clears it.
    pub fn set_print_areas(&mut self, areas: Vec<RangeRef>) {
        let mut it = areas.into_iter();
        self.print_area = it.next();
        self.print_area_more = it.collect();
    }
}

impl Default for PrintSettings {
    fn default() -> Self {
        PrintSettings {
            orientation: Orientation::Portrait,
            paper: "Letter".into(),
            margins: [0.7, 0.7, 0.75, 0.75, 0.3, 0.3],
            print_area: None,
            print_area_more: Vec::new(),
            title_rows: None,
            title_cols: None,
            scale: 100,
            fit_to: None,
            gridlines: false,
            headings: false,
            center_h: false,
            center_v: false,
            header: String::new(),
            footer: String::new(),
            row_breaks: vec![],
            col_breaks: vec![],
            different_first: false,
            different_odd_even: false,
            first_header: String::new(),
            first_footer: String::new(),
            even_header: String::new(),
            even_footer: String::new(),
            hf_scale_with_doc: true,
            hf_align_with_margins: true,
            page_order: PageOrder::DownThenOver,
            output: PrintOutput::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SheetProtection {
    /// Hash of the password (Excel-compatible, written to XLSX `sheetProtection`; never the
    /// password itself).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<crate::PasswordHash>,
    /// The internal hash older GridCraft JSON workbooks stored ([`password_hash`]); still
    /// verified when unprotecting, never written for new passwords.
    #[serde(rename = "password_hash", default, skip_serializing_if = "Option::is_none")]
    pub legacy_password_hash: Option<u64>,
    pub select_locked: bool,
    pub select_unlocked: bool,
    pub format_cells: bool,
    pub format_columns: bool,
    pub format_rows: bool,
    pub insert_columns: bool,
    pub insert_rows: bool,
    pub delete_columns: bool,
    pub delete_rows: bool,
    pub sort: bool,
    pub autofilter: bool,
    /// Allowed actions (the inverse of XLSX protection bits).
    #[serde(default)]
    pub objects: bool,
    #[serde(default)]
    pub scenarios: bool,
    #[serde(default)]
    pub insert_hyperlinks: bool,
    #[serde(default)]
    pub pivot_tables: bool,
}

/// Review › Allow Edit Ranges: cells that stay editable on a protected sheet, with or without a
/// password (XLSX `<protectedRange>`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditRange {
    pub title: String,
    pub ranges: Vec<RangeRef>,
    pub password: Option<crate::PasswordHash>,
    /// Windows permissions (`securityDescriptor`), kept as read from the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub security_descriptor: Option<String>,
}

impl EditRange {
    pub fn contains(&self, c: CellRef) -> bool {
        self.ranges.iter().any(|r| r.contains(c))
    }
}

/// The non-cryptographic password hash older GridCraft versions stored for sheet protection
/// (only verified now, see [`SheetProtection::legacy_password_hash`]).
pub fn password_hash(pw: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325 ^ 0x5348_4545_5443;
    for b in pw.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Outline grouping info for a row or column.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LineInfo {
    /// Size in pixels at 96 dpi; `None` = default (rows auto-fit to content).
    pub size: Option<f32>,
    pub hidden: bool,
    pub outline: u8,
    pub collapsed: bool,
    pub style: Option<crate::style::StyleId>,
    /// The size was set by the user (rows): automatic heights follow the content.
    #[serde(default)]
    pub custom: bool,
}

/// What printing needs of a chart sheet (`xl/chartsheets/sheetN.xml`) read from a file. Chart
/// sheets aren't modelled as sheets: their parts are kept and written back unchanged (see
/// [`crate::PreservedSheet`]), and this read-only copy lets Print Entire Workbook print each
/// visible one as a page holding its chart, like Excel.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartSheetPrint {
    pub name: String,
    pub visibility: crate::Visibility,
    /// How many worksheets come before it: its position among the model's sheets.
    pub before: usize,
    pub chart: crate::Chart,
    /// Orientation, paper, margins and header/footer from the chart sheet's page setup.
    pub print: PrintSettings,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_order_numbers_pages_and_old_settings_load_down_then_over() {
        // Two pages tall, three wide: (row, col) → place in the printed sequence.
        let place = |order: PageOrder| [(0, 0), (1, 0), (0, 1), (1, 1), (0, 2), (1, 2)].map(|(r, c)| order.index(r, c, 2, 3));
        assert_eq!(place(PageOrder::DownThenOver), [0, 1, 2, 3, 4, 5]);
        assert_eq!(place(PageOrder::OverThenDown), [0, 3, 1, 4, 2, 5]);
        assert_eq!(PageOrder::OverThenDown.index(usize::MAX, usize::MAX, usize::MAX, usize::MAX), usize::MAX);
        for order in [PageOrder::DownThenOver, PageOrder::OverThenDown] {
            assert_eq!(PageOrder::parse(order.id()), Some(order));
        }
        assert_eq!(PageOrder::parse("OverThenDown"), None);
        // Saved under the same id the command takes; settings saved before the field existed load
        // with Excel's default.
        let mut p = PrintSettings { page_order: PageOrder::OverThenDown, ..Default::default() };
        let mut saved = serde_json::to_value(&p).unwrap();
        assert_eq!(saved["page_order"], "overThenDown");
        assert!(saved.as_object_mut().unwrap().remove("page_order").is_some());
        p = serde_json::from_value(saved).unwrap();
        assert_eq!(p, PrintSettings::default());
    }

    #[test]
    fn output_options_round_trip_json_and_old_json_loads_unset() {
        let output = PrintOutput {
            black_and_white: Some(false),
            cell_comments: Some(PrintComments::AtEnd),
            errors: Some(PrintErrors::Na),
            first_page_number: Some(u32::MAX),
            copies: Some(0),
            ..Default::default()
        };
        let p = PrintSettings { output, ..Default::default() };
        let saved = serde_json::to_value(&p).unwrap();
        assert_eq!(saved["output"]["errors"], "NA");
        assert_eq!(saved["output"]["cell_comments"], "atEnd");
        assert_eq!(serde_json::from_value::<PrintSettings>(saved).unwrap(), p);
        let plain = serde_json::to_value(PrintSettings::default()).unwrap();
        assert!(plain.get("output").is_none());
        assert_eq!(serde_json::from_value::<PrintSettings>(plain).unwrap().output, PrintOutput::default());
        for e in [PrintErrors::Displayed, PrintErrors::Blank, PrintErrors::Dash, PrintErrors::Na] {
            assert_eq!(PrintErrors::parse(e.id()), Some(e));
        }
        for c in [PrintComments::None, PrintComments::AsDisplayed, PrintComments::AtEnd] {
            assert_eq!(PrintComments::parse(c.id()), Some(c));
        }
        assert_eq!(PrintErrors::parse("na"), None);
    }
}
