//! Excel cannot store `#SPILL!` or `#CALC!` in a cell's `<v>`: it writes the fallback `#VALUE!` and
//! points the cell (`vm`) at a rich value of structure type `_error` whose `errorType` names the real
//! error ([MS-XLSX] 2.3.6 rich data, ECMA-376 Part 1 §18.9 metadata). Part spellings and the
//! `errorType` numbers (8 = `#SPILL!`, 13 = `#CALC!`) are those of a workbook saved by Microsoft 365
//! Excel with a blocked spill, an empty FILTER and formulas that pass those errors on.

use super::make_zip;
use crate::read_xlsx;
use gridcraft_core::{CellError, CellRef, Value};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const RD: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
const RD_REL: &str = "http://schemas.microsoft.com/office/2017/06/relationships";

/// A one-sheet package with `cells` in row order and the metadata and rich-value parts for five
/// error values: `vm` 1 and 2 are `#SPILL!` (errorType 8), 3 and 4 `#CALC!` (errorType 13), 5 an
/// errorType this reader does not know (99).
fn package(cells: &str) -> Vec<u8> {
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/metadata.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"/><Override PartName="/xl/richData/rdrichvalue.xml" ContentType="application/vnd.ms-excel.rdrichvalue+xml"/><Override PartName="/xl/richData/rdrichvaluestructure.xml" ContentType="application/vnd.ms-excel.rdrichvaluestructure+xml"/><Override PartName="/xl/richData/rdRichValueTypes.xml" ContentType="application/vnd.ms-excel.rdrichvaluetypes+xml"/></Types>"#;
    let root = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let wb = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId5" Type="{REL}/sheetMetadata" Target="metadata.xml"/><Relationship Id="rId6" Type="{RD_REL}/rdRichValue" Target="richData/rdrichvalue.xml"/><Relationship Id="rId7" Type="{RD_REL}/rdRichValueStructure" Target="richData/rdrichvaluestructure.xml"/><Relationship Id="rId8" Type="{RD_REL}/rdRichValueTypes" Target="richData/rdRichValueTypes.xml"/></Relationships>"#
    );
    let sheet =
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{MAIN}"><sheetData>{cells}</sheetData></worksheet>"#);
    let blocks: String = (0..5)
        .map(|i| format!(r#"<bk><extLst><ext uri="{{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}}"><xlrd:rvb i="{i}"/></ext></extLst></bk>"#))
        .collect();
    let value_blocks: String = (0..5).map(|i| format!(r#"<bk><rc t="1" v="{i}"/></bk>"#)).collect();
    let metadata = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><metadata xmlns="{MAIN}" xmlns:xlrd="{RD}"><metadataTypes count="1"><metadataType name="XLRICHVALUE" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1"/></metadataTypes><futureMetadata name="XLRICHVALUE" count="5">{blocks}</futureMetadata><valueMetadata count="5">{value_blocks}</valueMetadata></metadata>"#
    );
    // Structures as Excel writes them: a blocked spill, a passed-on error, an error with a subtype.
    let structures = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><rvStructures xmlns="{RD}" count="3"><s t="_error"><k n="colOffset" t="i"/><k n="errorType" t="i"/><k n="rwOffset" t="i"/><k n="subType" t="i"/></s><s t="_error"><k n="errorType" t="i"/><k n="propagated" t="b"/></s><s t="_error"><k n="errorType" t="i"/><k n="subType" t="i"/></s></rvStructures>"#
    );
    let values = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><rvData xmlns="{RD}" count="5"><rv s="0"><v>0</v><v>8</v><v>2</v><v>1</v></rv><rv s="1"><v>8</v><v>1</v></rv><rv s="2"><v>13</v><v>3</v></rv><rv s="1"><v>13</v><v>1</v></rv><rv s="1"><v>99</v><v>0</v></rv></rvData>"#
    );
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><rvTypesInfo xmlns="http://schemas.microsoft.com/office/spreadsheetml/2017/richdata2"><global><keyFlags><key name="_Self"><flag name="ExcludeFromFile" value="1"/><flag name="ExcludeFromCalcComparison" value="1"/></key></keyFlags></global></rvTypesInfo>"#;
    make_zip(&[
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/metadata.xml", metadata.as_bytes()),
        ("xl/richData/rdrichvalue.xml", values.as_bytes()),
        ("xl/richData/rdrichvaluestructure.xml", structures.as_bytes()),
        ("xl/richData/rdRichValueTypes.xml", types.as_bytes()),
    ])
}

/// A cached error cell: `vm` 0 writes no `vm` attribute.
fn error_cell(a1: &str, vm: u32, fallback: &str) -> String {
    let row = a1.trim_start_matches(char::is_alphabetic);
    let vm = if vm == 0 { String::new() } else { format!(r#" vm="{vm}""#) };
    format!(r#"<row r="{row}"><c r="{a1}" t="e"{vm}><v>{fallback}</v></c></row>"#)
}

fn value(wb: &gridcraft_model::Workbook, a1: &str) -> Value {
    wb.sheets[0].value(CellRef::parse(a1).unwrap())
}

/// Warnings about value metadata or pictures in cells.
fn metadata_warnings(w: &[String]) -> Vec<&String> {
    w.iter().filter(|w| w.contains("value metadata") || w.contains("picture")).collect()
}

#[test]
fn rich_error_values_read_as_spill_and_calc() {
    let cells =
        [error_cell("A1", 1, "#VALUE!"), error_cell("A2", 2, "#VALUE!"), error_cell("A3", 3, "#VALUE!"), error_cell("A4", 4, "#VALUE!")].concat();
    let (wb, report) = read_xlsx(&package(&cells)).unwrap();
    assert_eq!(value(&wb, "A1"), Value::Error(CellError::Spill), "blocked spill");
    assert_eq!(value(&wb, "A2"), Value::Error(CellError::Spill), "#SPILL! passed on by a formula");
    assert_eq!(value(&wb, "A3"), Value::Error(CellError::Calc), "empty array");
    assert_eq!(value(&wb, "A4"), Value::Error(CellError::Calc), "#CALC! passed on by a formula");
    assert!(metadata_warnings(&report.warnings).is_empty(), "no warning for error rich values: {:?}", report.warnings);
}

#[test]
fn rich_error_values_leave_plain_and_unknown_errors_alone() {
    let cells = [error_cell("A5", 0, "#VALUE!"), error_cell("A6", 0, "#DIV/0!"), error_cell("A7", 5, "#VALUE!")].concat();
    let (wb, _) = read_xlsx(&package(&cells)).unwrap();
    assert_eq!(value(&wb, "A5"), Value::Error(CellError::Value), "no value metadata");
    assert_eq!(value(&wb, "A6"), Value::Error(CellError::Div0), "an error Excel stores as itself");
    assert_eq!(value(&wb, "A7"), Value::Error(CellError::Value), "an errorType this reader does not know keeps the fallback");
}
