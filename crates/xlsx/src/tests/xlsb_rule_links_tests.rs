//! Rule formulas use the workbook links and BIFF12 extra data, just like cell formulas.
use super::minimal;
use super::xlsb_names_tests::{area3d, book, externals, name_ptg, name_record, package, record};
use super::xlsb_rules_tests::{cond_format, sheet, sqrfx};
use crate::{ReadReport, read_xlsb, read_xlsx};
use gridcraft_model::{CfRule, Workbook};

fn parsed(rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    [(rgce.len() as u32).to_le_bytes().to_vec(), rgce.to_vec(), (rgcb.len() as u32).to_le_bytes().to_vec(), rgcb.to_vec()].concat()
}

fn read(tail: &[u8]) -> (Workbook, ReadReport) {
    // XTI 1 is the second sheet, XTI 2 belongs to the external SupBook.
    let names = [name_record(0, u32::MAX, "Choices", &area3d(1, 0, 19, 0, 0, false))];
    let workbook = book(&["Sheet1", "Lists"], &externals(), &names);
    read_xlsb(&package(&workbook, &[sheet(tail), sheet(&[])])).unwrap()
}

fn validation(row: u32, rgce: &[u8]) -> Vec<u8> {
    let payload = [
        3u32.to_le_bytes().to_vec(), // list validation
        sqrfx(&[(row, row, 1, 1)]),
        vec![0xff; 16], // four null DValStrings
        parsed(rgce, &[]),
        parsed(&[], &[]),
    ]
    .concat();
    [record(573, &[0; 18]), record(64, &payload), record(574, &[])].concat()
}

fn expression(rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    let mut header = vec![];
    for n in [2u32, 1, u32::MAX, 1, 0, 0, 0] {
        header.extend(n.to_le_bytes());
    }
    header.extend(0u16.to_le_bytes());
    header.extend((rgce.len() as u32).to_le_bytes());
    header.extend([0; 8]); // no second or third formula
    header.extend(u32::MAX.to_le_bytes()); // null strParam
    header.extend(parsed(rgce, rgcb));
    cond_format(&[(0, 4, 0, 0)], &[[record(463, &header), record(464, &[])].concat()])
}

#[test]
fn xlsb_validation_list_uses_another_sheet() {
    // [MS-XLSB] 2.5.98.19 PtgArea3d: u16 XTI and 12-byte RgceArea.
    let (wb, report) = read(&validation(1, &area3d(1, 0, 19, 0, 0, false)));
    assert_eq!(wb.sheets[0].validations.len(), 1, "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].validations[0].f1, "=Lists!$A$1:$A$20");
    let xml = r#"<dataValidations count="1"><dataValidation type="list" sqref="B2"><formula1>Lists!$A$1:$A$20</formula1></dataValidation></dataValidations>"#;
    let (oracle, _) = read_xlsx(&minimal(xml, &[], "", "")).unwrap();
    assert_eq!(wb.sheets[0].validations[0], oracle.sheets[0].validations[0]);
    assert!(!report.warnings.iter().any(|w| w.contains("were skipped")));
}

#[test]
fn xlsb_validation_list_uses_a_defined_name() {
    // [MS-XLSB] 2.5.98.60 PtgName: one-based u32 index into BrtName records.
    let (wb, report) = read(&validation(2, &name_ptg(1)));
    assert_eq!(wb.sheets[0].validations.len(), 1, "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].validations[0].f1, "=Choices");
    assert_eq!(wb.names[0].name, "Choices");
    let xml = r#"<dataValidations count="1"><dataValidation type="list" sqref="B3"><formula1>Choices</formula1></dataValidation></dataValidations>"#;
    let (oracle, _) = read_xlsx(&minimal(xml, &[], "", "")).unwrap();
    assert_eq!(wb.sheets[0].validations[0], oracle.sheets[0].validations[0]);
    assert!(!report.warnings.iter().any(|w| w.contains("were skipped")));
}

fn array_formula() -> (Vec<u8>, Vec<u8>) {
    let mut rgce = vec![0x44]; // PtgRef, value class: relative A1
    rgce.extend(0u32.to_le_bytes());
    rgce.extend(0xC000u16.to_le_bytes());
    // [MS-XLSB] 2.5.98.23 PtgArray: array class and 14 unused bytes.
    rgce.push(0x60);
    rgce.extend([0; 14]);
    rgce.extend([0x0B, 0x22, 1, 37, 0]); // PtgEq, PtgFuncVar OR with one argument
    // 2.5.98.41 PtgExtraArray: u32 row/column counts, then row-major SerAr.
    let mut rgcb = [1u32.to_le_bytes(), 3u32.to_le_bytes()].concat();
    for n in [1.0f64, 2.0, 3.0] {
        rgcb.push(0); // 2.5.98.96 SerNum
        rgcb.extend(n.to_le_bytes());
    }
    (rgce, rgcb)
}

#[test]
fn xlsb_cf_expression_keeps_array_extra_data() {
    let (rgce, rgcb) = array_formula();
    let (wb, report) = read(&expression(&rgce, &rgcb));
    assert_eq!(wb.sheets[0].cond_formats.len(), 1, "{:?}", report.warnings);
    assert!(matches!(&wb.sheets[0].cond_formats[0].rule, CfRule::Expression { formula, .. } if formula == "OR(A1={1,2,3})"));
    let xml = r#"<conditionalFormatting sqref="A1:A5"><cfRule type="expression" priority="1"><formula>OR(A1={1,2,3})</formula></cfRule></conditionalFormatting>"#;
    let (oracle, _) = read_xlsx(&minimal(xml, &[], "", "")).unwrap();
    assert_eq!(wb.sheets[0].cond_formats, oracle.sheets[0].cond_formats);
    assert!(!report.warnings.iter().any(|w| w.contains("were skipped")));
}

#[test]
fn xlsb_cf_external_workbook_is_still_refused() {
    let (wb, report) = read(&expression(&area3d(2, 0, 19, 0, 0, false), &[]));
    assert!(wb.sheets[0].cond_formats.is_empty());
    let warnings: Vec<_> = report.warnings.iter().filter(|w| w.contains("were skipped")).collect();
    assert_eq!(warnings, ["1 XLSB conditional formatting rule(s) on sheet 'Sheet1' were skipped: a formula uses a token this import can't decode."]);
}

#[test]
fn xlsb_cf_truncated_array_is_refused() {
    let (rgce, mut rgcb) = array_formula();
    rgcb.pop();
    let (wb, report) = read(&expression(&rgce, &rgcb));
    assert!(wb.sheets[0].cond_formats.is_empty());
    assert_eq!(report.warnings.iter().filter(|w| w.contains("were skipped")).count(), 1);
}

#[test]
fn xlsb_cf_expression_uses_workbook_links() {
    for (rgce, expected) in [(name_ptg(1), "Choices"), (area3d(1, 0, 19, 0, 0, false), "Lists!$A$1:$A$20")] {
        let (wb, report) = read(&expression(&rgce, &[]));
        assert_eq!(wb.sheets[0].cond_formats.len(), 1, "{:?}", report.warnings);
        assert!(matches!(&wb.sheets[0].cond_formats[0].rule, CfRule::Expression { formula, .. } if formula == expected));
        assert!(!report.warnings.iter().any(|w| w.contains("were skipped")));
    }
}
