//! A small sales list ("Data" A1:C5: Region, Product, Sales) with a PivotTable on "Report", built
//! in code twice: as an .xlsb package (records from the public [MS-XLSB] layouts) and as the
//! equivalent .xlsx package (hand-written SpreadsheetML), so tests can check that both readers
//! give the same PivotTable and that it refreshes. The calculated-field cases add a Units column
//! (Data!D) and calculated fields over it.

use std::io::Write;

/// The PivotTable variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotCase {
    /// Rows Region, columns Product, values Sum of Sales, at Report!A3.
    Basic,
    /// Page field Region = East, rows Product, values Sum of Sales.
    PageFilter,
    /// As `Basic` with Count of Sales.
    Count,
    /// As `Basic`, the cache source being table "SalesTbl" over Data!A1:C5.
    TableSource,
    /// As `Basic` without the cache definition part.
    MissingCache,
    /// As `Basic` with a truncated `BrtBeginSXView` record.
    TruncatedView,
    /// Cache fields Region, Product, Sales, Units and calculated Price = Sales/Units; rows
    /// Region, values Sum of Sales and Sum of Price (Σ Values in Columns), at Report!A3.
    Calculated,
    /// As `Calculated` plus `Unit Bonus` = Price*0.1 and `Half Bonus` = 'Unit Bonus'/2, which
    /// use calculated fields.
    CalculatedChain,
    /// As `Calculated` plus `Bad`, whose formula is a token the import refuses (`PtgExp`), and
    /// values Sum of Sales, Sum of Bad and Sum of Price.
    CalculatedUnsupported,
}

impl PivotCase {
    fn calculated(self) -> bool {
        matches!(self, PivotCase::Calculated | PivotCase::CalculatedChain | PivotCase::CalculatedUnsupported)
    }
}

/// Units of the `DATA` records in the calculated-field cases.
pub const UNITS: [f64; 4] = [2.0, 4.0, 3.0, 10.0];

/// `PtgSxName` ([MS-XLSB] 2.5.98.16): 0x18, eptg 0x1D, then `sxIndex` (the position of the
/// field in the formula's PNAMES collection).
fn sx_name(index: u32) -> Vec<u8> {
    [vec![0x18, 0x1D], index.to_le_bytes().to_vec()].concat()
}

/// A calculated cache field: name, the .xlsx `formula` attribute, the `rgce` tokens and the
/// cache fields (`ifdb`) its `PtgSxName` tokens refer to.
struct Calc {
    name: &'static str,
    xlsx: &'static str,
    rgce: Vec<u8>,
    refs: Vec<u32>,
}

/// The calculated fields of `case`, after the four source fields (indices 4, 5, …).
fn calcs(case: PivotCase) -> Vec<Calc> {
    if !case.calculated() {
        return vec![];
    }
    // Price = Sales/Units: two field references and PtgDiv (0x06).
    let mut out = vec![Calc { name: "Price", xlsx: "Sales/Units", rgce: [sx_name(0), sx_name(1), vec![0x06]].concat(), refs: vec![2, 3] }];
    match case {
        PivotCase::CalculatedChain => {
            // Unit Bonus = Price*0.1: PtgNum (0x1F, f64) and PtgMul (0x05).
            let num = [vec![0x1F], 0.1f64.to_le_bytes().to_vec()].concat();
            out.push(Calc { name: "Unit Bonus", xlsx: "Price*0.1", rgce: [sx_name(0), num, vec![0x05]].concat(), refs: vec![4] });
            // Half Bonus = 'Unit Bonus'/2: PtgInt (0x1E, u16) and PtgDiv.
            out.push(Calc { name: "Half Bonus", xlsx: "'Unit Bonus'/2", rgce: [sx_name(0), vec![0x1E, 2, 0, 0x06]].concat(), refs: vec![5] });
        }
        PivotCase::CalculatedUnsupported => {
            // PtgExp (0x01) and a row: never valid in a PivotParsedFormula.
            out.push(Calc { name: "Bad", xlsx: "", rgce: vec![0x01, 0, 0, 0, 0], refs: vec![] });
        }
        _ => {}
    }
    out
}

/// The data fields of `case`: (cache field, caption).
fn data_fields(case: PivotCase) -> Vec<(u32, &'static str)> {
    match case {
        PivotCase::Count => vec![(2, "Count of Sales")],
        PivotCase::Calculated | PivotCase::CalculatedChain => vec![(2, "Sum of Sales"), (4, "Sum of Price")],
        PivotCase::CalculatedUnsupported => vec![(2, "Sum of Sales"), (5, "Sum of Bad"), (4, "Sum of Price")],
        _ => vec![(2, "Sum of Sales")],
    }
}

const DATA: [(&str, &str, f64); 4] = [("East", "A", 10.0), ("West", "A", 20.0), ("East", "B", 30.0), ("West", "B", 40.0)];
const HEADERS: [&str; 3] = ["Region", "Product", "Sales"];

fn zip(parts: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in parts {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(data).unwrap();
    }
    z.finish().unwrap().into_inner()
}

// ================================================================ .xlsb

/// One BIFF12 record: variable-length type and size, then the payload.
pub fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
    let mut out = vec![];
    if id < 128 {
        out.push(id as u8);
    } else {
        out.extend([(id as u8 & 0x7f) | 0x80, (id >> 7) as u8]);
    }
    let mut len = bytes.len();
    loop {
        let byte = (len & 0x7f) as u8;
        len >>= 7;
        out.push(if len == 0 { byte } else { byte | 0x80 });
        if len == 0 {
            break;
        }
    }
    out.extend(bytes);
    out
}
fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}
fn u32s(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn row(r: u32) -> Vec<u8> {
    let mut out = r.to_le_bytes().to_vec();
    out.extend([0; 13]);
    record(0, &out)
}
fn text_cell(col: u32, s: &str) -> Vec<u8> {
    let mut data = u32s(&[col, 0]);
    data.extend(wide(s));
    record(6, &data)
}
fn number_cell(col: u32, n: f64) -> Vec<u8> {
    let mut data = u32s(&[col, 0]);
    data.extend(n.to_le_bytes());
    record(5, &data)
}
fn worksheet(rows: &[(u32, Vec<Vec<u8>>)], tail: &[u8]) -> Vec<u8> {
    let mut out = [record(129, &[]), record(145, &[])].concat();
    for (r, cells) in rows {
        out.extend(row(*r));
        out.extend(cells.concat());
    }
    [out, record(146, &[]), tail.to_vec(), record(130, &[])].concat()
}

/// The Data sheet: headers and four records, plus table "SalesTbl" for `TableSource` and a
/// Units column for the calculated-field cases.
fn data_sheet(table: bool, units: bool) -> Vec<u8> {
    let mut header: Vec<Vec<u8>> = HEADERS.iter().enumerate().map(|(c, h)| text_cell(c as u32, h)).collect();
    if units {
        header.push(text_cell(3, "Units"));
    }
    let mut rows = vec![(0, header)];
    for (r, (region, product, sales)) in DATA.iter().enumerate() {
        let mut cells = vec![text_cell(0, region), text_cell(1, product), number_cell(2, *sales)];
        if units {
            cells.push(number_cell(3, UNITS[r]));
        }
        rows.push((r as u32 + 1, cells));
    }
    let tail = if table { [record(660, &u32s(&[1])), record(661, &wide("rIdT1")), record(662, &[])].concat() } else { vec![] };
    worksheet(&rows, &tail)
}

/// The report sheet holds the last rendered values of the Basic report at A3:D7.
fn report_sheet() -> Vec<u8> {
    let rows = vec![
        (2, vec![text_cell(0, "Sum of Sales"), text_cell(1, "Column Labels")]),
        (3, vec![text_cell(0, "Row Labels"), text_cell(1, "A"), text_cell(2, "B"), text_cell(3, "Grand Total")]),
        (4, vec![text_cell(0, "East"), number_cell(1, 10.0), number_cell(2, 30.0), number_cell(3, 40.0)]),
        (5, vec![text_cell(0, "West"), number_cell(1, 20.0), number_cell(2, 40.0), number_cell(3, 60.0)]),
        (6, vec![text_cell(0, "Grand Total"), number_cell(1, 30.0), number_cell(2, 70.0), number_cell(3, 100.0)]),
    ];
    worksheet(&rows, &[])
}

/// The "SalesTbl" table part over Data!A1:C5.
fn table_part() -> Vec<u8> {
    let mut begin = u32s(&[0, 4, 0, 2]);
    begin.extend(u32s(&[0, 1, 1, 0, 0]));
    begin.extend([0xff; 24]); // No differential formats.
    begin.extend(u32s(&[0]));
    begin.extend(wide("SalesTbl"));
    begin.extend(wide("SalesTbl"));
    begin.extend([0xff; 16]); // Comment and three cell style names.
    let mut out = [record(343, &begin), record(345, &u32s(&[3]))].concat();
    for (i, name) in HEADERS.iter().enumerate() {
        let mut col = u32s(&[i as u32 + 1, 0]);
        col.extend([0xff; 12]);
        col.extend(u32s(&[0]));
        col.extend([0xff; 4]);
        col.extend(wide(name));
        col.extend([0xff; 16]);
        out.extend([record(347, &col), record(348, &[])].concat());
    }
    let mut style = 4u16.to_le_bytes().to_vec();
    style.extend(wide("TableStyleMedium2"));
    [out, record(346, &[]), record(513, &style), record(344, &[])].concat()
}

/// `BrtBeginPCDField` … `BrtEndPCDField` for a source field, with its shared items `items`.
fn cache_field(name: &str, atbl_flags: u16, minmax: Option<(f64, f64)>, items: &[Vec<u8>], count: u32) -> Vec<u8> {
    let mut field = 0x0004u16.to_le_bytes().to_vec(); // fSrcField
    field.extend([0; 18]);
    field.extend(wide(name));
    let mut atbl = atbl_flags.to_le_bytes().to_vec();
    atbl.extend(count.to_le_bytes());
    if let Some((min, max)) = minmax {
        atbl.extend(min.to_le_bytes());
        atbl.extend(max.to_le_bytes());
    }
    [record(183, &field), record(189, &atbl), items.concat(), record(190, &[]), record(184, &[])].concat()
}

/// `BrtBeginPCDField` … `BrtEndPCDField` for a calculated field: flags fLoadFmla (0x0100), not
/// fSrcField; ifmt, wTypeSql, ihdb, isxtl, cIsxtmps (18 zero bytes); stFldName; fldFmla, a
/// PivotParsedFormula (cce, rgce, cb = 0). Then PNAMES: `BrtBeginPNames` (253, cnames), per
/// referenced field `BrtBeginPName` (255: ifdb, ifn 0xFF "not specified", flags 0) and
/// `BrtEndPName` (256), and `BrtEndPNames` (254).
fn calc_field(c: &Calc) -> Vec<u8> {
    let mut field = 0x0100u16.to_le_bytes().to_vec();
    field.extend([0; 18]);
    field.extend(wide(c.name));
    field.extend(u32s(&[c.rgce.len() as u32]));
    field.extend(&c.rgce);
    field.extend(u32s(&[0]));
    let mut out = record(183, &field);
    if !c.refs.is_empty() {
        out.extend(record(253, &u32s(&[c.refs.len() as u32])));
        for ifdb in &c.refs {
            let mut name = ifdb.to_le_bytes().to_vec();
            name.extend([0xff, 0]);
            out.extend([record(255, &name), record(256, &[])].concat());
        }
        out.extend(record(254, &[]));
    }
    out.extend(record(184, &[]));
    out
}

/// The PivotCache Definition part.
pub fn cache_part(case: PivotCase) -> Vec<u8> {
    // Version bytes, fRefreshOnLoad and fEnableRefresh, citmGhostMax, refresh date, flags,
    // cRecords and the unused field.
    let mut def = vec![3, 3, 3, 0x14];
    def.extend([0xff; 4]);
    def.extend(45_000.0f64.to_le_bytes());
    def.push(0);
    def.extend([0; 8]);
    let range = if case == PivotCase::TableSource {
        [vec![1, 0, 0], wide("SalesTbl")].concat()
    } else {
        [vec![0, 0, 2], wide("Data"), u32s(&[0, 4, 0, if case.calculated() { 3 } else { 2 }])].concat()
    };
    let calcs = calcs(case);
    let mut extra = vec![];
    if case.calculated() {
        extra.push(cache_field("Units", 0x01c0, Some((2.0, 10.0)), &[], 0));
    }
    extra.extend(calcs.iter().map(calc_field));
    let count = if case.calculated() { 4 + calcs.len() as u32 } else { 3 };
    let strings = |items: &[&str]| -> Vec<Vec<u8>> { items.iter().map(|s| record(24, &wide(s))).collect() };
    let mut run = 2u16.to_le_bytes().to_vec();
    run.extend(u32s(&[2]));
    run.extend([wide("A"), wide("B")].concat());
    [
        record(179, &def),
        record(185, &u32s(&[0, 0])),
        record(187, &range),
        record(188, &[]),
        record(186, &[]),
        record(181, &u32s(&[count])),
        cache_field("Region", 0x0009, None, &strings(&["East", "West"]), 2),
        cache_field("Product", 0x0009, None, &[record(191, &run), record(192, &[])], 2),
        cache_field("Sales", 0x01c0, Some((10.0, 40.0)), &[], 0),
        extra.concat(),
        record(182, &[]),
        record(180, &[]),
    ]
    .concat()
}

/// `BrtBeginSXVD` … `BrtEndSXVD` on `axis` (SXAxis bits), with `items` data items in cache
/// order and a default subtotal item when it has any.
fn pivot_field(axis: u8, items: u32) -> Vec<u8> {
    let w0 = u32::from(axis) | if items > 0 { 1 << 8 } else { 0 } | 1 << 28; // fDefault, fCompact
    let w2 = 0x1f | 1 << 6 | 1 << 8; // fDragTo*, fOutline, fSubtotalAtTop
    let mut vd = u32s(&[w0, 0, w2, 10]);
    vd.extend([0xff; 4]);
    let mut out = record(285, &vd);
    if items > 0 {
        out.extend(record(283, &u32s(&[items + 1])));
        for i in 0..items {
            let mut vi = vec![0, 0, 0];
            vi.extend(i.to_le_bytes());
            out.extend([record(282, &vi), record(281, &[])].concat());
        }
        out.extend([record(282, &[1, 0, 0, 0xff, 0xff, 0xff, 0xff]), record(281, &[]), record(284, &[])].concat());
    }
    out.extend(record(286, &[]));
    out
}

/// The PivotTable part.
pub fn view_part(case: PivotCase) -> Vec<u8> {
    let page = case == PivotCase::PageFilter;
    let w0 = 3; // bVerSxMacro
    let w1 = 1 << 4 | 1 << 5 | 1 << 13 | 1 << 14 | 1 << 19; // wizard, drilldown, grand totals, fDisplayData
    let w2 = 0x0f | 1 << 6 | 1 << 7; // compact/outline defaults, no error or null strings
    let mut view = u32s(&[w0, w1, w2]);
    view.extend([2, 0, 3, 3]); // Σ Values on columns
    view.extend([0xff; 4]); // ipos4Data -1
    view.extend([0; 8]);
    view.extend(u32s(&[1])); // idCache
    view.extend(wide("PivotTable1"));
    view.extend(wide("Values"));
    if case == PivotCase::TruncatedView {
        view.truncate(10);
    }
    // rfxGeom, rwFirstHead, rwFirstData, colFirstData, crwPage, ccolPage
    let location = if page {
        u32s(&[2, 5, 0, 1, 3, 3, 1, 1, 1])
    } else if case.calculated() {
        u32s(&[2, 5, 0, 2, 3, 3, 1, 0, 0])
    } else {
        u32s(&[2, 6, 0, 3, 3, 4, 1, 0, 0])
    };
    let func = if case == PivotCase::Count { 1 } else { 0 };
    let data_fields = data_fields(case);
    // BrtBeginSXDI: isxvdData, iiftab, df, isxvd, isxvi, ifmt, fName and stDisplayName.
    let data: Vec<u8> = data_fields
        .iter()
        .flat_map(|(field, name)| {
            let mut data = u32s(&[*field, func, 0, 0, 0, 0]);
            data.push(1);
            data.extend(wide(name));
            [record(293, &data), record(294, &[])].concat()
        })
        .collect();
    // Pivot fields after Sales: Units (no axis) and the calculated fields (data).
    let extra: Vec<u8> =
        if case.calculated() { [pivot_field(0, 0), calcs(case).iter().flat_map(|_| pivot_field(8, 0)).collect()].concat() } else { vec![] };
    let fields = if case.calculated() { 4 + calcs(case).len() as u32 } else { 3 };
    let mut style = 0x0030u16.to_le_bytes().to_vec();
    style.extend(wide("PivotStyleLight16"));
    let mut out = [
        record(280, &view),
        record(314, &location),
        record(313, &[]),
        record(287, &u32s(&[fields])),
        pivot_field(if page { 4 } else { 1 }, 2),
        pivot_field(
            if page {
                1
            } else if case.calculated() {
                0
            } else {
                2
            },
            2,
        ),
        pivot_field(8, 0),
        extra,
        record(288, &[]),
    ]
    .concat();
    if page {
        out.extend([record(309, &u32s(&[1, 1])), record(310, &[])].concat());
        let mut pi = u32s(&[0, 0]);
        pi.extend([0xff; 4]);
        pi.push(0);
        out.extend([record(291, &u32s(&[1])), record(289, &pi), record(290, &[]), record(292, &[])].concat());
    } else {
        // Columns: Product, or Σ Values (-2) in the calculated-field cases.
        let col = if case.calculated() { (-2i32) as u32 } else { 1 };
        out.extend([record(309, &u32s(&[1, 0])), record(310, &[]), record(311, &u32s(&[1, col])), record(312, &[])].concat());
    }
    out.extend([record(295, &u32s(&[data_fields.len() as u32])), data, record(296, &[])].concat());
    out.extend([record(513, &style), record(315, &[])].concat());
    out
}

const RELS_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rels(list: &[(&str, &str, &str)]) -> String {
    let mut s = format!(r#"<?xml version="1.0"?><Relationships xmlns="{RELS_NS}">"#);
    for (id, kind, target) in list {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// The .xlsb package for `case`.
pub fn xlsb(case: PivotCase) -> Vec<u8> {
    xlsb_with(case, &cache_part(case), &view_part(case))
}

/// The .xlsb package for `case` with the given cache and PivotTable parts.
pub fn xlsb_with(case: PivotCase, cache: &[u8], view: &[u8]) -> Vec<u8> {
    let table = case == PivotCase::TableSource;
    let mut types = String::from(r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">"#);
    for (part, ty) in [
        ("workbook.bin", "sheet.binary.macroEnabled.main"),
        ("worksheets/sheet1.bin", "worksheet"),
        ("worksheets/sheet2.bin", "worksheet"),
        ("pivotTables/pivotTable1.bin", "pivotTable"),
        ("pivotCache/pivotCacheDefinition1.bin", "pivotCacheDefinition"),
        ("tables/table1.bin", "table"),
    ] {
        types.push_str(&format!(r#"<Override PartName="/xl/{part}" ContentType="application/vnd.ms-excel.{ty}"/>"#));
    }
    types.push_str("</Types>");
    let mut book = [record(131, &[]), record(143, &[])].concat();
    for (i, name) in ["Data", "Report"].iter().enumerate() {
        let mut sheet = u32s(&[0, i as u32 + 1]);
        sheet.extend(wide(&format!("rId{}", i + 1)));
        sheet.extend(wide(name));
        book.extend(record(156, &sheet));
    }
    book.extend(record(144, &[]));
    let mut cache_id = u32s(&[1]);
    cache_id.extend(wide("rId3"));
    book.extend([record(384, &[]), record(386, &cache_id), record(387, &[]), record(385, &[]), record(132, &[])].concat());

    let root = rels(&[("rId1", "officeDocument", "xl/workbook.bin")]);
    let book_rels = rels(&[
        ("rId1", "worksheet", "worksheets/sheet1.bin"),
        ("rId2", "worksheet", "worksheets/sheet2.bin"),
        ("rId3", "pivotCacheDefinition", "pivotCache/pivotCacheDefinition1.bin"),
    ]);
    let data_rels = rels(&[("rIdT1", "table", "../tables/table1.bin")]);
    let report_rels = rels(&[("rId1", "pivotTable", "../pivotTables/pivotTable1.bin")]);
    let view_rels = rels(&[("rId1", "pivotCacheDefinition", "../pivotCache/pivotCacheDefinition1.bin")]);
    let (data, report, table_bin) = (data_sheet(table, case.calculated()), report_sheet(), table_part());
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.bin", &book),
        ("xl/_rels/workbook.bin.rels", book_rels.as_bytes()),
        ("xl/worksheets/sheet1.bin", &data),
        ("xl/worksheets/sheet2.bin", &report),
        ("xl/worksheets/_rels/sheet2.bin.rels", report_rels.as_bytes()),
        ("xl/pivotTables/pivotTable1.bin", view),
        ("xl/pivotTables/_rels/pivotTable1.bin.rels", view_rels.as_bytes()),
    ];
    if case != PivotCase::MissingCache {
        parts.push(("xl/pivotCache/pivotCacheDefinition1.bin", cache));
    }
    if table {
        parts.push(("xl/worksheets/_rels/sheet1.bin.rels", data_rels.as_bytes()));
        parts.push(("xl/tables/table1.bin", &table_bin));
    }
    zip(&parts)
}

// ================================================================ .xlsx

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";

fn sheet_xml(rows: &[(u32, Vec<String>)], tail: &str) -> String {
    let mut s = format!(r#"<?xml version="1.0"?><worksheet xmlns="{MAIN_NS}" xmlns:r="{REL}"><sheetData>"#);
    for (r, cells) in rows {
        s.push_str(&format!(r#"<row r="{r}">{}</row>"#, cells.concat()));
    }
    s.push_str("</sheetData>");
    s.push_str(tail);
    s.push_str("</worksheet>");
    s
}
fn text_xml(cell: &str, text: &str) -> String {
    format!(r#"<c r="{cell}" t="inlineStr"><is><t>{text}</t></is></c>"#)
}
fn number_xml(cell: &str, n: f64) -> String {
    format!(r#"<c r="{cell}"><v>{n}</v></c>"#)
}

/// The .xlsx package equivalent to `xlsb(case)` (`Basic` for the damaged cases).
pub fn xlsx(case: PivotCase) -> Vec<u8> {
    let table = case == PivotCase::TableSource;
    let page = case == PivotCase::PageFilter;
    let calc = case.calculated();
    let mut header: Vec<String> = HEADERS.iter().zip(["A1", "B1", "C1"]).map(|(h, c)| text_xml(c, h)).collect();
    if calc {
        header.push(text_xml("D1", "Units"));
    }
    let mut data_rows = vec![(1, header)];
    for (i, (region, product, sales)) in DATA.iter().enumerate() {
        let r = i + 2;
        let mut cells = vec![text_xml(&format!("A{r}"), region), text_xml(&format!("B{r}"), product), number_xml(&format!("C{r}"), *sales)];
        if calc {
            cells.push(number_xml(&format!("D{r}"), UNITS[i]));
        }
        data_rows.push((r as u32, cells));
    }
    let data = sheet_xml(&data_rows, if table { r#"<tableParts count="1"><tablePart r:id="rIdT1"/></tableParts>"# } else { "" });
    let report = sheet_xml(&[(3, vec![text_xml("A3", "Sum of Sales")])], "");
    let book = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{MAIN_NS}" xmlns:r="{REL}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Report" sheetId="2" r:id="rId2"/></sheets><pivotCaches><pivotCache cacheId="1" r:id="rId3"/></pivotCaches></workbook>"#
    );
    let source = if table {
        r#"<worksheetSource name="SalesTbl"/>"#
    } else if calc {
        r#"<worksheetSource ref="A1:D5" sheet="Data"/>"#
    } else {
        r#"<worksheetSource ref="A1:C5" sheet="Data"/>"#
    };
    let calcs = calcs(case);
    let mut extra_fields = String::new();
    if calc {
        extra_fields.push_str(r#"<cacheField name="Units" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" containsInteger="1" minValue="2" maxValue="10"/></cacheField>"#);
    }
    for c in &calcs {
        let formula = c.xlsx.replace('\'', "&apos;");
        extra_fields.push_str(&format!(r#"<cacheField name="{}" numFmtId="0" formula="{formula}" databaseField="0"/>"#, c.name));
    }
    let field_count = if calc { 4 + calcs.len() } else { 3 };
    let cache = format!(
        r#"<?xml version="1.0"?><pivotCacheDefinition xmlns="{MAIN_NS}" xmlns:r="{REL}" refreshOnLoad="1" createdVersion="3" refreshedVersion="3" minRefreshableVersion="3" recordCount="0"><cacheSource type="worksheet">{source}</cacheSource><cacheFields count="{field_count}"><cacheField name="Region" numFmtId="0"><sharedItems count="2"><s v="East"/><s v="West"/></sharedItems></cacheField><cacheField name="Product" numFmtId="0"><sharedItems count="2"><s v="A"/><s v="B"/></sharedItems></cacheField><cacheField name="Sales" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1" containsInteger="1" minValue="10" maxValue="40"/></cacheField>{extra_fields}</cacheFields></pivotCacheDefinition>"#
    );
    let items = r#"<items count="3"><item x="0"/><item x="1"/><item t="default"/></items>"#;
    let (location, fields, areas) = if calc {
        (
            r#"<location ref="A3:C6" firstHeaderRow="0" firstDataRow="1" firstDataCol="1"/>"#,
            format!(r#"<pivotField axis="axisRow" showAll="0">{items}</pivotField><pivotField showAll="0">{items}</pivotField>"#),
            r#"<rowFields count="1"><field x="0"/></rowFields><colFields count="1"><field x="-2"/></colFields>"#,
        )
    } else if page {
        (
            r#"<location ref="A3:B6" firstHeaderRow="1" firstDataRow="1" firstDataCol="1" rowPageCount="1" colPageCount="1"/>"#,
            format!(r#"<pivotField axis="axisPage" showAll="0">{items}</pivotField><pivotField axis="axisRow" showAll="0">{items}</pivotField>"#),
            r#"<rowFields count="1"><field x="1"/></rowFields><rowItems count="3"><i><x/></i><i><x v="1"/></i><i t="grand"><x/></i></rowItems><colItems count="1"><i/></colItems><pageFields count="1"><pageField fld="0" item="0" hier="-1"/></pageFields>"#,
        )
    } else {
        (
            r#"<location ref="A3:D7" firstHeaderRow="1" firstDataRow="2" firstDataCol="1"/>"#,
            format!(r#"<pivotField axis="axisRow" showAll="0">{items}</pivotField><pivotField axis="axisCol" showAll="0">{items}</pivotField>"#),
            r#"<rowFields count="1"><field x="0"/></rowFields><colFields count="1"><field x="1"/></colFields>"#,
        )
    };
    let subtotal = if case == PivotCase::Count { r#" subtotal="count""# } else { "" };
    let data_list = data_fields(case);
    let data_field: String =
        data_list.iter().map(|(fld, name)| format!(r#"<dataField name="{name}" fld="{fld}"{subtotal} baseField="0" baseItem="0"/>"#)).collect();
    let data_count = data_list.len();
    let extra_pivot_fields = if calc {
        format!(r#"<pivotField showAll="0"/>{}"#, r#"<pivotField dataField="1" showAll="0"/>"#.repeat(calcs.len()))
    } else {
        String::new()
    };
    let view = format!(
        r#"<?xml version="1.0"?><pivotTableDefinition xmlns="{MAIN_NS}" name="PivotTable1" cacheId="1" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" applyWidthHeightFormats="1" dataCaption="Values" updatedVersion="3" minRefreshableVersion="3" useAutoFormatting="1" itemPrintTitles="1" createdVersion="3" indent="0" outline="1" outlineData="1" multipleFieldFilters="0">{location}<pivotFields count="{field_count}">{fields}<pivotField dataField="1" showAll="0"/>{extra_pivot_fields}</pivotFields>{areas}<dataFields count="{data_count}">{data_field}</dataFields><pivotTableStyleInfo name="PivotStyleLight16" showRowHeaders="1" showColHeaders="1" showRowStripes="0" showColStripes="0" showLastColumn="1"/></pivotTableDefinition>"#
    );
    let table_xml = format!(
        r#"<?xml version="1.0"?><table xmlns="{MAIN_NS}" id="1" name="SalesTbl" displayName="SalesTbl" ref="A1:C5"><autoFilter ref="A1:C5"/><tableColumns count="3"><tableColumn id="1" name="Region"/><tableColumn id="2" name="Product"/><tableColumn id="3" name="Sales"/></tableColumns><tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/></table>"#
    );
    let types = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/></Types>"#;
    let root = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let book_rels = rels(&[
        ("rId1", "worksheet", "worksheets/sheet1.xml"),
        ("rId2", "worksheet", "worksheets/sheet2.xml"),
        ("rId3", "pivotCacheDefinition", "pivotCache/pivotCacheDefinition1.xml"),
    ]);
    let data_rels = rels(&[("rIdT1", "table", "../tables/table1.xml")]);
    let report_rels = rels(&[("rId1", "pivotTable", "../pivotTables/pivotTable1.xml")]);
    let view_rels = rels(&[("rId1", "pivotCacheDefinition", "../pivotCache/pivotCacheDefinition1.xml")]);
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", book.as_bytes()),
        ("xl/_rels/workbook.xml.rels", book_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", data.as_bytes()),
        ("xl/worksheets/sheet2.xml", report.as_bytes()),
        ("xl/worksheets/_rels/sheet2.xml.rels", report_rels.as_bytes()),
        ("xl/pivotTables/pivotTable1.xml", view.as_bytes()),
        ("xl/pivotTables/_rels/pivotTable1.xml.rels", view_rels.as_bytes()),
        ("xl/pivotCache/pivotCacheDefinition1.xml", cache.as_bytes()),
    ];
    if table {
        parts.push(("xl/worksheets/_rels/sheet1.xml.rels", data_rels.as_bytes()));
        parts.push(("xl/tables/table1.xml", table_xml.as_bytes()));
    }
    zip(&parts)
}
