//! XLSB dynamic-array formulas: an array formula (BrtArrFmla) whose anchor cell follows a
//! BrtCellMeta pointing at a cell-metadata block of the "XLDAPR" type opens as a dynamic-array
//! formula, like `cm` on an .xlsx cell. Records are assembled in Rust from [MS-XLSB]:
//! - record ids, 2.3.2 record enumeration:
//!   <https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-xlsb/acc8aa92-1f02-4167-99f5-84f9f676b95a>
//!   (BrtCellMeta 49, BrtMdb 51, BrtBeginFmd 52, BrtEndFmd 53, BrtBeginMetadata 332,
//!   BrtEndMetadata 333, BrtBeginEsmdtinfo 334, BrtMdtinfo 335, BrtEndEsmdtinfo 336,
//!   BrtBeginEsmdb 337, BrtEndEsmdb 338, BrtBeginEsfmd 339, BrtEndEsfmd 340,
//!   BrtBeginDynamicArrayPr 4096, BrtEndDynamicArrayPr 4097, BrtFRTBegin 35, BrtFRTEnd 36);
//! - the Metadata part ABNF, 2.1.7.34 (relationship type `.../relationships/sheetMetadata`);
//! - BrtCellMeta 2.4.325 (`icmb`: one-based index of a BrtMdb after a BrtBeginEsmdb with
//!   fCellMeta = 1, for the cell record that follows), BrtMdb 2.4.710 and Mdir 2.5.94 (`iMdt`
//!   one-based into the BrtMdtinfo list, `mdd` zero-based into the BrtBeginFmd records of the
//!   BrtBeginEsfmd with the same name), BrtMdtinfo 2.4.711, BrtBeginEsmdb 2.4.74,
//!   BrtBeginEsfmd 2.4.73, BrtBeginDynamicArrayPr 2.4.60.
//!
//! Spec note: the packet diagram for fDynamic and fCollapsed (one byte each) is printed under
//! BrtEndDynamicArrayPr (2.4.416), while BrtBeginDynamicArrayPr (2.4.60) shows only a 4-byte
//! FRTBlank header. The reader follows the spec as published and takes fDynamic from
//! BrtEndDynamicArrayPr.
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::Workbook;

use super::make_zip;
use super::xlsb_tests::{ptg_exp, ptg_int, record, rfx, row, wide, worksheet};
use crate::package::Package;
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

const METADATA_TYPE: &str = "application/vnd.ms-excel.sheetMetadata";

/// A formula cell (BrtFmlaNum) in column `col` with its cached number and rgce.
fn fmla(col: u32, cached: f64, rgce: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(cached.to_le_bytes());
    data.extend([0; 2]);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(9, &data)
}
/// BrtArrFmla over `range` with its rgce.
fn arr_fmla(range: &[u8], rgce: &[u8]) -> Vec<u8> {
    let mut data = range.to_vec();
    data.push(0);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(426, &data)
}
/// `SEQUENCE(3)`: a newer function, called by name through the hidden name `_xlfn.SEQUENCE`
/// (PtgName 1) and PtgFuncVar with iftab 255.
fn sequence() -> Vec<u8> {
    [&[0x23u8, 1, 0, 0, 0][..], &ptg_int(3), &[0x22, 2, 0xFF, 0]].concat()
}
fn cell_meta(icmb: u32) -> Vec<u8> {
    record(49, &icmb.to_le_bytes())
}
/// A1 = SEQUENCE(3) as an array over A1:A3 with cached 1, 2, 3; `meta` goes before A1.
fn sheet(meta: &[u8]) -> Vec<u8> {
    let data = [
        row(0),
        meta.to_vec(),
        fmla(0, 1.0, &ptg_exp(0)),
        arr_fmla(&rfx(0, 2, 0, 0), &sequence()),
        row(1),
        fmla(0, 2.0, &ptg_exp(0)),
        row(2),
        fmla(0, 3.0, &ptg_exp(0)),
    ]
    .concat();
    worksheet(&data, &[])
}
/// The metadata part: types `XLDAPR`, one XLDAPR future metadata record with `f_dynamic`, and
/// one cell-metadata block referring to it.
fn metadata(f_dynamic: u8) -> Vec<u8> {
    let mut mdtinfo = 0u32.to_le_bytes().to_vec();
    mdtinfo.extend(120_000u32.to_le_bytes());
    mdtinfo.extend(wide("XLDAPR"));
    let mut esfmd = 1u32.to_le_bytes().to_vec();
    esfmd.extend(wide("XLDAPR"));
    let mdb: Vec<u8> = [1u32, 1, 0].iter().flat_map(|v| v.to_le_bytes()).collect();
    [
        record(332, &[]),
        record(334, &1u32.to_le_bytes()),
        record(335, &mdtinfo),
        record(336, &[]),
        record(339, &esfmd),
        record(52, &[]),
        record(35, &[0; 4]),
        record(4096, &[0; 4]),
        record(4097, &[f_dynamic, 0]),
        record(36, &[]),
        record(53, &[]),
        record(340, &[]),
        record(337, &[1, 0, 0, 0, 1, 0, 0, 0]),
        record(51, &mdb),
        record(338, &[]),
        record(333, &[]),
    ]
    .concat()
}
fn book() -> Vec<u8> {
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    let mut sheet = 0u32.to_le_bytes().to_vec();
    sheet.extend(1u32.to_le_bytes());
    sheet.extend(wide("s0"));
    sheet.extend(wide("Data"));
    // BrtName: hidden future-function name, workbook scope, no formula, no comment.
    let mut name = 0x0002_0001u32.to_le_bytes().to_vec();
    name.push(0);
    name.extend(u32::MAX.to_le_bytes());
    name.extend(wide("_xlfn.SEQUENCE"));
    name.extend(0u32.to_le_bytes());
    name.extend(0u32.to_le_bytes());
    name.extend(u32::MAX.to_le_bytes());
    [record(131, &[]), record(153, &props), record(143, &[]), record(156, &sheet), record(144, &[]), record(39, &name), record(132, &[])].concat()
}
/// The package; `metadata` adds `xl/metadata.bin` and its relationship.
fn package(sheet: &[u8], metadata: Option<&[u8]>) -> Vec<u8> {
    let root = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#;
    let meta_rel =
        r#"<Relationship Id="m" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.bin"/>"#;
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s0" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.bin"/>{}</Relationships>"#,
        if metadata.is_some() { meta_rel } else { "" }
    );
    let types = format!(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/worksheets/sheet1.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/metadata.bin" ContentType="{METADATA_TYPE}"/></Types>"#
    );
    let book = book();
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.bin", sheet),
    ];
    if let Some(m) = metadata {
        parts.push(("xl/metadata.bin", m));
    }
    make_zip(&parts)
}
fn read(bytes: &[u8]) -> (Workbook, ReadReport) {
    read_xlsb(bytes).unwrap()
}
fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}
fn formula(wb: &Workbook, a1: &str) -> Option<gridcraft_model::Formula> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_deref().cloned())
}
fn metadata_warnings(report: &ReadReport) -> usize {
    report.warnings.iter().filter(|w| w.contains("metadata")).count()
}

/// The .xlsx reader on the equivalent XML: the oracle.
fn xlsx_oracle() -> Workbook {
    const METADATA: &str = r#"<?xml version="1.0"?><metadata xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray"><metadataTypes count="1"><metadataType name="XLDAPR" minSupportedVersion="120000" cellMeta="1"/></metadataTypes><futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="1" v="0"/></bk></cellMetadata></metadata>"#;
    const METADATA_REL: &str =
        r#"<Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml"/>"#;
    let body = r#"<sheetData><row r="1"><c r="A1" cm="1"><f t="array" ref="A1:A3">SEQUENCE(3)</f><v>1</v></c></row><row r="2"><c r="A2"><v>2</v></c></row><row r="3"><c r="A3"><v>3</v></c></row></sheetData>"#;
    read_xlsx(&super::minimal(body, &[("xl/metadata.xml", METADATA)], METADATA_REL, "")).unwrap().0
}

#[test]
fn dynamic_anchor_imports_as_a_spilling_formula_like_xlsx() {
    let (wb, report) = read(&package(&sheet(&cell_meta(1)), Some(&metadata(1))));
    assert_eq!(metadata_warnings(&report), 0, "{:?}", report.warnings);
    let f = formula(&wb, "A1").expect("A1 formula");
    assert_eq!((f.text.as_str(), f.dynamic, f.array), ("SEQUENCE(3)", true, None));
    assert_eq!(wb.sheets[0].value(at("A1")), Value::Number(1.0));
    for a1 in ["A2", "A3"] {
        assert_eq!(formula(&wb, a1), None, "{a1}");
        assert_eq!(wb.sheets[0].value(at(a1)), Value::Empty, "{a1}");
    }
    let oracle = xlsx_oracle();
    for a1 in ["A1", "A2", "A3"] {
        assert_eq!(formula(&wb, a1), formula(&oracle, a1), "{a1} formula matches the .xlsx reader");
        assert_eq!(wb.sheets[0].value(at(a1)), oracle.sheets[0].value(at(a1)), "{a1} value matches the .xlsx reader");
    }
}

#[test]
fn legacy_array_without_cell_metadata_is_unchanged() {
    for metadata_part in [None, Some(metadata(1))] {
        let (wb, _) = read(&package(&sheet(&[]), metadata_part.as_deref()));
        let f = formula(&wb, "A1").expect("A1 formula");
        assert_eq!((f.text.as_str(), f.dynamic, f.array), ("SEQUENCE(3)", false, Some(RangeRef::parse("A1:A3").unwrap())));
        assert_eq!(formula(&wb, "A2"), None);
    }
}

#[test]
fn cell_metadata_that_is_not_dynamic_keeps_the_legacy_array() {
    let (wb, _) = read(&package(&sheet(&cell_meta(1)), Some(&metadata(0))));
    let f = formula(&wb, "A1").expect("A1 formula");
    assert_eq!((f.dynamic, f.array), (false, Some(RangeRef::parse("A1:A3").unwrap())));
}

#[test]
fn missing_metadata_part_falls_back_to_any_cell_metadata_being_dynamic() {
    let (wb, report) = read(&package(&sheet(&cell_meta(1)), None));
    assert_eq!(metadata_warnings(&report), 0, "{:?}", report.warnings);
    let f = formula(&wb, "A1").expect("A1 formula");
    assert_eq!((f.dynamic, f.array), (true, None));
    assert_eq!(wb.sheets[0].value(at("A3")), Value::Empty);
}

#[test]
fn dynamic_formula_round_trips_through_xlsx() {
    let (wb, _) = read(&package(&sheet(&cell_meta(1)), Some(&metadata(1))));
    let saved = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&saved).unwrap();
    let f = formula(&back, "A1").expect("A1 formula");
    assert_eq!((f.text.as_str(), f.dynamic), ("SEQUENCE(3)", true));
    let mut pkg = Package::open(&saved).unwrap();
    assert!(pkg.read("xl/metadata.xml").unwrap().is_some(), "metadata part written");
    let xml = String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap();
    let a1 = xml.split("<c r=\"A1\"").nth(1).and_then(|s| s.split("</c>").next()).unwrap();
    assert!(a1.contains("cm="), "{xml}");
    assert!(!xml.contains('{'), "{xml}");
}

#[test]
fn truncated_metadata_and_cell_metadata_never_crash() {
    let full = metadata(1);
    for len in 0..full.len() {
        let (wb, report) = read_xlsb(&package(&sheet(&cell_meta(1)), Some(&full[..len]))).unwrap();
        assert!(metadata_warnings(&report) <= 1, "{len}: {:?}", report.warnings);
        // A damaged part falls back to the `cm` rule.
        assert!(formula(&wb, "A1").is_some_and(|f| f.dynamic), "{len}");
    }
    for len in 0..4 {
        let meta = record(49, &1u32.to_le_bytes()[..len]);
        let (wb, report) = read_xlsb(&package(&sheet(&meta), Some(&full))).unwrap();
        assert_eq!(report.warnings.iter().filter(|w| w.contains("BrtCellMeta")).count(), 1, "{len}: {:?}", report.warnings);
        assert!(formula(&wb, "A1").is_some(), "{len}");
    }
    // Hostile indexes and counts: no block, a block past the end, a huge Mdir count.
    for icmb in [0, 2, u32::MAX] {
        assert!(read_xlsb(&package(&sheet(&cell_meta(icmb)), Some(&full))).is_ok());
    }
    // Replace the four closing records (BrtBeginEsmdb, BrtMdb, BrtEndEsmdb, BrtEndMetadata).
    let huge = [
        &full[..full.len() - (11 + 14 + 3 + 3)],
        &record(337, &[1, 0, 0, 0, 1, 0, 0, 0]),
        &record(51, &u32::MAX.to_le_bytes()),
        &record(338, &[]),
        &record(333, &[]),
    ]
    .concat();
    let (wb, report) = read_xlsb(&package(&sheet(&cell_meta(1)), Some(&huge))).unwrap();
    assert_eq!(metadata_warnings(&report), 1, "{:?}", report.warnings);
    assert!(formula(&wb, "A1").is_some_and(|f| f.dynamic));
}
