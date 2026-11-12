//! Cube (MDX) value metadata written next to pictures in cells: both kinds of `valueMetadata`
//! block share one `metadata` part, and each cell's `vm` still resolves to its own kind.

use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{CellPicture, MdxMetadata, Workbook};

use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

fn workbook() -> Workbook {
    let mut wb = Workbook::new();
    wb.mdx_metadata = Some(Arc::new(MdxMetadata {
        type_attrs: vec![("name".into(), "XLMDX".into()), ("minSupportedVersion".into(), "120000".into())],
        strings: vec!["Cube".into(), "[Measures].[Units]".into()],
        records: vec![r#"<mdx n="0" f="m"><t c="1"><n x="1"/></t></mdx>"#.into(), r#"<mdx n="0" f="v"><t c="1"><n x="1"/></t></mdx>"#.into()],
    }));
    let sh = wb.sheet_mut(0).unwrap();
    sh.set_value(at("A1"), gridcraft_core::Value::text("Units"));
    sh.cell_mdx.insert(at("A1"), 0);
    sh.set_value(at("A2"), gridcraft_core::Value::Number(5.0));
    sh.cell_mdx.insert(at("A2"), 1);
    sh.set_picture(at("C1"), Arc::new(CellPicture { data: PNG.to_vec(), mime: "image/png".into(), alt: String::new() }));
    wb
}

/// The metadata type name a cell's `vm` resolves to, with the `rc v` index.
fn resolve(bytes: &[u8], address: &str) -> (String, u32) {
    let mut pkg = Package::open(bytes).unwrap();
    let sheet = pkg.read_xml("xl/worksheets/sheet1.xml").unwrap().unwrap();
    let c = sheet.child("sheetData").unwrap().kids("row").flat_map(|r| r.kids("c")).find(|c| c.attr("r") == Some(address)).unwrap();
    let vm: usize = c.attr("vm").unwrap().parse().unwrap();
    let metadata = pkg.read_xml("xl/metadata.xml").unwrap().unwrap();
    let rc = metadata.child("valueMetadata").unwrap().kids("bk").nth(vm - 1).unwrap().child("rc").unwrap();
    let types: Vec<&str> = metadata.child("metadataTypes").unwrap().kids("metadataType").map(|t| t.attr("name").unwrap()).collect();
    (types[rc.attr_u32("t").unwrap() as usize - 1].to_string(), rc.attr_u32("v").unwrap())
}

#[test]
fn cube_links_and_cell_pictures_share_the_metadata_part() {
    let bytes = write_xlsx(&workbook()).unwrap();
    assert_eq!(resolve(&bytes, "A1"), ("XLMDX".into(), 0));
    assert_eq!(resolve(&bytes, "A2"), ("XLMDX".into(), 1));
    assert_eq!(resolve(&bytes, "C1"), ("XLRICHVALUE".into(), 0));
    // Read back: the picture is still a picture and the links still name their records.
    let (back, report) = read_xlsx(&bytes).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("metadata")), "{:?}", report.warnings);
    let sh = back.sheet(0).unwrap();
    assert!(sh.cell_pictures.contains_key(&at("C1")));
    assert_eq!(sh.cell_mdx.get(&at("A1")), Some(&0));
    assert_eq!(sh.cell_mdx.get(&at("A2")), Some(&1));
    assert_eq!(back.mdx_metadata, workbook().mdx_metadata);
}

#[test]
fn a_picture_placed_over_a_cube_value_drops_the_link() {
    let mut wb = workbook();
    let sh = wb.sheet_mut(0).unwrap();
    sh.set_picture(at("A2"), Arc::new(CellPicture { data: PNG.to_vec(), mime: "image/png".into(), alt: String::new() }));
    assert!(!sh.cell_mdx.contains_key(&at("A2")));
    let bytes = write_xlsx(&wb).unwrap();
    assert_eq!(resolve(&bytes, "A2").0, "XLRICHVALUE");
}

#[test]
fn a_link_to_a_record_the_workbook_lacks_is_not_written() {
    let mut wb = workbook();
    wb.sheet_mut(0).unwrap().cell_mdx.insert(at("A1"), 9);
    let bytes = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&bytes).unwrap();
    let sheet = pkg.read_xml("xl/worksheets/sheet1.xml").unwrap().unwrap();
    let a1 = sheet.child("sheetData").unwrap().kids("row").flat_map(|r| r.kids("c")).find(|c| c.attr("r") == Some("A1")).unwrap();
    assert_eq!(a1.attr("vm"), None);
}
