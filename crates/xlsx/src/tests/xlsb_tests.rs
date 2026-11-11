//! Original BIFF12 records and ZIP packages assembled in Rust from the public format schema.
use gridcraft_core::{CellError, CellRef, DateSystem, RangeRef, Value};
use gridcraft_model::Visibility;

use super::make_zip;
use crate::{Format, read_xlsb, read_xlsx, sniff, write_xlsx};

pub(super) const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="records/book.bin"/></Relationships>"#;
pub(super) const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/one.bin"/><Relationship Id="two" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/two.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../text/strings.bin"/></Relationships>"#;
pub(super) const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/></Types>"#;

pub(super) fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
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
pub(super) fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    for unit in units {
        out.extend(unit.to_le_bytes());
    }
    out
}
pub(super) fn rich(text: &str, extras: bool) -> Vec<u8> {
    let mut out = vec![if extras { 3 } else { 0 }];
    out.extend(wide(text));
    if extras {
        out.extend(1u32.to_le_bytes());
        out.extend([0, 0, 0, 0]); // One original formatting run.
        out.extend(wide("sound"));
        out.extend(1u32.to_le_bytes());
        out.extend([0, 0, 0, 0, 1, 0, 0, 0, 0, 0]); // One ten-byte phonetic run.
    }
    out
}
pub(super) fn row(r: u32) -> Vec<u8> {
    let mut out = r.to_le_bytes().to_vec();
    out.extend([0; 13]);
    record(0, &out)
}
pub(super) fn cell(id: u16, col: u32, value: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(value);
    if (8..=11).contains(&id) {
        data.extend([0; 2]); // Formula flags.
        data.extend(3u32.to_le_bytes());
        data.extend([0x1e, 1, 0]); // Original PtgInt(1); ignored because we import the cache.
        data.extend(0u32.to_le_bytes());
    }
    record(id, &data)
}
pub(super) fn worksheet(data: &[u8], tail: &[u8]) -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(record(148, &[0xff; 16])); // A bogus dimension must never allocate a dense grid.
    out.extend(record(145, &[]));
    out.extend(data);
    out.extend(record(146, &[]));
    out.extend(tail);
    out.extend(record(130, &[]));
    out
}
pub(super) fn book(date1904: bool) -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = u32::from(date1904).to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    for (id, rel, name, state) in [(1u32, "one", "Data", 0u32), (2, "two", "Hidden", 1)] {
        let mut data = state.to_le_bytes().to_vec();
        data.extend(id.to_le_bytes());
        data.extend(wide(rel));
        data.extend(wide(name));
        out.extend(record(156, &data));
    }
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}
pub(super) fn sst() -> Vec<u8> {
    let mut out = record(159, &[1, 0, 0, 0, 1, 0, 0, 0]);
    out.extend(record(19, &rich("shared 🦀 café", true)));
    out.extend(record(160, &[]));
    out
}
pub(super) fn package(book: &[u8], sheet: &[u8], strings: &[u8], roots: &str, rels: &str, types: &str) -> Vec<u8> {
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", roots.as_bytes()),
        ("records/book.bin", book),
        ("records/_rels/book.bin.rels", rels.as_bytes()),
        ("data/one.bin", sheet),
        ("data/two.bin", &worksheet(&[], &[])),
        ("text/strings.bin", strings),
    ])
}
pub(super) fn file(sheet: &[u8]) -> Vec<u8> {
    package(&book(false), sheet, &sst(), ROOT, RELS, TYPES)
}
fn text(s: &str) -> Value {
    Value::Text(s.into())
}

#[test]
fn xlsb_scalar_caches_rk_unicode_visibility_and_epochs_survive_xlsx_save() {
    let mut data = row(2);
    let real_rk = (3.5f64.to_bits() >> 32) as u32;
    let encoded = [
        cell(1, 0, &[]),
        cell(2, 1, &((-123i32 << 2) | 2).to_le_bytes()),
        cell(2, 2, &((123i32 << 2) | 3).to_le_bytes()),
        cell(2, 3, &real_rk.to_le_bytes()),
        cell(2, 4, &(real_rk | 1).to_le_bytes()),
        cell(3, 5, &[0x17]),
        cell(4, 6, &[1]),
        cell(5, 7, &123.5f64.to_le_bytes()),
        cell(6, 8, &wide("inline λ")),
        cell(7, 9, &0u32.to_le_bytes()),
        cell(8, 10, &wide("cached text")),
        cell(9, 11, &2.0f64.to_le_bytes()),
        cell(10, 12, &[0]),
        cell(11, 13, &[7]),
        cell(62, 14, &rich("rich text", true)),
    ];
    for cell in encoded {
        data.extend(cell);
    }
    let mut merge = vec![];
    for n in [5u32, 6, 0, 1] {
        merge.extend(n.to_le_bytes());
    }
    let sheet = worksheet(&data, &record(176, &merge));
    let expected = [
        Value::Empty,
        Value::Number(-123.0),
        Value::Number(1.23),
        Value::Number(3.5),
        Value::Number(0.035),
        Value::Error(CellError::Ref),
        Value::Bool(true),
        Value::Number(123.5),
        text("inline λ"),
        text("shared 🦀 café"),
        text("cached text"),
        Value::Number(2.0),
        Value::Bool(false),
        Value::Error(CellError::Div0),
        text("rich text"),
    ];
    for epoch in [false, true] {
        let bytes = package(&book(epoch), &sheet, &sst(), ROOT, RELS, TYPES);
        assert_eq!(sniff(&bytes), Format::Xlsb);
        let (wb, report) = read_xlsb(&bytes).unwrap();
        assert_eq!(wb.date_system, if epoch { DateSystem::D1904 } else { DateSystem::D1900 });
        assert_eq!(wb.sheets.len(), 2);
        assert_eq!(wb.sheets[1].visibility, Visibility::Hidden);
        assert_eq!(wb.sheets[0].cells.len(), 14);
        assert_eq!(wb.sheets[0].merges, vec![RangeRef::new(CellRef::new(5, 0), CellRef::new(6, 1))]);
        assert!(report.warnings.iter().any(|w| w.contains("not imported")));
        for (col, expected) in expected.iter().enumerate() {
            assert_eq!(&wb.sheets[0].value(CellRef::new(2, col as u32)), expected);
        }
        assert!(wb.sheets[0].cells.iter().all(|(_, c)| c.formula.is_none()));
        let (roundtrip, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
        assert_eq!(roundtrip.date_system, wb.date_system);
        for (col, expected) in expected.iter().enumerate() {
            assert_eq!(&roundtrip.sheets[0].value(CellRef::new(2, col as u32)), expected);
        }
    }
}

#[test]
fn xlsb_extension_blocks_are_skipped_with_strict_boundaries_and_record_framing() {
    let mut data = row(0);
    data.extend(cell(5, 0, &1.0f64.to_le_bytes()));
    data.extend(record(35, &[0; 4]));
    data.extend(record(37, &[1, 0, 0, 0, 0, 0]));
    data.extend(cell(5, 1, &99.0f64.to_le_bytes()));
    data.extend(record(130, &[])); // Looks like EndSheet, but is inside an ignored extension.
    data.extend(record(38, &[]));
    data.extend(record(36, &[]));
    data.extend([0xe8, 0x87, 0x80, 0x80, 0x80, 0x80]); // ID1000: ignored high second-ID/fourth-length bits.
    data.extend(cell(5, 1, &2.0f64.to_le_bytes()));
    let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
    assert_eq!(wb.sheets[0].cells.len(), 2);
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 1)), Value::Number(2.0));
    assert!(report.warnings.iter().any(|w| w.contains("blocks were skipped")));
    for bad in [
        vec![0x81],
        vec![0x81, 0],
        vec![1, 0x80],
        vec![1, 10, 0],
        record(36, &[]),
        [record(35, &[0; 4]), record(38, &[])].concat(),
        record(35, &[0; 4]),
        record(37, &[0, 0]),
    ] {
        assert!(read_xlsb(&file(&worksheet(&bad, &[]))).is_err(), "{bad:?}");
    }
    let nested = record(35, &[0; 4]).repeat(65);
    assert!(read_xlsb(&file(&worksheet(&nested, &[]))).unwrap_err().to_string().contains("nested too deeply"));
    let mut missing_end = worksheet(&[], &[]);
    missing_end.truncate(missing_end.len() - record(130, &[]).len());
    assert!(read_xlsb(&file(&missing_end)).is_err());
}

#[test]
fn xlsb_bad_values_strings_and_coordinates_fail_without_silent_data_loss() {
    let malformed = [
        cell(4, 0, &[2]),
        cell(3, 0, &[0xff]),
        cell(5, 0, &f64::INFINITY.to_le_bytes()),
        cell(2, 0, &0x7ff00000u32.to_le_bytes()),
        cell(7, 0, &1u32.to_le_bytes()),
        cell(6, 0, &[1, 0, 0, 0, 0, 0xd8]),
        cell(6, 0, &u32::MAX.to_le_bytes()),
        cell(5, 16384, &1.0f64.to_le_bytes()),
        cell(5, 0, &[0; 3]),
        cell(62, 0, &[1, 0, 0, 0, 0, 0xff, 0xff, 0xff, 0xff]),
    ];
    for bad in malformed {
        assert!(read_xlsb(&file(&worksheet(&[row(0), bad].concat(), &[]))).is_err());
    }
    assert!(read_xlsb(&file(&worksheet(&cell(5, 0, &1.0f64.to_le_bytes()), &[]))).is_err());
    assert!(read_xlsb(&file(&worksheet(&row(1048576), &[]))).is_err());
    let repeated = [row(0), cell(5, 0, &1.0f64.to_le_bytes()), cell(5, 0, &2.0f64.to_le_bytes())].concat();
    assert!(read_xlsb(&file(&worksheet(&repeated, &[]))).is_err());
    let mut formula_payload = [0u8; 18].to_vec(); // Cell header + f64 cache + flags, but no parsed-formula structure.
    assert!(read_xlsb(&file(&worksheet(&[row(0), record(9, &formula_payload)].concat(), &[]))).is_err());
    formula_payload.extend(0u32.to_le_bytes()); // Invalid zero token count.
    assert!(read_xlsb(&file(&worksheet(&[row(0), record(9, &formula_payload)].concat(), &[]))).is_err());
    let bad_sst = [record(159, &[1, 0, 0, 0, 2, 0, 0, 0]), record(160, &[])].concat();
    assert!(read_xlsb(&package(&book(false), &worksheet(&[], &[]), &bad_sst, ROOT, RELS, TYPES)).is_err());
}

#[test]
fn xlsb_requires_internal_typed_relationships_and_sniffs_only_root_workbook() {
    let sheet = worksheet(&[], &[]);
    for (roots, rels, types) in [
        (ROOT.replace("officeDocument\"", "wrong\""), RELS.to_owned(), TYPES.to_owned()),
        (ROOT.replace("Target=", "TargetMode=\"External\" Target="), RELS.to_owned(), TYPES.to_owned()),
        (
            ROOT.to_owned(),
            RELS.replace("Target=\"../data/one.bin\"", "TargetMode=\"External\" Target=\"https://example.com/data.bin\""),
            TYPES.to_owned(),
        ),
        (
            ROOT.to_owned(),
            RELS.replace("Target=\"../text/strings.bin\"", "TargetMode=\"External\" Target=\"https://example.com/strings.bin\""),
            TYPES.to_owned(),
        ),
        (ROOT.to_owned(), RELS.to_owned(), TYPES.replace("application/vnd.ms-excel.worksheet", "application/xml")),
        (ROOT.to_owned(), RELS.trim_end_matches("</Relationships>").to_owned(), TYPES.to_owned()),
        (ROOT.to_owned(), RELS.to_owned(), TYPES.replace("http://schemas.openxmlformats.org/package/2006/content-types", "wrong")),
    ] {
        assert!(read_xlsb(&package(&book(false), &sheet, &sst(), &roots, &rels, &types)).is_err());
    }
    // A worksheet relationship naming a part that isn't in the package (and has no content type)
    // opens with that sheet empty and a warning, like a truncated file in Excel.
    let missing = RELS.replace("Target=\"../data/one.bin\"", "Target=\"missing.bin\"");
    let (wb, report) = read_xlsb(&package(&book(false), &sheet, &sst(), ROOT, &missing, TYPES)).unwrap();
    assert_eq!(wb.sheets[0].used_range(), None);
    assert!(report.warnings.iter().any(|w| w.contains("missing part") && w.contains(&wb.sheets[0].name)), "{:?}", report.warnings);
    let chartsheet = RELS.replace(
        "Id=\"two\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
        "Id=\"two\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet",
    );
    // A chart sheet whose part isn't a readable chart sheet is skipped; the workbook still loads.
    let (wb, report) = read_xlsb(&package(&book(false), &sheet, &sst(), ROOT, &chartsheet, TYPES)).unwrap();
    assert_eq!(wb.sheets.len(), 1);
    assert!(report.warnings.iter().any(|w| w.contains("chart sheets that could not be read")));
    let dialogsheet = chartsheet.replace("relationships/chartsheet", "relationships/dialogsheet");
    let (wb, report) = read_xlsb(&package(&book(false), &sheet, &sst(), ROOT, &dialogsheet, TYPES)).unwrap();
    assert_eq!(wb.sheets.len(), 1);
    assert!(report.warnings.iter().any(|w| w.contains("non-worksheet")));
    // An unrelated binary part must not change the format of an XML workbook package.
    let xml_book = TYPES.replace(
        "/records/book.bin\" ContentType=\"application/vnd.ms-excel.sheet.binary.macroEnabled.main",
        "/elsewhere.bin\" ContentType=\"application/vnd.ms-excel.sheet.binary.macroEnabled.main",
    );
    assert_eq!(sniff(&package(&book(false), &sheet, &sst(), ROOT, RELS, &xml_book)), Format::Xlsx);
    let spec_type = TYPES.replace("application/vnd.ms-excel.sheet.binary.macroEnabled.main", "application/vnd.ms-excel.main");
    assert_eq!(sniff(&package(&book(false), &sheet, &sst(), ROOT, RELS, &spec_type)), Format::Xlsb);
    assert!(read_xlsb(&package(&book(false), &sheet, &sst(), ROOT, RELS, &spec_type)).is_ok());
}

#[test]
fn xlsb_declared_counts_and_decoded_text_are_bounded() {
    let bad_sst = record(159, &[0xff; 8]);
    assert!(read_xlsb(&package(&book(false), &worksheet(&[], &[]), &bad_sst, ROOT, RELS, TYPES)).is_err());
    let mut count_mismatch = record(159, &[1, 0, 0, 0, 1, 0, 0, 0]);
    count_mismatch.extend(record(160, &[]));
    assert!(read_xlsb(&package(&book(false), &worksheet(&[], &[]), &count_mismatch, ROOT, RELS, TYPES)).is_err());
    let mut data = row(0);
    let text = wide(&"x".repeat(32767));
    for col in 0..2050 {
        data.extend(cell(6, col, &text));
    }
    assert!(read_xlsb(&file(&worksheet(&data, &[]))).unwrap_err().to_string().contains("64 MiB"));
}

#[test]
fn xlsb_phonetic_runs_use_documented_size_and_tolerate_padding() {
    // [MS-XLSB] 2.5.103: a PhRun is 10 bytes. Phonetic data is not imported, so
    // padding after the documented runs (e.g. 12-byte runs) is ignored. Runs
    // shorter than documented are ignored too: Excel opens workbooks whose
    // phonetic tails fall short of `count * 10` bytes.
    let phonetic = |runs: u32, bytes: usize| {
        let mut out = vec![2];
        out.extend(wide("ruby"));
        out.extend(wide("ルビ"));
        out.extend(runs.to_le_bytes());
        out.extend(vec![0; bytes]);
        out
    };
    for padded in [phonetic(1, 10), phonetic(1, 12), phonetic(2, 24)] {
        let (wb, _) = read_xlsb(&file(&worksheet(&[row(0), cell(62, 0, &padded)].concat(), &[]))).unwrap();
        assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), text("ruby"));
    }
    let (wb, _) = read_xlsb(&file(&worksheet(&[row(0), cell(62, 0, &phonetic(2, 12))].concat(), &[]))).unwrap();
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), text("ruby"));
}

// ---- Formulas (MS-XLSB CellParsedFormula): rgce tokens decode to formula text.

/// A formula cell record (8 string, 9 number, 10 bool, 11 error) with its cached value and rgce.
fn fmla(id: u16, col: u32, cached: &[u8], rgce: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend([0; 4]);
    data.extend(cached);
    data.extend([0; 2]); // Formula flags.
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(id, &data)
}
/// PtgRef (reference class) to row/col with relative flags (ColRelShort bits 14/15).
fn ptg_ref(row: u32, col: u16, col_rel: bool, row_rel: bool) -> Vec<u8> {
    let mut out = vec![0x24];
    out.extend(row.to_le_bytes());
    out.extend((col | if col_rel { 0x4000 } else { 0 } | if row_rel { 0x8000 } else { 0 }).to_le_bytes());
    out
}
/// PtgArea (reference class), all corners relative.
pub(super) fn ptg_area(r1: u32, r2: u32, c1: u16, c2: u16) -> Vec<u8> {
    let mut out = vec![0x25];
    out.extend(r1.to_le_bytes());
    out.extend(r2.to_le_bytes());
    out.extend((c1 | 0xC000).to_le_bytes());
    out.extend((c2 | 0xC000).to_le_bytes());
    out
}
pub(super) fn ptg_int(n: u16) -> Vec<u8> {
    let mut out = vec![0x1E];
    out.extend(n.to_le_bytes());
    out
}
fn formula_book(cells: &[Vec<u8>]) -> gridcraft_model::Workbook {
    let mut data = row(0);
    data.extend(cell(5, 0, &1.0f64.to_le_bytes())); // A1 = 1
    data.extend(cell(5, 1, &2.0f64.to_le_bytes())); // B1 = 2
    data.extend(row(1));
    data.extend(cell(5, 0, &3.0f64.to_le_bytes())); // A2 = 3
    data.extend(cell(5, 1, &4.0f64.to_le_bytes())); // B2 = 4
    data.extend(row(2));
    for c in cells {
        data.extend(c);
    }
    read_xlsb(&file(&worksheet(&data, &[]))).unwrap().0
}
pub(super) fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(CellRef::parse(a1).unwrap()).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}
fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

#[test]
fn xlsb_formulas_decode_references_operators_and_functions() {
    let wb = formula_book(&[
        // A3: =A1+1 (cached 2)
        fmla(9, 0, &2.0f64.to_le_bytes(), &cat(&[&ptg_ref(0, 0, true, true), &ptg_int(1), &[0x03]])),
        // B3: =$A$1*2 (cached 2)
        fmla(9, 1, &2.0f64.to_le_bytes(), &cat(&[&ptg_ref(0, 0, false, false), &ptg_int(2), &[0x05]])),
        // C3: =SUM(A1:B2) (cached 10): PtgFuncVar, 1 argument, iftab 4
        fmla(9, 2, &10.0f64.to_le_bytes(), &cat(&[&ptg_area(0, 1, 0, 1), &[0x22, 1, 4, 0]])),
        // D3: =ABS(B$2) (cached 4): PtgFunc iftab 24, column relative, row absolute
        fmla(9, 3, &4.0f64.to_le_bytes(), &cat(&[&ptg_ref(1, 1, true, false), &[0x21, 24, 0]])),
        // E3: =A1>1 (cached FALSE)
        fmla(10, 4, &[0], &cat(&[&ptg_ref(0, 0, true, true), &ptg_int(1), &[0x0D]])),
        // F3: =1/0 (cached #DIV/0!)
        fmla(11, 5, &[0x07], &cat(&[&ptg_int(1), &ptg_int(0), &[0x06]])),
    ]);
    let rows = [
        ("A3", "A1+1", Value::Number(2.0)),
        ("B3", "$A$1*2", Value::Number(2.0)),
        ("C3", "SUM(A1:B2)", Value::Number(10.0)),
        ("D3", "ABS(B$2)", Value::Number(4.0)),
        ("E3", "A1>1", Value::Bool(false)),
        ("F3", "1/0", Value::Error(CellError::Div0)),
    ];
    for (a1, text, cached) in rows {
        assert_eq!(ftext(&wb, a1).as_deref(), Some(text), "{a1} formula");
        assert_eq!(wb.sheets[0].value(CellRef::parse(a1).unwrap()), cached, "{a1} keeps its saved result");
    }
}

#[test]
fn xlsb_formulas_survive_saving_as_xlsx() {
    let wb = formula_book(&[fmla(9, 2, &10.0f64.to_le_bytes(), &cat(&[&ptg_area(0, 1, 0, 1), &[0x22, 1, 4, 0]]))]);
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(ftext(&back, "C3").as_deref(), Some("SUM(A1:B2)"));
    assert_eq!(back.sheets[0].value(CellRef::parse("C3").unwrap()), Value::Number(10.0));
}

#[test]
fn xlsb_unsupported_formula_tokens_keep_the_saved_result_and_warn() {
    let mut data = row(0);
    // A1: an unknown token (0x7F) after a valid one: kept as its cached value, the file still opens.
    data.extend(fmla(9, 0, &5.0f64.to_le_bytes(), &[0x1E, 1, 0, 0x7F]));
    // B1: a decodable formula next to it is still imported.
    data.extend(fmla(9, 1, &6.0f64.to_le_bytes(), &cat(&[&ptg_int(2), &ptg_int(3), &[0x05]])));
    let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::Number(5.0));
    assert_eq!(ftext(&wb, "A1"), None);
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("2*3"));
    assert!(report.warnings.iter().any(|w| w.to_lowercase().contains("formula")), "{:?}", report.warnings);
}

#[test]
fn xlsb_import_warning_no_longer_says_formulas_become_values_when_all_decode() {
    let mut data = row(0);
    data.extend(fmla(9, 0, &3.0f64.to_le_bytes(), &cat(&[&ptg_int(1), &ptg_int(2), &[0x03]])));
    let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("1+2"));
    assert!(!report.warnings.iter().any(|w| w.contains("formulas become their saved results")), "{:?}", report.warnings);
}

// ---- Shared (BrtShrFmla 427) and array (BrtArrFmla 426) formulas, referenced by PtgExp.

/// PtgExp: the cell is part of the shared or array formula anchored in `row`.
pub(super) fn ptg_exp(row: u32) -> Vec<u8> {
    let mut out = vec![0x01];
    out.extend(row.to_le_bytes());
    out
}
/// PtgRefN (reference class): row/column offsets (or absolute coordinates) with relative flags.
pub(super) fn ptg_ref_n(row: i32, col: i16, col_rel: bool, row_rel: bool) -> Vec<u8> {
    let mut out = vec![0x2C];
    out.extend(row.to_le_bytes());
    out.extend(((col as u16 & 0x3FFF) | if col_rel { 0x4000 } else { 0 } | if row_rel { 0x8000 } else { 0 }).to_le_bytes());
    out
}
/// PtgAreaN (reference class), all corners relative offsets.
fn ptg_area_n(r1: i32, r2: i32, c1: i16, c2: i16) -> Vec<u8> {
    let mut out = vec![0x2D];
    out.extend(r1.to_le_bytes());
    out.extend(r2.to_le_bytes());
    out.extend(((c1 as u16 & 0x3FFF) | 0xC000).to_le_bytes());
    out.extend(((c2 as u16 & 0x3FFF) | 0xC000).to_le_bytes());
    out
}
pub(super) fn rfx(r0: u32, r1: u32, c0: u32, c1: u32) -> Vec<u8> {
    [r0, r1, c0, c1].iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn shr_fmla(range: &[u8], rgce: &[u8]) -> Vec<u8> {
    let mut data = range.to_vec();
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(427, &data)
}
fn arr_fmla(range: &[u8], rgce: &[u8]) -> Vec<u8> {
    let mut data = range.to_vec();
    data.push(0); // fAlwaysCalc
    data.extend((rgce.len() as u32).to_le_bytes());
    data.extend(rgce);
    data.extend(0u32.to_le_bytes());
    record(426, &data)
}
fn num(n: f64) -> Vec<u8> {
    n.to_le_bytes().to_vec()
}
fn rows_book(rows: &[Vec<u8>]) -> (gridcraft_model::Workbook, crate::ReadReport) {
    let mut data = vec![];
    for (r, cells) in rows.iter().enumerate() {
        data.extend(row(r as u32));
        data.extend(cells);
    }
    read_xlsb(&file(&worksheet(&data, &[]))).unwrap()
}
fn val(wb: &gridcraft_model::Workbook, a1: &str) -> Value {
    wb.sheets[0].value(CellRef::parse(a1).unwrap())
}

#[test]
fn xlsb_shared_formula_filled_down_shifts_relative_references() {
    // A1:A3 = 1,2,3; B1:B3 = A1*2 filled down: B1 holds the PtgExp, then BrtShrFmla B1:B3.
    let times2 = cat(&[&ptg_ref_n(0, -1, true, true), &ptg_int(2), &[0x05]]);
    let (wb, report) = rows_book(&[
        cat(&[&cell(5, 0, &num(1.0)), &fmla(9, 1, &num(2.0), &ptg_exp(0)), &shr_fmla(&rfx(0, 2, 1, 1), &times2)]),
        cat(&[&cell(5, 0, &num(2.0)), &fmla(9, 1, &num(4.0), &ptg_exp(0))]),
        cat(&[&cell(5, 0, &num(3.0)), &fmla(9, 1, &num(6.0), &ptg_exp(0))]),
    ]);
    for (a1, text, cached) in [("B1", "A1*2", 2.0), ("B2", "A2*2", 4.0), ("B3", "A3*2", 6.0)] {
        assert_eq!(ftext(&wb, a1).as_deref(), Some(text), "{a1}");
        assert_eq!(val(&wb, a1), Value::Number(cached), "{a1} keeps its saved result");
    }
    assert!(!report.warnings.iter().any(|w| w.contains("formula")), "{:?}", report.warnings);
}

#[test]
fn xlsb_shared_formula_filled_right_keeps_absolute_parts() {
    // C1:E1 = C$5+$A1: the row of the first ref and the column of the second are absolute.
    let rgce = cat(&[&ptg_ref_n(4, 0, true, false), &ptg_ref_n(0, 0, false, true), &[0x03]]);
    let (wb, _) = rows_book(&[cat(&[
        &fmla(9, 2, &num(0.0), &ptg_exp(0)),
        &shr_fmla(&rfx(0, 0, 2, 4), &rgce),
        &fmla(9, 3, &num(0.0), &ptg_exp(0)),
        &fmla(9, 4, &num(0.0), &ptg_exp(0)),
    ])]);
    assert_eq!(ftext(&wb, "C1").as_deref(), Some("C$5+$A1"));
    assert_eq!(ftext(&wb, "D1").as_deref(), Some("D$5+$A1"));
    assert_eq!(ftext(&wb, "E1").as_deref(), Some("E$5+$A1"));
}

#[test]
fn xlsb_shared_formula_area_shifts() {
    // B1:B2 = SUM(A1:A2) filled down.
    let rgce = cat(&[&ptg_area_n(0, 1, -1, -1), &[0x19, 0x10, 0, 0]]);
    let (wb, _) = rows_book(&[cat(&[&fmla(9, 1, &num(0.0), &ptg_exp(0)), &shr_fmla(&rfx(0, 1, 1, 1), &rgce)]), fmla(9, 1, &num(0.0), &ptg_exp(0))]);
    assert_eq!(ftext(&wb, "B1").as_deref(), Some("SUM(A1:A2)"));
    assert_eq!(ftext(&wb, "B2").as_deref(), Some("SUM(A2:A3)"));
}

#[test]
fn xlsb_array_formula_anchors_its_range_and_survives_xlsx_save() {
    // {=A1:A3*2} over D1:D3: D1 holds the PtgExp, then BrtArrFmla; D2/D3 refer to it too.
    let rgce = cat(&[&ptg_area(0, 2, 0, 0), &ptg_int(2), &[0x05]]);
    let (wb, report) = rows_book(&[
        cat(&[&cell(5, 0, &num(1.0)), &fmla(9, 3, &num(2.0), &ptg_exp(0)), &arr_fmla(&rfx(0, 2, 3, 3), &rgce)]),
        cat(&[&cell(5, 0, &num(2.0)), &fmla(9, 3, &num(4.0), &ptg_exp(0))]),
        cat(&[&cell(5, 0, &num(3.0)), &fmla(9, 3, &num(6.0), &ptg_exp(0))]),
    ]);
    let f = wb.sheets[0].cell(CellRef::parse("D1").unwrap()).and_then(|c| c.formula.clone()).unwrap();
    assert_eq!((f.text.as_str(), f.array), ("A1:A3*2", Some(RangeRef::parse("D1:D3").unwrap())));
    assert_eq!(val(&wb, "D1"), Value::Number(2.0));
    // The other cells are the array's results: as for .xls and .xlsx, they hold no formula of
    // their own and the array fills them when it calculates.
    assert_eq!(ftext(&wb, "D2"), None);
    assert_eq!(ftext(&wb, "D3"), None);
    assert!(!report.warnings.iter().any(|w| w.contains("formula")), "{:?}", report.warnings);
    let saved = write_xlsx(&wb).unwrap();
    let xml = String::from_utf8(crate::package::Package::open(&saved).unwrap().read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap();
    assert!(xml.contains(r#"<f t="array" ref="D1:D3">"#), "{xml}");
    let (back, _) = read_xlsx(&saved).unwrap();
    let f = back.sheets[0].cell(CellRef::parse("D1").unwrap()).and_then(|c| c.formula.clone()).unwrap();
    assert_eq!((f.text.as_str(), f.array), ("A1:A3*2", Some(RangeRef::parse("D1:D3").unwrap())));
}

#[test]
fn xlsb_ptg_exp_without_its_formula_keeps_the_saved_value() {
    let (wb, report) = rows_book(&[cat(&[&fmla(9, 1, &num(7.0), &ptg_exp(0)), &fmla(9, 2, &num(3.0), &cat(&[&ptg_int(1), &ptg_int(2), &[0x03]]))])]);
    assert_eq!(val(&wb, "B1"), Value::Number(7.0));
    assert_eq!(ftext(&wb, "B1"), None);
    assert_eq!(ftext(&wb, "C1").as_deref(), Some("1+2"));
    let kept: Vec<_> = report.warnings.iter().filter(|w| w.contains("kept as their saved results")).collect();
    assert_eq!(kept.len(), 1, "{:?}", report.warnings);
    assert!(kept[0].starts_with("1 XLSB formula(s)"), "{kept:?}");
    assert!(!kept[0].contains("shared or array"), "{kept:?}");
}

#[test]
fn xlsb_malformed_or_huge_shared_ranges_are_handled_without_blowing_up() {
    let times2 = cat(&[&ptg_ref_n(0, -1, true, true), &ptg_int(2), &[0x05]]);
    // B1: a shared range with rwLast < rwFirst; C1: one past the grid; both refused.
    // D1: a valid shared formula over the whole sheet from D1 expands only for the cells that
    // exist (D1, D2), and E2 next to it imports normally.
    let (wb, report) = rows_book(&[
        cat(&[
            &fmla(9, 1, &num(5.0), &ptg_exp(0)),
            &shr_fmla(&rfx(3, 0, 1, 1), &times2),
            &fmla(9, 2, &num(6.0), &ptg_exp(0)),
            &shr_fmla(&rfx(0, 1 << 20, 2, 2), &times2),
            &fmla(9, 3, &num(0.0), &ptg_exp(0)),
            &shr_fmla(&rfx(0, (1 << 20) - 1, 3, 0x3FFF), &times2),
        ]),
        cat(&[&fmla(9, 3, &num(0.0), &ptg_exp(0)), &fmla(9, 4, &num(3.0), &cat(&[&ptg_int(1), &ptg_int(2), &[0x03]]))]),
    ]);
    assert_eq!((val(&wb, "B1"), ftext(&wb, "B1")), (Value::Number(5.0), None));
    assert_eq!((val(&wb, "C1"), ftext(&wb, "C1")), (Value::Number(6.0), None));
    assert_eq!(ftext(&wb, "D1").as_deref(), Some("C1*2"));
    assert_eq!(ftext(&wb, "D2").as_deref(), Some("C2*2"));
    assert_eq!(ftext(&wb, "E2").as_deref(), Some("1+2"));
    assert_eq!(wb.sheets[0].cells.iter().count(), 5);
    assert!(report.warnings.iter().any(|w| w.contains("Invalid XLSB shared or array formula ranges")), "{:?}", report.warnings);
    assert!(report.warnings.iter().any(|w| w.starts_with("2 XLSB formula(s)")), "{:?}", report.warnings);
    // A truncated BrtShrFmla is a damaged file, refused as such (no panic).
    let short = record(427, &rfx(0, 0, 0, 0)[..10]);
    let mut data = row(0);
    data.extend(fmla(9, 1, &num(5.0), &ptg_exp(0)));
    data.extend(short);
    assert!(read_xlsb(&file(&worksheet(&data, &[]))).is_err());
}

// Number formats from the styles part ([MS-XLSB] 2.1.7.50): BrtFmt (44) in BrtBeginFmts (615)..BrtEndFmts (616),
// BrtXF (47) in BrtBeginCellXFs (617)..BrtEndCellXFs (618); cell style XFs (626..627) are not cell XFs.
const STYLES_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/one.bin"/><Relationship Id="two" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/two.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../text/strings.bin"/><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="../text/styles.bin"/></Relationships>"#;
const STYLES_TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/><Override PartName="/text/styles.bin" ContentType="application/vnd.ms-excel.styles"/></Types>"#;

fn xf(fmt: u16) -> Vec<u8> {
    // ixfeParent, iFmt, then font/fill/border/rotation/indent/alignment/flags (16 bytes in all).
    let mut data = 0u16.to_le_bytes().to_vec();
    data.extend(fmt.to_le_bytes());
    data.extend([0; 8]);
    data.extend(0x1010u16.to_le_bytes()); // alcv = 2 (bottom) and fLocked, as Excel writes them
    data.extend([0; 2]);
    record(47, &data)
}
/// Cell XFs: 0 General, 1 built-in 14, 2 custom 164 `0.00%`. A cell style XF with iFmt 10 comes first.
fn styles_part() -> Vec<u8> {
    let mut out = record(278, &[]);
    out.extend(record(615, &1u32.to_le_bytes()));
    let mut fmt = 164u16.to_le_bytes().to_vec();
    fmt.extend(wide("0.00%"));
    out.extend(record(44, &fmt));
    out.extend(record(616, &[]));
    out.extend(record(626, &1u32.to_le_bytes()));
    out.extend(xf(10));
    out.extend(record(627, &[]));
    out.extend(record(617, &3u32.to_le_bytes()));
    for f in [0, 14, 164] {
        out.extend(xf(f));
    }
    out.extend(record(618, &[]));
    out.extend(record(279, &[]));
    out
}
/// A cell record with `iStyleRef` = `xf` (low 24 bits; fPhShow set to check it is masked off).
fn styled(id: u16, col: u32, xf: u32, value: &[u8]) -> Vec<u8> {
    let mut data = col.to_le_bytes().to_vec();
    data.extend((xf | 1 << 24).to_le_bytes());
    data.extend(value);
    if (8..=11).contains(&id) {
        data.extend([0; 2]);
        data.extend(3u32.to_le_bytes());
        data.extend([0x1e, 1, 0]); // PtgInt(1)
        data.extend(0u32.to_le_bytes());
    }
    record(id, &data)
}
fn styled_sheet() -> Vec<u8> {
    let mut data = row(0);
    data.extend(styled(5, 0, 1, &45292.0f64.to_le_bytes())); // A1 date
    data.extend(styled(5, 1, 2, &0.25f64.to_le_bytes())); // B1 percent
    data.extend(styled(5, 2, 0, &3.5f64.to_le_bytes())); // C1 General
    data.extend(styled(1, 3, 2, &[])); // D1 blank, formatted
    data.extend(styled(5, 4, 99, &7.0f64.to_le_bytes())); // E1 XF past the list
    data.extend(styled(9, 5, 2, &1.0f64.to_le_bytes())); // F1 =1 cached 1 (BrtFmlaNum)
    data.extend(styled(1, 6, 0, &[])); // G1 blank, General: not stored
    worksheet(&data, &[])
}
fn styled_file(styles: Option<&[u8]>) -> Vec<u8> {
    let book = book(false);
    let sheet = styled_sheet();
    let strings = sst();
    let two = worksheet(&[], &[]);
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", STYLES_TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", if styles.is_some() { STYLES_RELS.as_bytes() } else { RELS.as_bytes() }),
        ("data/one.bin", &sheet),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
    ];
    if let Some(s) = styles {
        parts.push(("text/styles.bin", s));
    }
    make_zip(&parts)
}
fn fmt_at(wb: &gridcraft_model::Workbook, a1: &str) -> String {
    wb.styles.get(wb.sheets[0].style_id(CellRef::parse(a1).unwrap())).num_fmt.as_str().to_string()
}

#[test]
fn xlsb_styles_number_formats_apply_to_cells() {
    let (wb, _) = read_xlsb(&styled_file(Some(&styles_part()))).unwrap();
    let sh = &wb.sheets[0];
    assert_eq!(sh.value(CellRef::parse("A1").unwrap()), Value::Number(45292.0));
    assert_eq!(fmt_at(&wb, "A1"), "m/d/yyyy");
    assert_eq!(fmt_at(&wb, "B1"), "0.00%");
    assert_eq!(sh.style_id(CellRef::parse("C1").unwrap()), gridcraft_model::StyleId::DEFAULT);
    let d1 = sh.cells.get(CellRef::parse("D1").unwrap()).expect("formatted blank cell is stored");
    assert_eq!(d1.value, Value::Empty);
    assert_eq!(fmt_at(&wb, "D1"), "0.00%");
    assert_eq!(sh.value(CellRef::parse("E1").unwrap()), Value::Number(7.0));
    assert_eq!(sh.style_id(CellRef::parse("E1").unwrap()), gridcraft_model::StyleId::DEFAULT);
    let f1 = sh.cells.get(CellRef::parse("F1").unwrap()).unwrap();
    assert!(f1.formula.is_none(), "a lone constant stays a value (P05-001 rule)");
    assert_eq!(fmt_at(&wb, "F1"), "0.00%");
    assert!(sh.cells.get(CellRef::parse("G1").unwrap()).is_none());
}

#[test]
fn xlsb_styles_formula_cells_keep_formula_and_format() {
    let mut data = row(0);
    data.extend(styled(5, 0, 0, &2.0f64.to_le_bytes())); // A1 = 2
    let mut rgce = vec![0x44]; // PtgRef (value class) A1, absolute
    rgce.extend(0u32.to_le_bytes());
    rgce.extend(0u16.to_le_bytes());
    let mut cellrec = 1u32.to_le_bytes().to_vec(); // column B
    cellrec.extend(2u32.to_le_bytes()); // XF 2
    cellrec.extend(2.0f64.to_le_bytes());
    cellrec.extend([0; 2]);
    cellrec.extend((rgce.len() as u32).to_le_bytes());
    cellrec.extend(&rgce);
    cellrec.extend(0u32.to_le_bytes());
    data.extend(record(9, &cellrec));
    let sheet = worksheet(&data, &[]);
    let (book_, strings, two, styles) = (book(false), sst(), worksheet(&[], &[]), styles_part());
    let bytes = make_zip(&[
        ("[Content_Types].xml", STYLES_TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book_),
        ("records/_rels/book.bin.rels", STYLES_RELS.as_bytes()),
        ("data/one.bin", &sheet),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
        ("text/styles.bin", &styles),
    ]);
    let (wb, _) = read_xlsb(&bytes).unwrap();
    let b1 = wb.sheets[0].cells.get(CellRef::parse("B1").unwrap()).unwrap();
    assert!(b1.formula.is_some());
    assert_eq!(fmt_at(&wb, "B1"), "0.00%");
}

#[test]
fn xlsb_styles_survive_saving_as_xlsx() {
    let (wb, _) = read_xlsb(&styled_file(Some(&styles_part()))).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(fmt_at(&back, "A1"), "m/d/yyyy");
    assert_eq!(fmt_at(&back, "B1"), "0.00%");
    assert_eq!(fmt_at(&back, "D1"), "0.00%");
}

#[test]
fn xlsb_styles_part_is_optional() {
    let (wb, report) = read_xlsb(&styled_file(None)).unwrap();
    assert_eq!(wb.sheets[0].value(CellRef::parse("B1").unwrap()), Value::Number(0.25));
    assert_eq!(wb.sheets[0].style_id(CellRef::parse("B1").unwrap()), gridcraft_model::StyleId::DEFAULT);
    assert!(!report.warnings.iter().any(|w| w.to_lowercase().contains("styles")));
}

#[test]
fn xlsb_styles_damaged_part_keeps_values_and_warns() {
    let mut styles = styles_part();
    styles.truncate(styles.len() - 12); // Cuts the last BrtXF mid-record.
    let (wb, report) = read_xlsb(&styled_file(Some(&styles))).unwrap();
    assert_eq!(wb.sheets[0].value(CellRef::parse("A1").unwrap()), Value::Number(45292.0));
    assert_eq!(wb.sheets[0].value(CellRef::parse("B1").unwrap()), Value::Number(0.25));
    assert_eq!(wb.sheets[0].style_id(CellRef::parse("A1").unwrap()), gridcraft_model::StyleId::DEFAULT);
    assert!(report.warnings.iter().any(|w| w.to_lowercase().contains("styles")));
}

// ---- Text constants (BIFF12 PtgStr: u16 cch + UTF-16LE characters).

fn ptg_str(s: &str) -> Vec<u8> {
    let units: Vec<u16> = s.encode_utf16().collect();
    let mut out = vec![0x17];
    out.extend((units.len() as u16).to_le_bytes());
    for u in units {
        out.extend(u.to_le_bytes());
    }
    out
}
/// PtgAttrIf / PtgAttrSkip; the jump offsets are not needed to decode the formula text.
fn attr(kind: u8) -> Vec<u8> {
    vec![0x19, kind, 0, 0]
}
fn text_constant_book() -> gridcraft_model::Workbook {
    formula_book(&[
        // A3: ="x"&A1 (cached "x1")
        fmla(8, 0, &wide("x1"), &cat(&[&ptg_str("x"), &ptg_ref(0, 0, true, true), &[0x08]])),
        // B3: =IF(A1>0,"pos","") (cached "pos"): PtgFuncVar, 3 arguments, iftab 1
        fmla(
            8,
            1,
            &wide("pos"),
            &cat(&[
                &ptg_ref(0, 0, true, true),
                &ptg_int(0),
                &[0x0D],
                &attr(0x02),
                &ptg_str("pos"),
                &attr(0x08),
                &ptg_str(""),
                &attr(0x08),
                &[0x42, 3, 1, 0],
            ]),
        ),
        // C3: =LEN("Ünïcødé €") (cached 9): PtgFunc iftab 32
        fmla(9, 2, &9.0f64.to_le_bytes(), &cat(&[&ptg_str("Ünïcødé €"), &[0x41, 32, 0]])),
        // D3: =A1&"say ""hi""" (cached 1say "hi")
        fmla(8, 3, &wide("1say \"hi\""), &cat(&[&ptg_ref(0, 0, true, true), &ptg_str("say \"hi\""), &[0x08]])),
    ])
}

#[test]
fn xlsb_text_constants_decode_to_formula_text() {
    let wb = text_constant_book();
    let rows = [
        ("A3", "\"x\"&A1", text("x1")),
        ("B3", "IF(A1>0,\"pos\",\"\")", text("pos")),
        ("C3", "LEN(\"Ünïcødé €\")", Value::Number(9.0)),
        ("D3", "A1&\"say \"\"hi\"\"\"", text("1say \"hi\"")),
    ];
    for (a1, formula, cached) in rows {
        assert_eq!(ftext(&wb, a1).as_deref(), Some(formula), "{a1} formula");
        assert_eq!(wb.sheets[0].value(CellRef::parse(a1).unwrap()), cached, "{a1} keeps its saved result");
    }
}

#[test]
fn xlsb_text_constants_survive_saving_as_xlsx() {
    let (back, _) = read_xlsx(&write_xlsx(&text_constant_book()).unwrap()).unwrap();
    assert_eq!(ftext(&back, "B3").as_deref(), Some("IF(A1>0,\"pos\",\"\")"));
    assert_eq!(ftext(&back, "C3").as_deref(), Some("LEN(\"Ünïcødé €\")"));
}

#[test]
fn xlsb_text_constants_do_not_trigger_the_kept_as_saved_results_warning() {
    let mut data = row(0);
    data.extend(fmla(8, 0, &wide("ab"), &cat(&[&ptg_str("a"), &ptg_str("b"), &[0x08]])));
    let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("\"a\"&\"b\""));
    assert!(!report.warnings.iter().any(|w| w.contains("kept as their saved results")), "{:?}", report.warnings);
    assert!(!report.warnings.iter().any(|w| w.contains("text or array constants")), "{:?}", report.warnings);
}

#[test]
fn xlsb_truncated_text_constant_keeps_the_saved_result() {
    let mut data = row(0);
    // PtgStr claims 5 characters but carries 1: the cell keeps its cached value and the file opens.
    data.extend(fmla(8, 0, &wide("a"), &[0x17, 5, 0, b'a', 0]));
    let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
    assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), text("a"));
    assert_eq!(ftext(&wb, "A1"), None);
    assert!(report.warnings.iter().any(|w| w.to_lowercase().contains("formula")), "{:?}", report.warnings);
}

// Fonts, fills, borders, alignment and protection from the styles part ([MS-XLSB] 2.4.862 BrtXF,
// 2.4.690 BrtFont, 2.4.681 BrtFill, 2.4.314 BrtBorder, 2.5.5 Blxf, 2.4.337 BrtColor). Record types:
// BrtFont 43 in 611..612, BrtFill 45 in 603..604, BrtBorder 46 in 613..614.
const AUTO: [u8; 8] = [0; 8];
fn indexed(i: u8) -> [u8; 8] {
    [1 << 1, i, 0, 0, 0, 0, 0, 0]
}
fn rgb(r: u8, g: u8, b: u8) -> [u8; 8] {
    [1 | 2 << 1, 0, 0, 0, r, g, b, 255]
}
fn theme(i: u8, tint: i16) -> [u8; 8] {
    let t = tint.to_le_bytes();
    [3 << 1, i, t[0], t[1], 0, 0, 0, 0]
}
#[allow(clippy::too_many_arguments)]
pub(super) fn font(twips: u16, grbit: u16, bls: u16, sss: u16, uls: u8, family: u8, charset: u8, color: [u8; 8], scheme: u8, name: &str) -> Vec<u8> {
    let mut d = twips.to_le_bytes().to_vec();
    d.extend(grbit.to_le_bytes());
    d.extend(bls.to_le_bytes());
    d.extend(sss.to_le_bytes());
    d.extend([uls, family, charset, 0]);
    d.extend(color);
    d.push(scheme);
    d.extend(wide(name));
    record(43, &d)
}
fn fill(fls: u32, fore: [u8; 8], back: [u8; 8]) -> Vec<u8> {
    let mut d = fls.to_le_bytes().to_vec();
    d.extend(fore);
    d.extend(back);
    d.extend([0; 4 + 5 * 8 + 4]); // no gradient
    record(45, &d)
}
fn blxf(dg: u8, color: [u8; 8]) -> Vec<u8> {
    let mut d = vec![dg, 0];
    d.extend(color);
    d
}
fn border(diag: u8, top: Vec<u8>, bottom: Vec<u8>, left: Vec<u8>, right: Vec<u8>, dg: Vec<u8>) -> Vec<u8> {
    let mut d = vec![diag];
    for b in [top, bottom, left, right, dg] {
        d.extend(b);
    }
    record(46, &d)
}
/// A cell XF; `flags` is the alc/alcv/wrap/shrink/locked/hidden word.
pub(super) fn xf_full(font: u16, fill: u16, border: u16, trot: u8, indent: u8, flags: u16) -> Vec<u8> {
    let mut d = 0u16.to_le_bytes().to_vec();
    for v in [0, font, fill, border] {
        d.extend(v.to_le_bytes());
    }
    d.extend([trot, indent]);
    d.extend(flags.to_le_bytes());
    d.extend(0u16.to_le_bytes());
    record(47, &d)
}
const LOCKED: u16 = 1 << 12;
const BOTTOM: u16 = 2 << 3;
fn formats_part() -> Vec<u8> {
    let mut out = record(278, &[]);
    out.extend(record(611, &3u32.to_le_bytes()));
    out.extend(font(220, 0, 400, 0, 0, 2, 0, theme(1, 0), 2, "Calibri")); // Excel's default font
    out.extend(font(280, 1 << 1 | 1 << 3, 700, 0, 1, 2, 0, rgb(0xFF, 0, 0), 0, "Arial"));
    out.extend(font(220, 0, 400, 1, 0x22, 1, 0xCC, theme(4, 13107), 2, "Calibri"));
    out.extend(record(612, &[]));
    out.extend(record(603, &4u32.to_le_bytes()));
    out.extend(fill(0, indexed(64), indexed(65)));
    out.extend(fill(17, indexed(64), indexed(65)));
    out.extend(fill(1, rgb(0xFF, 0xFF, 0), indexed(64)));
    out.extend(fill(9, theme(5, -8192), rgb(0, 0xFF, 0)));
    out.extend(record(604, &[]));
    out.extend(record(613, &2u32.to_le_bytes()));
    out.extend(border(0, blxf(0, AUTO), blxf(0, AUTO), blxf(0, AUTO), blxf(0, AUTO), blxf(0, AUTO)));
    out.extend(border(3, blxf(2, rgb(0, 0, 0xFF)), blxf(6, indexed(10)), blxf(3, theme(1, 0)), blxf(0, AUTO), blxf(1, rgb(0, 0xFF, 0))));
    out.extend(record(614, &[]));
    out.extend(record(617, &7u32.to_le_bytes()));
    out.extend(xf_full(0, 0, 0, 0, 0, BOTTOM | LOCKED)); // 0: default
    out.extend(xf_full(1, 2, 1, 0, 0, 2 | 1 << 6 | LOCKED)); // 1: center, top, wrap
    out.extend(xf_full(2, 3, 0, 45, 2, 3 | 1 << 3 | 1 << 8 | LOCKED)); // 2: right, center, shrink
    out.extend(xf_full(0, 1, 0, 135, 0, 7 | 4 << 3 | LOCKED)); // 3: distributed both ways
    out.extend(xf_full(0, 0, 0, 255, 0, 6 | BOTTOM | LOCKED)); // 4: vertical text, center across
    out.extend(xf_full(0, 0, 0, 0, 0, BOTTOM | 1 << 13)); // 5: unlocked, hidden
    out.extend(xf_full(99, 99, 99, 0, 0, 1 | BOTTOM | LOCKED)); // 6: indices past the lists, left
    out.extend(record(618, &[]));
    out.extend(record(279, &[]));
    out
}
fn formats_file(styles: &[u8]) -> Vec<u8> {
    let mut data = row(0);
    for xf in 0..7 {
        data.extend(styled(5, xf, xf, &f64::from(xf).to_le_bytes()));
    }
    let (book, sheet, strings, two) = (book(false), worksheet(&data, &[]), sst(), worksheet(&[], &[]));
    make_zip(&[
        ("[Content_Types].xml", STYLES_TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("records/book.bin", &book),
        ("records/_rels/book.bin.rels", STYLES_RELS.as_bytes()),
        ("data/one.bin", &sheet),
        ("data/two.bin", &two),
        ("text/strings.bin", &strings),
        ("text/styles.bin", styles),
    ])
}
fn style_at<'a>(wb: &'a gridcraft_model::Workbook, a1: &str) -> &'a gridcraft_model::Style {
    wb.styles.get(wb.sheets[0].style_id(CellRef::parse(a1).unwrap()))
}

#[test]
fn xlsb_formats_fonts_are_imported() {
    use gridcraft_model::{Color, Underline, VertAlign};
    let (wb, report) = read_xlsb(&formats_file(&formats_part())).unwrap();
    assert!(!report.warnings.iter().any(|w| w.to_lowercase().contains("styles")), "{:?}", report.warnings);
    // Excel's default font and alignment are the default style.
    assert_eq!(wb.sheets[0].style_id(CellRef::parse("A1").unwrap()), gridcraft_model::StyleId::DEFAULT);
    let b = &style_at(&wb, "B1").font;
    assert_eq!((b.name.as_str(), b.size, b.bold, b.italic, b.strike), ("Arial", 14.0, true, true, true));
    assert_eq!((b.underline, b.color, b.vert, b.minor), (Underline::Single, Color::Rgb(0xFF0000), VertAlign::Baseline, false));
    let c = &style_at(&wb, "C1").font;
    assert_eq!((c.name.as_str(), c.size, c.bold, c.italic), ("Calibri", 11.0, false, false));
    assert_eq!((c.underline, c.color, c.vert, c.minor), (Underline::DoubleAccounting, Color::Theme(4, 400), VertAlign::Superscript, true));
    assert_eq!((c.family, c.charset), (Some(1), Some(0xCC)));
}

#[test]
fn xlsb_formats_fills_are_imported() {
    use gridcraft_model::{Color, Fill, PatternType};
    let (wb, _) = read_xlsb(&formats_file(&formats_part())).unwrap();
    assert_eq!(style_at(&wb, "A1").fill, Fill::default());
    assert_eq!(style_at(&wb, "B1").fill, Fill::solid(Color::Rgb(0xFFFF00)));
    assert_eq!(style_at(&wb, "C1").fill, Fill { pattern: PatternType::DarkGrid, fg: Color::Theme(5, -250), bg: Color::Rgb(0x00FF00) });
    assert_eq!(style_at(&wb, "D1").fill, Fill { pattern: PatternType::Gray125, fg: Color::Auto, bg: Color::Auto });
}

#[test]
fn xlsb_formats_borders_are_imported() {
    use gridcraft_model::{BorderLine, BorderStyle, Borders, Color};
    let (wb, _) = read_xlsb(&formats_file(&formats_part())).unwrap();
    let line = |style, color| BorderLine { style, color };
    let green = line(BorderStyle::Thin, Color::Rgb(0x00FF00));
    assert_eq!(
        style_at(&wb, "B1").borders,
        Borders {
            top: line(BorderStyle::Medium, Color::Rgb(0x0000FF)),
            bottom: line(BorderStyle::Double, Color::Rgb(0xFF0000)),
            left: line(BorderStyle::Dashed, Color::Theme(1, 0)),
            right: BorderLine::default(),
            diag_down: green,
            diag_up: green,
            ..Borders::default()
        }
    );
    assert_eq!(style_at(&wb, "C1").borders, Borders::default());
}

#[test]
fn xlsb_formats_alignment_and_protection_are_imported() {
    use gridcraft_model::style::Protection;
    use gridcraft_model::{Alignment, HAlign, Style, VAlign};
    let (wb, _) = read_xlsb(&formats_file(&formats_part())).unwrap();
    let al = |h, v, wrap, shrink, indent, rotation| Alignment { h, v, wrap, shrink, indent, rotation, ..Alignment::default() };
    assert_eq!(style_at(&wb, "B1").align, al(HAlign::Center, VAlign::Top, true, false, 0, 0));
    assert_eq!(style_at(&wb, "C1").align, al(HAlign::Right, VAlign::Center, false, true, 2, 45));
    assert_eq!(style_at(&wb, "D1").align, al(HAlign::Distributed, VAlign::Distributed, false, false, 0, -45));
    assert_eq!(style_at(&wb, "E1").align, al(HAlign::CenterAcross, VAlign::Bottom, false, false, 0, 255));
    assert_eq!(style_at(&wb, "F1").protection, Protection { locked: false, hidden: true });
    assert_eq!(style_at(&wb, "B1").protection, Protection::default());
    // Font, fill and border indices past their lists fall back to the defaults; the alignment stays.
    let mut left = Style::default();
    left.align.h = HAlign::Left;
    assert_eq!(style_at(&wb, "G1"), &left);
}

#[test]
fn xlsb_formats_survive_saving_as_xlsx() {
    use gridcraft_model::{Color, Fill, HAlign};
    let (wb, _) = read_xlsb(&formats_file(&formats_part())).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let b = style_at(&back, "B1");
    assert!(b.font.bold && b.font.italic);
    assert_eq!(b.font.name, "Arial");
    assert_eq!(b.fill, Fill::solid(Color::Rgb(0xFFFF00)));
    assert_eq!(b.align.h, HAlign::Center);
    assert_eq!(back.sheets[0].value(CellRef::parse("B1").unwrap()), Value::Number(1.0));
}

#[test]
fn xlsb_formats_damaged_font_record_keeps_values_and_warns() {
    let mut part = formats_part();
    // Cut the second BrtFont (Arial) to 6 bytes: shorter than its fixed fields.
    let arial = font(280, 1 << 1 | 1 << 3, 700, 0, 1, 2, 0, rgb(0xFF, 0, 0), 0, "Arial");
    let at = part.windows(arial.len()).position(|w| w == arial.as_slice()).unwrap();
    part.splice(at..at + arial.len(), record(43, &arial[2..8]));
    let (wb, report) = read_xlsb(&formats_file(&part)).unwrap();
    assert!(report.warnings.iter().any(|w| w.to_lowercase().contains("styles")), "{:?}", report.warnings);
    assert_eq!(wb.sheets[0].value(CellRef::parse("C1").unwrap()), Value::Number(2.0));
    assert_eq!(wb.sheets[0].style_id(CellRef::parse("B1").unwrap()), gridcraft_model::StyleId::DEFAULT);
}
