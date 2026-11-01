//! Original BIFF12 records and ZIP packages assembled in Rust from the public format schema.
use gridcraft_core::{CellError, CellRef, DateSystem, RangeRef, Value};
use gridcraft_model::Visibility;

use super::make_zip;
use crate::{Format, read_xlsb, read_xlsx, sniff, write_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="records/book.bin"/></Relationships>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/one.bin"/><Relationship Id="two" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/two.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../text/strings.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/></Types>"#;

fn record(id: u16, bytes: &[u8]) -> Vec<u8> {
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
fn rich(text: &str, extras: bool) -> Vec<u8> {
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
fn row(r: u32) -> Vec<u8> {
    let mut out = r.to_le_bytes().to_vec();
    out.extend([0; 13]);
    record(0, &out)
}
fn cell(id: u16, col: u32, value: &[u8]) -> Vec<u8> {
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
fn worksheet(data: &[u8], tail: &[u8]) -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(record(148, &[0xff; 16])); // A bogus dimension must never allocate a dense grid.
    out.extend(record(145, &[]));
    out.extend(data);
    out.extend(record(146, &[]));
    out.extend(tail);
    out.extend(record(130, &[]));
    out
}
fn book(date1904: bool) -> Vec<u8> {
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
fn sst() -> Vec<u8> {
    let mut out = record(159, &[1, 0, 0, 0, 1, 0, 0, 0]);
    out.extend(record(19, &rich("shared 🦀 café", true)));
    out.extend(record(160, &[]));
    out
}
fn package(book: &[u8], sheet: &[u8], strings: &[u8], roots: &str, rels: &str, types: &str) -> Vec<u8> {
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
fn file(sheet: &[u8]) -> Vec<u8> {
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
        (ROOT.to_owned(), RELS.replace("Target=\"../data/one.bin\"", "Target=\"missing.bin\""), TYPES.to_owned()),
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
    let chartsheet = RELS.replace(
        "Id=\"two\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet",
        "Id=\"two\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chartsheet",
    );
    let (wb, report) = read_xlsb(&package(&book(false), &sheet, &sst(), ROOT, &chartsheet, TYPES)).unwrap();
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
    // padding after the documented runs (e.g. 12-byte runs) is ignored, while
    // a run shorter than documented is still rejected.
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
    assert!(read_xlsb(&file(&worksheet(&[row(0), cell(62, 0, &phonetic(2, 12))].concat(), &[]))).is_err());
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
fn ptg_area(r1: u32, r2: u32, c1: u16, c2: u16) -> Vec<u8> {
    let mut out = vec![0x25];
    out.extend(r1.to_le_bytes());
    out.extend(r2.to_le_bytes());
    out.extend((c1 | 0xC000).to_le_bytes());
    out.extend((c2 | 0xC000).to_le_bytes());
    out
}
fn ptg_int(n: u16) -> Vec<u8> {
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
fn ftext(wb: &gridcraft_model::Workbook, a1: &str) -> Option<String> {
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

// Number formats from the styles part ([MS-XLSB] 2.1.7.50): BrtFmt (44) in BrtBeginFmts (615)..BrtEndFmts (616),
// BrtXF (47) in BrtBeginCellXFs (617)..BrtEndCellXFs (618); cell style XFs (626..627) are not cell XFs.
const STYLES_RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/one.bin"/><Relationship Id="two" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="../data/two.bin"/><Relationship Id="strings" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="../text/strings.bin"/><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="../text/styles.bin"/></Relationships>"#;
const STYLES_TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/records/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/data/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/data/two.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/text/strings.bin" ContentType="application/vnd.ms-excel.sharedStrings"/><Override PartName="/text/styles.bin" ContentType="application/vnd.ms-excel.styles"/></Types>"#;

fn xf(fmt: u16) -> Vec<u8> {
    // ixfeParent, iFmt, then font/fill/border/rotation/indent/alignment/flags (16 bytes in all).
    let mut data = 0u16.to_le_bytes().to_vec();
    data.extend(fmt.to_le_bytes());
    data.extend([0; 12]);
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
