//! The round trip of a workbook that carries a VBA project is saved as `.xlsm` whatever the source
//! was named (`.xlsb` with macros, a macro workbook named `.xlsx`): an `.xlsx` save drops the project
//! by design, which is not a loss of the file being checked. The `.xlsb` sheet index
//! (`xlBinaryIndex`) is a lookup cache with no SpreadsheetML counterpart, so it is not a loss either.

use std::io::Write as _;
use std::sync::Arc;

use super::fidelity::round_trip_name;
use super::*;

const WB_RELS: &str = "xl/_rels/workbook.xml.rels";
const SHEET_RELS: &str = "xl/worksheets/_rels/sheet1.xml.rels";
/// A relationship kind GridCraft doesn't know: it stays lost, so the comparison is known to run.
const MADE_UP: &str = r#"<Relationship Id="rId90" Type="http://schemas.microsoft.com/office/2099/relationships/madeUp" Target="madeUp.xml"/>"#;
const MADE_UP_PART: (&str, &str) = ("xl/madeUp.xml", "<madeUp><item/></madeUp>");
const BINARY_INDEX: &str =
    r#"<Relationship Id="rId91" Type="http://schemas.microsoft.com/office/2006/relationships/xlBinaryIndex" Target="binaryIndex1.bin"/>"#;

fn book(vba: bool) -> Workbook {
    let mut wb = Workbook::new();
    wb.sheet_mut(0).unwrap().set_cell(CellRef::parse("A1").unwrap(), Cell::value(Value::number(1.0)));
    if vba {
        // Not a real VBA project: bytes the package check counts as content.
        wb.vba_project = Some(Arc::new(b"<vba><module/></vba>".to_vec()));
    }
    wb
}

/// `bytes` with relationships added to rels parts (created when missing) and `parts` added.
fn with_rels(bytes: &[u8], rels: &[(&str, &str)], parts: &[(&str, &str)]) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut pending: Vec<(&str, &str)> = rels.to_vec();
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut body = Vec::new();
        f.read_to_end(&mut body).unwrap();
        if let Some(at) = pending.iter().position(|(r, _)| *r == name) {
            let text = String::from_utf8(body).unwrap();
            body = text.replace("</Relationships>", &format!("{}</Relationships>", pending.remove(at).1)).into_bytes();
        }
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&body).unwrap();
    }
    for (name, extra) in pending {
        let body = format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{extra}</Relationships>"#);
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(body.as_bytes()).unwrap();
    }
    for (name, body) in parts {
        out.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(body.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn package_of(name: &str, source: &[u8]) -> Vec<String> {
    let r = check_bytes(name, source, &Options::default());
    assert!(r.error.is_none(), "{name} opens: {:?}", r.error);
    r.round_trip.categories.iter().find(|c| c.name == "package").unwrap().examples.clone()
}

#[test]
fn macro_source_round_trip_name_follows_the_vba_project() {
    let (with, without) = (book(true), book(false));
    // (source name, carries a VBA project, the name the round trip is saved under)
    let table: &[(&str, bool, &str)] = &[
        ("book.xlsb", true, "roundtrip.xlsm"),
        ("BOOK.XLSB", true, "roundtrip.xlsm"),
        ("book.xls", true, "roundtrip.xlsm"),
        ("book.xlsx", true, "roundtrip.xlsm"),
        ("book", true, "roundtrip.xlsm"),
        ("book.xlsb", false, "roundtrip.xlsx"),
        ("book.xls", false, "roundtrip.xlsx"),
        ("book.xlsx", false, "roundtrip.xlsx"),
        ("book.ods", false, "roundtrip.xlsx"),
        ("book.xlsm", true, "roundtrip.xlsm"),
        ("book.xlsm", false, "roundtrip.xlsm"),
        ("book.xltm", true, "roundtrip.xltm"),
        ("book.xltm", false, "roundtrip.xltm"),
        ("book.xltx", false, "roundtrip.xltx"),
    ];
    let wrong: Vec<String> = table
        .iter()
        .filter_map(|&(name, vba, want)| {
            let got = round_trip_name(name, if vba { &with } else { &without });
            (got != want).then(|| format!("{name} (vba {vba}): want {want}, got {got}"))
        })
        .collect();
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn macro_source_named_xlsx_keeps_its_vba_project_in_the_round_trip() {
    // A macro-enabled package under an `.xlsx` name: the reader keeps the project, so the check
    // must save it as `.xlsm` and report only the made-up kind as lost.
    let xlsm = gridcraft_engine::io::save_bytes(&book(true), "t.xlsm").unwrap();
    let source = with_rels(&xlsm, &[(WB_RELS, MADE_UP)], &[MADE_UP_PART]);
    assert_eq!(package_of("t.xlsx", &source), ["lost relationship madeUp"]);
}

#[test]
fn macro_source_without_macros_is_still_checked_as_xlsx() {
    let xlsx = gridcraft_engine::io::save_bytes(&book(false), "t.xlsx").unwrap();
    let source = with_rels(&xlsx, &[(WB_RELS, MADE_UP)], &[MADE_UP_PART]);
    assert_eq!(package_of("t.xlsx", &source), ["lost relationship madeUp"]);
}

#[test]
fn macro_source_binary_index_is_not_a_loss() {
    // The `.xlsb` sheet index: a lookup cache for the binary sheet stream, never written to `.xlsx`.
    let xlsx = gridcraft_engine::io::save_bytes(&book(false), "t.xlsx").unwrap();
    let source = with_rels(
        &xlsx,
        &[(WB_RELS, MADE_UP), (SHEET_RELS, BINARY_INDEX)],
        // Not a real index: bytes the package check counts as content, as it does for some real
        // binary index parts (their bytes are not an empty XML part).
        &[MADE_UP_PART, ("xl/worksheets/binaryIndex1.bin", "<index><entry/></index>")],
    );
    assert_eq!(package_of("t.xlsx", &source), ["lost relationship madeUp"]);
}
