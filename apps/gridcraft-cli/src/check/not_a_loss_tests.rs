//! Two round-trip differences the check used to report that are not losses:
//! - names that call Excel 4.0 macro functions are left out of macro-free saves on purpose (Excel
//!   refuses them there), so a workbook holding such names is round-tripped macro-enabled;
//! - a shared-string table whose strings no cell refers to carries nothing: the writer builds the
//!   table from the cells, and Excel's own save drops such strings too.

use std::io::Write as _;

use super::fidelity::round_trip_name;
use super::package::relationship_kinds;
use super::*;

const WB_RELS: &str = "xl/_rels/workbook.xml.rels";
const SST: &str = "xl/sharedStrings.xml";
/// A relationship kind GridCraft doesn't know: it stays lost, so the comparison is known to run.
const MADE_UP: &str = r#"<Relationship Id="rId90" Type="http://schemas.microsoft.com/office/2099/relationships/madeUp" Target="madeUp.xml"/>"#;
const MADE_UP_PART: (&str, &str) = ("xl/madeUp.xml", "<madeUp><item/></madeUp>");

fn name(name: &str, formula: &str) -> gridcraft_engine::model::DefinedName {
    gridcraft_engine::model::DefinedName { name: name.into(), scope: None, formula: formula.into(), comment: String::new(), hidden: true }
}

/// A one-sheet workbook: the given cells, and with `macro_names` a name calling an Excel 4.0 macro
/// function plus a name that only uses that name.
fn book(cells: &[(&str, Value)], macro_names: bool) -> Workbook {
    let mut wb = Workbook::new();
    let sh = wb.sheet_mut(0).unwrap();
    for (a1, value) in cells {
        sh.set_cell(CellRef::parse(a1).unwrap(), Cell::value(value.clone()));
    }
    wb.names.push(name("Plain", "SUM(1,2)"));
    if macro_names {
        wb.names.push(name("Macro", "INDEX(GET.WORKSPACE(37),17)"));
        wb.names.push(name("User", "REPT(Macro,2)"));
    }
    wb
}

/// `bytes` with the body of the parts in `replace` swapped (each must exist), the made-up
/// relationship added to the workbook's rels and its part added.
fn rebuilt(bytes: &[u8], replace: &[(&str, &str)]) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut replaced = 0;
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let part = f.name().to_string();
        let mut body = Vec::new();
        f.read_to_end(&mut body).unwrap();
        if let Some((_, new)) = replace.iter().find(|(n, _)| *n == part) {
            body = new.as_bytes().to_vec();
            replaced += 1;
        }
        if part == WB_RELS {
            body = String::from_utf8(body).unwrap().replace("</Relationships>", &format!("{MADE_UP}</Relationships>")).into_bytes();
        }
        out.start_file(part, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&body).unwrap();
    }
    assert_eq!(replaced, replace.len(), "every part to replace is in the package");
    out.start_file(MADE_UP_PART.0, zip::write::SimpleFileOptions::default()).unwrap();
    out.write_all(MADE_UP_PART.1.as_bytes()).unwrap();
    out.finish().unwrap().into_inner()
}

fn sst(items: &[&str]) -> String {
    let body: String = items.iter().map(|t| format!("<si><t>{t}</t></si>")).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{0}" uniqueCount="{0}">{body}</sst>"#,
        items.len()
    )
}

fn examples(name: &str, source: &[u8], category: &str) -> Vec<String> {
    let r = check_bytes(name, source, &Options::default());
    assert!(r.error.is_none(), "{name} opens: {:?}", r.error);
    r.round_trip.categories.iter().find(|c| c.name == category).unwrap().examples.clone()
}

// ---------------------------------------------------------------- Excel 4.0 macro names

#[test]
fn not_a_loss_macro_names_pick_a_macro_enabled_round_trip() {
    let (with, without) = (book(&[("A1", Value::number(1.0))], true), book(&[("A1", Value::number(1.0))], false));
    // (source name, has names calling Excel 4.0 macro functions, the name the round trip is saved under)
    let table: &[(&str, bool, &str)] = &[
        ("book.xls", true, "roundtrip.xlsm"),
        ("BOOK.XLS", true, "roundtrip.xlsm"),
        ("book.xlsx", true, "roundtrip.xlsm"),
        ("book.xlsb", true, "roundtrip.xlsm"),
        ("book.ods", true, "roundtrip.xlsm"),
        ("book", true, "roundtrip.xlsm"),
        ("book.xlsm", true, "roundtrip.xlsm"),
        ("book.xltm", true, "roundtrip.xltm"),
        ("book.xltx", true, "roundtrip.xltm"),
        ("book.xls", false, "roundtrip.xlsx"),
        ("book.xlsx", false, "roundtrip.xlsx"),
        ("book.xltx", false, "roundtrip.xltx"),
    ];
    let wrong: Vec<String> = table
        .iter()
        .filter_map(|&(name, macros, want)| {
            let got = round_trip_name(name, if macros { &with } else { &without });
            (got != want).then(|| format!("{name} (macro names {macros}): want {want}, got {got}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn not_a_loss_macro_names_survive_the_round_trip() {
    // The names are in the source (a macro-enabled package, whatever it is called); the check must
    // not save it macro-free and then report the names the macro-free writer leaves out.
    let source = gridcraft_engine::io::save_bytes(&book(&[("A1", Value::number(1.0))], true), "t.xlsm").unwrap();
    for file in ["t.xlsx", "t.xlsm"] {
        assert_eq!(examples(file, &source, "names"), Vec::<String>::new(), "{file}");
    }
}

#[test]
fn not_a_loss_plain_names_keep_the_macro_free_round_trip() {
    let wb = book(&[("A1", Value::number(1.0))], false);
    assert_eq!(round_trip_name("book.xls", &wb), "roundtrip.xlsx");
    let source = gridcraft_engine::io::save_bytes(&wb, "t.xlsx").unwrap();
    assert_eq!(examples("t.xlsx", &source, "names"), Vec::<String>::new());
    assert_eq!(examples("t.xlsx", &rebuilt(&source, &[]), "package"), ["lost relationship madeUp"]);
}

// ---------------------------------------------------------------- unused shared strings

#[test]
fn not_a_loss_shared_strings_no_cell_uses() {
    // (what the sheet holds, cells)
    let sheets: [(&str, &[(&str, Value)]); 2] = [("a number", &[("A1", Value::number(1.0))]), ("no cells", &[])];
    for (what, cells) in sheets {
        let saved = gridcraft_engine::io::save_bytes(&book(cells, false), "t.xlsx").unwrap();
        // A table with two strings although no cell refers to one, as some libraries write it.
        let source = rebuilt(&saved, &[(SST, &sst(&["spare", "other"]))]);
        assert!(!relationship_kinds(&source).unwrap().contains_key("sharedstrings"), "{what}: the unused table carries nothing");
        assert_eq!(examples("t.xlsx", &source, "package"), ["lost relationship madeUp"], "{what}");
    }
}

#[test]
fn not_a_loss_shared_strings_a_cell_uses_still_count() {
    let saved = gridcraft_engine::io::save_bytes(&book(&[("A1", Value::number(1.0)), ("A2", Value::text("used"))], false), "t.xlsx").unwrap();
    // The writer's own table holds "used" at index 0; one more string after it changes no cell.
    let source = rebuilt(&saved, &[(SST, &sst(&["used", "spare"]))]);
    assert!(relationship_kinds(&source).unwrap().contains_key("sharedstrings"), "a table a cell refers to is content");
    assert_eq!(examples("t.xlsx", &source, "package"), ["lost relationship madeUp"]);
    assert_eq!(examples("t.xlsx", &source, "values"), Vec::<String>::new());
}
