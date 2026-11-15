//! Core document properties in the round-trip check: a `docProps/core.xml` that holds no text, or a
//! core-properties relationship whose part is not in the package, has nothing to lose; and
//! `cp:revision` is kept on save, as Excel's own save keeps it.
#![allow(clippy::unwrap_used)]

use std::io::Write as _;

use super::*;

const ROOT_RELS: &str = "_rels/.rels";
const CORE: &str = "docProps/core.xml";
const WB_RELS: &str = "xl/_rels/workbook.xml.rels";
/// A relationship kind GridCraft doesn't know: it stays lost, so the comparison is known to run.
const MADE_UP: &str = r#"<Relationship Id="rId90" Type="http://schemas.microsoft.com/office/2099/relationships/madeUp" Target="madeUp.xml"/>"#;
const MADE_UP_PART: (&str, &str) = ("xl/madeUp.xml", "<madeUp><item/></madeUp>");
const CORE_OPEN: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">"#;

fn core(children: &str) -> String {
    format!("{CORE_OPEN}{children}</cp:coreProperties>")
}

fn plain() -> Vec<u8> {
    let mut wb = Workbook::new();
    wb.sheet_mut(0).unwrap().set_cell(CellRef::parse("A1").unwrap(), Cell::value(Value::number(1.0)));
    gridcraft_engine::io::save_bytes(&wb, "t.xlsx").unwrap()
}

/// `bytes` with `core` as the core part (`None`: the part removed), `root_rels_edit` applied to
/// `_rels/.rels`, and the made-up relationship + part added.
fn rebuilt(bytes: &[u8], core: Option<&str>, root_rels_edit: &dyn Fn(String) -> String) -> Vec<u8> {
    let mut src = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let mut saw_core = false;
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let part = f.name().to_string();
        let mut body = Vec::new();
        f.read_to_end(&mut body).unwrap();
        if part == CORE {
            saw_core = true;
            match core {
                Some(c) => body = c.as_bytes().to_vec(),
                None => continue,
            }
        }
        if part == ROOT_RELS {
            body = root_rels_edit(String::from_utf8(body).unwrap()).into_bytes();
        }
        if part == WB_RELS {
            body = String::from_utf8(body).unwrap().replace("</Relationships>", &format!("{MADE_UP}</Relationships>")).into_bytes();
        }
        out.start_file(part, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(&body).unwrap();
    }
    assert!(saw_core, "GridCraft's own save writes {CORE}");
    out.start_file(MADE_UP_PART.0, zip::write::SimpleFileOptions::default()).unwrap();
    out.write_all(MADE_UP_PART.1.as_bytes()).unwrap();
    out.finish().unwrap().into_inner()
}

fn package_of(source: &[u8]) -> Vec<String> {
    let r = check_bytes("t.xlsx", source, &Options::default());
    assert!(r.error.is_none(), "opens: {:?}", r.error);
    r.round_trip.categories.iter().find(|c| c.name == "package").unwrap().examples.clone()
}

fn same(s: String) -> String {
    s
}

fn saved_core(source: &[u8]) -> String {
    let (wb, _) = gridcraft_engine::io::open_bytes("t.xlsx", source).unwrap();
    let saved = gridcraft_engine::io::save_bytes(&wb, "t.xlsx").unwrap();
    let mut z = zip::ZipArchive::new(Cursor::new(saved)).unwrap();
    let mut s = String::new();
    z.by_name(CORE).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn core_props_with_only_empty_elements_are_not_a_loss() {
    let table: &[&str] = &[
        "<dc:title/><dc:subject/><dc:creator/><cp:keywords/><dc:description/><cp:lastModifiedBy/><cp:category/>",
        "<dc:title></dc:title><dc:creator></dc:creator><cp:contentType></cp:contentType><cp:contentStatus></cp:contentStatus>",
        "<dc:title> </dc:title><dc:creator>\n</dc:creator>",
        r#"<dcterms:created xsi:type="dcterms:W3CDTF"></dcterms:created>"#,
    ];
    for children in table {
        let source = rebuilt(&plain(), Some(&core(children)), &same);
        assert_eq!(package_of(&source), ["lost relationship madeUp"], "core children: {children}");
    }
}

#[test]
fn core_props_relationship_to_a_missing_part_is_not_a_loss() {
    // The root relationship names a part the package doesn't have; Excel reads no properties either.
    let source = rebuilt(&plain(), None, &|rels: String| rels.replace("Target=\"docProps/core.xml\"", "Target=\"/xl/docProps/core.xml\""));
    assert!(!String::from_utf8_lossy(&source).contains("Target=\"docProps/core.xml\""), "the fixture moved the target");
    assert_eq!(package_of(&source), ["lost relationship madeUp"]);
}

#[test]
fn core_props_revision_is_kept_on_save() {
    // (source cp:revision, what the save writes back; None = no cp:revision element)
    let table: &[(&str, Option<&str>)] =
        &[("0", Some("0")), ("7", Some("7")), ("12345", Some("12345")), ("abc", None), ("", None), ("12345678901", None)];
    for (rev, want) in table {
        let source = rebuilt(&plain(), Some(&core(&format!("<cp:revision>{rev}</cp:revision>"))), &same);
        let saved = saved_core(&source);
        match want {
            Some(v) => assert!(saved.contains(&format!("<cp:revision>{v}</cp:revision>")), "revision {rev:?}: saved {saved}"),
            None => assert!(!saved.contains("revision>"), "revision {rev:?}: saved {saved}"),
        }
    }
    let source = rebuilt(&plain(), Some(&core("<cp:revision>0</cp:revision>")), &same);
    assert_eq!(package_of(&source), ["lost relationship madeUp"]);
}

#[test]
fn core_props_with_text_still_round_trip_clean_and_keep_their_values() {
    let source = rebuilt(
        &plain(),
        Some(&core("<dc:title>Budget</dc:title><cp:lastModifiedBy>A. Person</cp:lastModifiedBy><cp:revision>3</cp:revision>")),
        &same,
    );
    let saved = saved_core(&source);
    for want in ["<dc:title>Budget</dc:title>", "<cp:lastModifiedBy>A. Person</cp:lastModifiedBy>", "<cp:revision>3</cp:revision>"] {
        assert!(saved.contains(want), "{want} in {saved}");
    }
    assert_eq!(package_of(&source), ["lost relationship madeUp"]);
}
