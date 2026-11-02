//! XLSB structured references: `PtgList` tokens in cell formulas, and the calculated column
//! (`BrtListCCFmla`) and custom totals (`BrtListTrFmla`) formulas of a table, in packages
//! assembled from the public [MS-XLSB] record layouts and compared with the .xlsx reader.
use gridcraft_core::{CellRef, Value};
use gridcraft_model::{TotalsFn, Workbook};

use super::minimal;
use super::xlsb_tables_tests::{
    NULL, list_col, list_parts, number_cell, package, record, row, style_client, table_rels, text_cell, u32s, wide, worksheet,
};
use crate::{ReadReport, read_xlsb, read_xlsx, write_xlsx};

/// The table's `idList`: not its position, so the lookup by id is what's tested.
const ID: u32 = 7;
const SUM: [u8; 4] = [0x42, 1, 4, 0];

/// `PtgList` (first byte `ptg`, carrying the operand class) for table `id`: `rows` is the
/// rowType, `cols` the columns kind (0 all, 1 one, 2 a range), then colFirst and colLast.
fn list(ptg: u8, id: u32, rows: u16, cols: u16, c0: u16, c1: u16) -> Vec<u8> {
    let mut out = vec![ptg, 0x19, 0, 0];
    out.extend((cols | (rows << 2)).to_le_bytes());
    out.extend(id.to_le_bytes());
    out.extend(c0.to_le_bytes());
    out.extend(c1.to_le_bytes());
    out
}
/// A data-rows reference to one column of the table, value class.
fn column(c: u16) -> Vec<u8> {
    list(0x18, ID, 0, 1, c, c)
}
/// `[@Qty]*[@Price]`: this-row references (reference class), then PtgMul.
fn this_row_product() -> Vec<u8> {
    [list(0x38, ID, 0x10, 1, 1, 1), list(0x38, ID, 0x10, 1, 2, 2), vec![0x05]].concat()
}
/// BrtFmlaNum: column, cached result, then a CellParsedFormula.
fn fmla(col: u32, cached: f64, rgce: &[u8]) -> Vec<u8> {
    let mut data = u32s(&[col, 0]);
    data.extend(cached.to_le_bytes());
    data.extend([0; 2]);
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(9, &data)
}
/// A `BrtListCCFmla` / `BrtListTrFmla` payload: flags, then a ListParsedFormula.
fn list_fmla(id: u16, rgce: &[u8]) -> Vec<u8> {
    let mut data = vec![0, 0];
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(id, &data)
}
/// `SUBTOTAL(109,[Qty])*2`.
fn custom_total() -> Vec<u8> {
    [&[0x1E, 109, 0][..], &column(1), &[0x42, 2, 0x58, 0x01, 0x1E, 2, 0, 0x05]].concat()
}

/// Table `Sales` at A1:D4 (A1:D5 with a totals row): Item, Qty, Price and the calculated
/// column Total = `[@Qty]*[@Price]`; Qty has the custom total `SUBTOTAL(109,[Qty])*2`.
fn sales_table(totals: bool) -> Vec<u8> {
    let mut begin = u32s(&[0, 3 + u32::from(totals), 0, 3]);
    begin.extend(u32s(&[0, ID, 1, u32::from(totals), u32::from(totals)]));
    begin.extend(NULL.repeat(6));
    begin.extend(u32s(&[0]));
    begin.extend(wide("Sales"));
    begin.extend(wide("Sales"));
    begin.extend(NULL.repeat(4));
    let cols = [
        list_col(1, "Item", 0, None, &[]),
        list_col(2, "Qty", 9, None, &list_fmla(352, &custom_total())),
        list_col(3, "Price", 0, None, &[]),
        list_col(4, "Total", 0, None, &list_fmla(351, &this_row_product())),
    ]
    .concat();
    [record(343, &begin), record(345, &u32s(&[4])), cols, record(346, &[]), style_client("TableStyleMedium2", 4), record(344, &[])].concat()
}

/// The sheet: the table's cells, then the formulas of column E (`extra` per row).
fn sheet(totals: bool, extra: &[Vec<u8>]) -> Vec<u8> {
    let mut data = [row(0), text_cell(0, "Item"), text_cell(1, "Qty"), text_cell(2, "Price"), text_cell(3, "Total")].concat();
    data.extend(extra.first().cloned().unwrap_or_default());
    for (r, (item, qty, price)) in [("a", 2.0, 3.0), ("b", 4.0, 5.0), ("c", 6.0, 7.0)].into_iter().enumerate() {
        let r = r + 1;
        data.extend(
            [row(r as u32), text_cell(0, item), number_cell(1, qty), number_cell(2, price), fmla(3, qty * price, &this_row_product())].concat(),
        );
        data.extend(extra.get(r).cloned().unwrap_or_default());
    }
    if totals {
        data.extend([row(4), fmla(1, 24.0, &custom_total())].concat());
    }
    for (r, cells) in extra.iter().enumerate().skip(4) {
        data.extend([row(r as u32), cells.clone()].concat());
    }
    worksheet(&data, &list_parts(&["rIdT1"]))
}

fn read(totals: bool, extra: &[Vec<u8>]) -> (Workbook, ReadReport) {
    read_xlsb(&package(&sheet(totals, extra), Some(&table_rels(1)), &[sales_table(totals)])).unwrap()
}
fn formula(wb: &Workbook, cell: &str) -> Option<String> {
    wb.sheets[0].cell(CellRef::parse(cell).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}
fn kept(report: &ReadReport) -> bool {
    report.warnings.iter().any(|w| w.contains("kept as their saved results"))
}

/// Column E's formulas, one per row from E1.
fn cell_formulas() -> Vec<Vec<u8>> {
    vec![
        fmla(4, 12.0, &[column(1), SUM.to_vec()].concat()),
        fmla(4, 68.0, &[column(1), column(2), vec![0x42, 2, 228, 0]].concat()),
        fmla(4, 4.0, &[list(0x18, ID, 0x01, 0, 0, 0), vec![0x41, 76, 0]].concat()),
        fmla(4, 3.0, &[list(0x58, ID, 0x02, 2, 0, 2), vec![0x42, 1, 169, 0]].concat()),
    ]
}

#[test]
fn xlsb_structured_references_in_cell_formulas() {
    let (wb, report) = read(false, &cell_formulas());
    assert_eq!(formula(&wb, "E1").as_deref(), Some("SUM(Sales[Qty])"));
    assert_eq!(formula(&wb, "E2").as_deref(), Some("SUMPRODUCT(Sales[Qty],Sales[Price])"));
    assert_eq!(formula(&wb, "E3").as_deref(), Some("ROWS(Sales[#All])"));
    assert_eq!(formula(&wb, "E4").as_deref(), Some("COUNTA(Sales[[#Headers],[Item]:[Price]])"));
    // Inside the table a this-row reference leaves out the table name.
    for cell in ["D2", "D3", "D4"] {
        assert_eq!(formula(&wb, cell).as_deref(), Some("[@Qty]*[@Price]"), "{cell}");
    }
    assert!(!kept(&report), "{:?}", report.warnings);

    // The .xlsx reader gives the same text for the same formulas.
    let body = r#"<sheetData><row r="1"><c r="E1"><f>SUM(Sales[Qty])</f></c></row><row r="2"><c r="D2"><f>Sales[[#This Row],[Qty]]*Sales[[#This Row],[Price]]</f></c><c r="E2"><f>SUMPRODUCT(Sales[Qty],Sales[Price])</f></c></row><row r="3"><c r="E3"><f>ROWS(Sales[#All])</f></c></row><row r="4"><c r="E4"><f>COUNTA(Sales[[#Headers],[Item]:[Price]])</f></c></row></sheetData>"#;
    let (xwb, _) = read_xlsx(&minimal(body, &[], "", "")).unwrap();
    for cell in ["E1", "E2", "E3", "E4"] {
        assert_eq!(formula(&wb, cell), formula(&xwb, cell), "{cell}");
    }
}

#[test]
fn xlsb_structured_references_to_missing_tables_or_columns_keep_the_saved_value() {
    let mut extra = cell_formulas();
    // E5: an unknown listIndex; E6: colLast past the table's four columns.
    extra.push(fmla(4, 41.0, &[list(0x18, 99, 0, 1, 1, 1), SUM.to_vec()].concat()));
    extra.push(fmla(4, 42.0, &[list(0x18, ID, 0, 2, 1, 9), SUM.to_vec()].concat()));
    let (wb, report) = read(false, &extra);
    for (cell, saved) in [("E5", 41.0), ("E6", 42.0)] {
        assert_eq!(formula(&wb, cell), None, "{cell}");
        assert_eq!(wb.sheets[0].value(CellRef::parse(cell).unwrap()), Value::Number(saved));
    }
    assert!(kept(&report));
    assert_eq!(report.warnings.iter().filter(|w| w.contains("kept as their saved results")).count(), 1);
    assert_eq!(formula(&wb, "E1").as_deref(), Some("SUM(Sales[Qty])"));

    // Truncated or foreign tokens are refused the same way, never a crash.
    let full = [column(1), SUM.to_vec()].concat();
    // Every cut but the one after the whole PtgList (a complete formula on its own).
    let mut bad: Vec<Vec<u8>> = (1..full.len()).filter(|&n| n != 14).map(|n| full[..n].to_vec()).collect();
    bad.push([&[0x18u8, 0x1A][..], &full[2..]].concat());
    bad.push(list(0x18, ID, 0x03, 1, 1, 1));
    bad.push(list(0x18, ID, 0, 3, 1, 1));
    bad.push([list(0x18, ID, 0, 1, 1, 1), SUM.to_vec()].concat().into_iter().enumerate().map(|(i, b)| if i == 5 { b | 0x30 } else { b }).collect());
    let extra: Vec<Vec<u8>> = bad.iter().map(|rgce| fmla(4, 1.0, rgce)).collect();
    let (wb, report) = read(false, &extra);
    for r in 0..extra.len() {
        assert_eq!(formula(&wb, &format!("E{}", r + 1)), None, "row {r}");
    }
    assert!(kept(&report));
}

#[test]
fn xlsb_calculated_column_and_custom_total_formulas() {
    let (wb, report) = read(true, &[]);
    let t = &wb.sheets[0].tables[0];
    assert_eq!(t.columns[3].formula.as_deref(), Some("[@Qty]*[@Price]"));
    assert_eq!(t.columns[1].totals, TotalsFn::Custom);
    assert_eq!(t.columns[1].totals_label.as_deref(), Some("SUBTOTAL(109,[Qty])*2"));
    assert!(!report.warnings.iter().any(|w| w.contains("calculated column") || w.contains("totals row")), "{:?}", report.warnings);
    // The totals cell holds a data-rows reference, which keeps the table name like in .xlsx.
    assert_eq!(formula(&wb, "B5").as_deref(), Some("SUBTOTAL(109,Sales[Qty])*2"));

    // The .xlsx reader stores the same texts for the same table.
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdT1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/></Relationships>"#;
    let table = r#"<table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="1" name="Sales" displayName="Sales" ref="A1:D5" totalsRowCount="1"><tableColumns count="4"><tableColumn id="1" name="Item"/><tableColumn id="2" name="Qty" totalsRowFunction="custom"><totalsRowFormula>SUBTOTAL(109,[Qty])*2</totalsRowFormula></tableColumn><tableColumn id="3" name="Price"/><tableColumn id="4" name="Total"><calculatedColumnFormula>[[#This Row],[Qty]]*[[#This Row],[Price]]</calculatedColumnFormula></tableColumn></tableColumns><tableStyleInfo name="TableStyleMedium2" showRowStripes="1"/></table>"#;
    let body = r#"<sheetData/><tableParts count="1"><tablePart r:id="rIdT1"/></tableParts>"#;
    let (xwb, _) = read_xlsx(&minimal(body, &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/tables/table1.xml", table)], "", "")).unwrap();
    let x = &xwb.sheets[0].tables[0];
    for i in 0..4 {
        assert_eq!((&t.columns[i].formula, &t.columns[i].totals_label), (&x.columns[i].formula, &x.columns[i].totals_label), "column {i}");
    }
}

#[test]
fn xlsb_list_formulas_survive_xlsx_save_and_reopen() {
    let (wb, _) = read(true, &cell_formulas());
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    for cell in ["E1", "E2", "E3", "E4", "D2", "D3", "D4", "B5"] {
        assert!(formula(&wb, cell).is_some(), "{cell}");
        assert_eq!(formula(&back, cell), formula(&wb, cell), "{cell}");
    }
    let (t, b) = (&wb.sheets[0].tables[0], &back.sheets[0].tables[0]);
    assert_eq!(b.columns[3].formula, t.columns[3].formula);
    assert_eq!((b.columns[1].totals, &b.columns[1].totals_label), (TotalsFn::Custom, &t.columns[1].totals_label));
}

#[test]
fn malformed_column_formulas_warn_and_keep_the_table() {
    let mut begin_cols = sales_table(false);
    // A calculated column formula naming a missing table column: the table stays, the formula goes.
    let bad = list_fmla(351, &[list(0x18, ID, 0x10, 1, 8, 8)].concat());
    let good = list_fmla(351, &this_row_product());
    let at = begin_cols.windows(good.len()).position(|w| w == good.as_slice()).unwrap();
    begin_cols.splice(at..at + good.len(), bad);
    let (wb, report) = read_xlsb(&package(&sheet(false, &[]), Some(&table_rels(1)), &[begin_cols])).unwrap();
    let t = &wb.sheets[0].tables[0];
    assert_eq!(t.columns[3].formula, None);
    assert!(report.warnings.iter().any(|w| w.contains("calculated column formulas")));
}
