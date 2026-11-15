//! Excel probe 1114n omits nv_font/nv_numfmt from whole-workbook printing, but keeps
//! sheets with a fill, border or merge. Build our own workbooks; parse saved XML through
//! the XLSX reader and assert the model/print JSON, never raw XML spelling.
use gridcraft_core::CellRef;
use gridcraft_engine::{Session, cmd::print::print_ranges};
use serde_json::{Value as Json, json};

fn decorated(bold: bool, numfmt: bool) -> Session {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("sheet.rename", json!({"name": "Decorative"})).unwrap();
    for cell in ["A1", "Z200"] {
        if bold {
            s.execute("home.bold", json!({"range": cell, "on": true})).unwrap();
        }
        if numfmt {
            s.execute("home.numberFormat", json!({"range": cell, "code": "0.00"})).unwrap();
        }
    }
    s
}

fn with_data(mut s: Session) -> Session {
    s.execute("home.insertSheet", json!({"name": "Data"})).unwrap();
    s.execute("sheet.activate", json!({"sheet": "Data"})).unwrap();
    s.execute("range.setValues", json!({"range": "A1", "values": [[1]]})).unwrap();
    s
}

fn saved(s: &mut Session) -> Vec<u8> {
    let out = s.execute("file.saveBytes", json!({"format": "xlsx"})).unwrap();
    gridcraft_engine::io::base64_decode(out["base64"].as_str().unwrap()).unwrap()
}

fn reopened(bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "synthetic.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn page_list(s: &mut Session) -> Vec<Json> {
    let p = s.execute("file.printPreview", json!({"sheets": "all"})).unwrap();
    let list = p["pageList"].as_array().unwrap().clone();
    assert_eq!(p["pages"], list.len());
    list
}

fn assert_only_data(s: &mut Session) {
    let list = page_list(s);
    assert_eq!(list.len(), 1, "nonvisual sheet must not add blank pages: {list:?}");
    assert_eq!(list[0]["sheet"], "Data");
    assert_eq!(list[0]["columns"], "A:A");
    assert_eq!(list[0]["rows"], "1:1");
    let pdf = s.execute("file.exportPdf", json!({"sheets": "all"})).unwrap();
    assert_eq!(pdf["pages"], 1);
}

fn skips_nonvisual_sheet(bold: bool, numfmt: bool) {
    let mut s = with_data(decorated(bold, numfmt));
    let bytes = saved(&mut s);
    // read_xlsx parses the worksheet and styles XML. Checking the resulting model prevents
    // a bogus fix that removes formatting/cells during saving or opening.
    let (wb, _) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    let sh = wb.sheet(wb.sheet_index("Decorative").unwrap()).unwrap();
    for at in [CellRef::new(0, 0), CellRef::new(199, 25)] {
        let c = sh.cells.get(at).unwrap();
        assert!(!c.has_content());
        let style = wb.styles.get(c.style);
        assert_eq!(style.font.bold, bold);
        assert_eq!(style.num_fmt.as_str(), if numfmt { "0.00" } else { "General" });
    }
    assert!(print_ranges(&wb, sh).is_empty(), "no automatic print range for nonvisual-only cells");
    assert_only_data(&mut s);
    assert_only_data(&mut reopened(&bytes));
}

#[test]
fn font_only_sheet_adds_no_page_and_keeps_its_styles() {
    skips_nonvisual_sheet(true, false);
}

#[test]
fn number_format_only_sheet_adds_no_page_and_keeps_its_styles() {
    skips_nonvisual_sheet(false, true);
}

#[test]
fn combined_nonvisual_formats_do_not_make_a_sheet_printable() {
    skips_nonvisual_sheet(true, true);
}

fn visible_guard(cmd: &str, params: Json, columns: &str, rows: &str) {
    let mut s = decorated(true, false);
    s.execute(cmd, params).unwrap();
    let mut s = with_data(s);
    let bytes = saved(&mut s);
    for mut book in [s, reopened(&bytes)] {
        let list = page_list(&mut book);
        assert_eq!(list.len(), 2, "visible empty-sheet formatting must still print: {list:?}");
        let p = list.iter().find(|p| p["sheet"] == "Decorative").unwrap();
        assert_eq!(p["columns"], columns);
        assert_eq!(p["rows"], rows);
        assert_eq!(book.execute("file.exportPdf", json!({"sheets": "all"})).unwrap()["pages"], 2);
    }
}

#[test]
fn a_fill_keeps_the_valueless_sheet_printable() {
    visible_guard("home.fillColor", json!({"range": "A1", "color": "#FFFF00"}), "A:A", "1:1");
}

#[test]
fn a_border_keeps_the_valueless_sheet_printable() {
    visible_guard("home.borders", json!({"range": "A1", "preset": "bottom"}), "A:A", "1:1");
}

#[test]
fn a_merge_keeps_the_valueless_sheet_printable() {
    visible_guard("home.mergeCells", json!({"range": "A1:B2"}), "A:B", "1:2");
}
