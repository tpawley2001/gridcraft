//! XLSB data validation and conditional formatting ([MS-XLSB] BrtDVal, BrtBeginConditionalFormatting,
//! BrtBeginCFRule, BrtCFVO, BrtColor, BrtDXF), from original BIFF12 records assembled in Rust.
use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::{CfOperator, CfRule, CfValueKind, Color, ErrorStyle, Fill, Validation, ValidationKind, Workbook};

use super::{make_zip, minimal};
use crate::{ReadReport, read_xlsb, read_xlsx};

const ROOT: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/book.bin"/></Relationships>"#;
const RELS: &str = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="one.bin"/><Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.bin"/></Relationships>"#;
const TYPES: &str = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/xl/one.bin" ContentType="application/vnd.ms-excel.worksheet"/><Override PartName="/xl/styles.bin" ContentType="application/vnd.ms-excel.styles"/></Types>"#;

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
/// The NULL XLNullableWideString.
const NULL: [u8; 4] = [0xff; 4];
fn book() -> Vec<u8> {
    let mut out = record(131, &[]);
    let mut props = 0u32.to_le_bytes().to_vec();
    props.extend(0u32.to_le_bytes());
    props.extend(wide(""));
    out.extend(record(153, &props));
    out.extend(record(143, &[]));
    let mut data = 0u32.to_le_bytes().to_vec();
    data.extend(1u32.to_le_bytes());
    data.extend(wide("one"));
    data.extend(wide("Data"));
    out.extend(record(156, &data));
    out.extend(record(144, &[]));
    out.extend(record(132, &[]));
    out
}
/// A worksheet with one value (A1 = 1) and `tail` after its cell table.
pub(super) fn sheet(tail: &[u8]) -> Vec<u8> {
    let mut out = record(129, &[]);
    out.extend(record(145, &[]));
    let mut row = 0u32.to_le_bytes().to_vec();
    row.extend([0; 13]);
    out.extend(record(0, &row));
    let mut cell = 0u32.to_le_bytes().to_vec();
    cell.extend([0; 4]);
    cell.extend(1.0f64.to_le_bytes());
    out.extend(record(5, &cell));
    out.extend(record(146, &[]));
    out.extend(tail);
    out.extend(record(130, &[]));
    out
}
/// A styles part whose differential formats are `dxfs` (BrtDXF payloads).
fn styles(dxfs: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(278, &[]);
    out.extend(record(505, &(dxfs.len() as u32).to_le_bytes()));
    for d in dxfs {
        out.extend(record(507, d));
    }
    out.extend(record(506, &[]));
    out.extend(record(279, &[]));
    out
}
fn file(tail: &[u8], dxfs: &[Vec<u8>]) -> Vec<u8> {
    make_zip(&[
        ("[Content_Types].xml", TYPES.as_bytes()),
        ("_rels/.rels", ROOT.as_bytes()),
        ("xl/book.bin", &book()),
        ("xl/_rels/book.bin.rels", RELS.as_bytes()),
        ("xl/one.bin", &sheet(tail)),
        ("xl/styles.bin", &styles(dxfs)),
    ])
}
fn read(tail: &[u8], dxfs: &[Vec<u8>]) -> (Workbook, ReadReport) {
    read_xlsb(&file(tail, dxfs)).unwrap()
}

/// UncheckedSqRfX of (first row, last row, first column, last column) ranges.
pub(super) fn sqrfx(ranges: &[(u32, u32, u32, u32)]) -> Vec<u8> {
    let mut out = (ranges.len() as u32).to_le_bytes().to_vec();
    for (r0, r1, c0, c1) in ranges {
        for n in [r0, r1, c0, c1] {
            out.extend(n.to_le_bytes());
        }
    }
    out
}
/// A parsed formula: cce, rgce, cb (0), no rgcb.
fn parsed(rgce: &[u8]) -> Vec<u8> {
    let mut out = (rgce.len() as u32).to_le_bytes().to_vec();
    out.extend(rgce);
    out.extend(0u32.to_le_bytes());
    out
}
fn int(n: u16) -> Vec<u8> {
    let mut out = vec![0x1E];
    out.extend(n.to_le_bytes());
    out
}
/// PtgStr: a BIFF12 text constant (u16 length, UTF-16 characters).
fn str_token(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = vec![0x17];
    out.extend((units.len() as u16).to_le_bytes());
    for u in units {
        out.extend(u.to_le_bytes());
    }
    out
}
/// `LEN(D2)<5`: a relative PtgRef (value class), PtgFunc LEN, PtgInt 5, PtgLt.
fn len_d2_below_5() -> Vec<u8> {
    let mut out = vec![0x44];
    out.extend(1u32.to_le_bytes());
    out.extend((3u16 | 0xC000).to_le_bytes());
    out.extend([0x41, 32, 0]);
    out.extend(int(5));
    out.push(0x09);
    out
}
/// `MOD(ROW(),2)=0`: PtgFuncVar ROW with no argument, PtgInt 2, PtgFunc MOD, PtgInt 0, PtgEq.
fn mod_row_even() -> Vec<u8> {
    let mut out = vec![0x42, 0, 8, 0];
    out.extend(int(2));
    out.extend([0x41, 39, 0]);
    out.extend(int(0));
    out.push(0x0B);
    out
}

struct Dv<'a> {
    flags: u32,
    ranges: &'a [(u32, u32, u32, u32)],
    strings: [Option<&'a str>; 4],
    f1: Vec<u8>,
    f2: Vec<u8>,
}
/// BrtDVal: flags, sqrfx, DValStrings (error title, error, prompt title, prompt), two formulas.
fn dval(d: &Dv<'_>) -> Vec<u8> {
    let mut out = d.flags.to_le_bytes().to_vec();
    out.extend(sqrfx(d.ranges));
    for s in d.strings {
        out.extend(s.map_or(NULL.to_vec(), wide));
    }
    out.extend(parsed(&d.f1));
    out.extend(if d.f2.is_empty() { parsed(&[]) } else { parsed(&d.f2) });
    out
}
fn dvals(list: &[Vec<u8>]) -> Vec<u8> {
    let mut out = record(573, &[0; 18]);
    for d in list {
        out.extend(record(64, d));
    }
    out.extend(record(574, &[]));
    out
}

struct Rule<'a> {
    kind: u32,
    template: u32,
    dxf: u32,
    priority: i32,
    param: u32,
    flags: u16,
    text: Option<&'a str>,
    formulas: [Vec<u8>; 2],
}
impl Rule<'_> {
    fn new(kind: u32, template: u32) -> Self {
        Rule { kind, template, dxf: u32::MAX, priority: 1, param: 0, flags: 0, text: None, formulas: [vec![], vec![]] }
    }
}
/// BrtBeginCFRule.
fn cf_rule(r: &Rule<'_>) -> Vec<u8> {
    let mut out = vec![];
    for n in [r.kind, r.template, r.dxf, r.priority as u32, r.param, 0, 0] {
        out.extend(n.to_le_bytes());
    }
    out.extend(r.flags.to_le_bytes());
    for f in &r.formulas {
        out.extend((f.len() as u32).to_le_bytes());
    }
    out.extend(0u32.to_le_bytes()); // cbFmla3
    out.extend(r.text.map_or(NULL.to_vec(), wide));
    for f in r.formulas.iter().filter(|f| !f.is_empty()) {
        out.extend(parsed(f));
    }
    out
}
/// A BrtBeginConditionalFormatting collection on `ranges` with the given rule collections.
pub(super) fn cond_format(ranges: &[(u32, u32, u32, u32)], rules: &[Vec<u8>]) -> Vec<u8> {
    let mut head = (rules.len() as u32).to_le_bytes().to_vec();
    head.extend(0u32.to_le_bytes()); // fPivot
    head.extend(sqrfx(ranges));
    let mut out = record(461, &head);
    for r in rules {
        out.extend(r);
    }
    out.extend(record(462, &[]));
    out
}
/// One rule collection: BrtBeginCFRule, the given inner records, BrtEndCFRule.
fn rule_records(rule: &Rule<'_>, inner: &[u8]) -> Vec<u8> {
    let mut out = record(463, &cf_rule(rule));
    out.extend(inner);
    out.extend(record(464, &[]));
    out
}
/// BrtCFVO without a formula.
fn cfvo(kind: u32, n: f64) -> Vec<u8> {
    let mut out = kind.to_le_bytes().to_vec();
    out.extend(n.to_le_bytes());
    out.extend(1u32.to_le_bytes()); // fSaveGTE
    out.extend(1u32.to_le_bytes()); // fGTE
    out.extend(0u32.to_le_bytes()); // cbFmla
    record(471, &out)
}
/// An RGB BrtColor / XFPropColor.
fn rgb(c: u32) -> Vec<u8> {
    vec![(2 << 1) | 1, 0, 0, 0, (c >> 16) as u8, (c >> 8) as u8, c as u8, 0xff]
}
/// A BrtDXF stating a solid fill with background colour `c`.
fn fill_dxf(c: u32) -> Vec<u8> {
    let mut out = vec![0, 0, 0, 0];
    out.extend(2u16.to_le_bytes());
    out.extend([0, 0, 5, 0, 1]); // FillPattern solid
    out.extend([2, 0, 12, 0]); // background colour
    out.extend(rgb(c));
    out
}

#[test]
fn xlsb_list_validation_with_drop_down_and_messages() {
    // List (3), information style (2 << 4), allow blank, drop-down shown, input and error shown.
    let flags = 3 | 2 << 4 | 1 << 8 | 1 << 18 | 1 << 19;
    let d = Dv {
        flags,
        ranges: &[(1, 9, 1, 1)],
        strings: [Some("Oops"), Some("Pick Yes or No"), Some("Answer"), Some("Yes or No?")],
        f1: str_token("Yes\0No"),
        f2: vec![],
    };
    let (wb, report) = read(&dvals(&[dval(&d)]), &[]);
    assert!(!report.warnings.iter().any(|w| w.contains("skipped")), "{:?}", report.warnings);
    let v = &wb.sheets[0].validations;
    assert_eq!(v.len(), 1);
    assert_eq!(
        v[0],
        Validation {
            ranges: vec![RangeRef::parse("B2:B10").unwrap()],
            kind: ValidationKind::List,
            op: CfOperator::Between,
            f1: "\"Yes,No\"".into(),
            f2: None,
            allow_blank: true,
            in_cell_dropdown: true,
            input_title: "Answer".into(),
            input_message: "Yes or No?".into(),
            show_input: true,
            error_title: "Oops".into(),
            error_message: "Pick Yes or No".into(),
            error_style: ErrorStyle::Information,
            show_error: true,
        }
    );
    // fSuppressCombo hides the drop-down; the list is the same as the .xlsx reader's.
    let hidden = Dv { flags: flags | 1 << 9, ..d };
    let (wb, _) = read(&dvals(&[dval(&hidden)]), &[]);
    assert!(!wb.sheets[0].validations[0].in_cell_dropdown);
    let xml = r#"<sheetData/><dataValidations count="1"><dataValidation type="list" sqref="B2:B10"><formula1>"Yes,No"</formula1></dataValidation></dataValidations>"#;
    let (x, _) = read_xlsx(&minimal(xml, &[], "", "")).unwrap();
    assert_eq!(x.sheets[0].validations[0].f1, wb.sheets[0].validations[0].f1);
}

#[test]
fn xlsb_whole_number_between_validation_with_stop_style() {
    let d = Dv { flags: 1 | 1 << 19, ranges: &[(1, 9, 2, 2)], strings: [None; 4], f1: int(1), f2: int(10) };
    let (wb, _) = read(&dvals(&[dval(&d)]), &[]);
    let v = &wb.sheets[0].validations[0];
    assert_eq!(v.ranges, vec![RangeRef::parse("C2:C10").unwrap()]);
    assert_eq!((v.kind, v.op, v.error_style), (ValidationKind::Whole, CfOperator::Between, ErrorStyle::Stop));
    assert_eq!((v.f1.as_str(), v.f2.as_deref()), ("1", Some("10")));
    assert!(v.show_error && !v.show_input && !v.allow_blank);
    assert_eq!((v.error_title.as_str(), v.input_message.as_str()), ("", ""));
    // Greater than (4 << 20): one formula only.
    let gt = Dv { flags: 1 | 4 << 20, ranges: &[(1, 9, 2, 2)], strings: [None; 4], f1: int(3), f2: vec![] };
    let (wb, _) = read(&dvals(&[dval(&gt)]), &[]);
    let v = &wb.sheets[0].validations[0];
    assert_eq!((v.op, v.f1.as_str(), v.f2.as_deref()), (CfOperator::Greater, "3", None));
}

#[test]
fn xlsb_custom_validation_formula() {
    let d = Dv { flags: 7, ranges: &[(1, 9, 3, 3)], strings: [None; 4], f1: len_d2_below_5(), f2: vec![] };
    let (wb, _) = read(&dvals(&[dval(&d)]), &[]);
    let v = &wb.sheets[0].validations[0];
    assert_eq!((v.kind, v.f1.as_str()), (ValidationKind::Custom, "LEN(D2)<5"));
    let xml = r#"<sheetData/><dataValidations count="1"><dataValidation type="custom" sqref="D2:D10"><formula1>LEN(D2)&lt;5</formula1></dataValidation></dataValidations>"#;
    let (x, _) = read_xlsx(&minimal(xml, &[], "", "")).unwrap();
    assert_eq!(&x.sheets[0].validations[0], v);
}

#[test]
fn xlsb_cell_is_rule_with_red_fill_matches_the_xlsx_reader() {
    let rule = Rule { dxf: 0, param: 5, formulas: [int(5), vec![]], ..Rule::new(1, 0) };
    let tail = cond_format(&[(0, 9, 0, 0)], &[rule_records(&rule, &[])]);
    let (wb, report) = read(&tail, &[fill_dxf(0xFF0000)]);
    assert!(!report.warnings.iter().any(|w| w.contains("skipped")), "{:?}", report.warnings);
    let cf = &wb.sheets[0].cond_formats;
    assert_eq!(cf.len(), 1);
    let CfRule::CellIs { op, a, b, style } = &cf[0].rule else { panic!("{:?}", cf[0].rule) };
    assert_eq!((*op, a.as_str(), b.as_deref()), (CfOperator::Greater, "5", None));
    assert_eq!(style.fill, Fill::solid(Color::Rgb(0xFF0000)));
    let styles = r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border/></borders><cellXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellXfs><dxfs count="1"><dxf><fill><patternFill patternType="solid"><bgColor rgb="FFFF0000"/></patternFill></fill></dxf></dxfs></styleSheet>"#;
    let rels = r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;
    let body = r#"<sheetData/><conditionalFormatting sqref="A1:A10"><cfRule type="cellIs" dxfId="0" priority="1" operator="greaterThan"><formula>5</formula></cfRule></conditionalFormatting>"#;
    let (x, _) = read_xlsx(&minimal(body, &[("xl/styles.xml", styles)], rels, "")).unwrap();
    assert_eq!(x.sheets[0].cond_formats, wb.sheets[0].cond_formats);
}

#[test]
fn xlsb_expression_rule_with_stop_if_true_and_priority() {
    let rule = Rule { dxf: 0, priority: 2, flags: 0b10, formulas: [mod_row_even(), vec![]], ..Rule::new(2, 1) };
    let tail = cond_format(&[(0, 19, 0, 3)], &[rule_records(&rule, &[])]);
    let (wb, _) = read(&tail, &[fill_dxf(0x00FF00)]);
    let f = &wb.sheets[0].cond_formats[0];
    assert_eq!(f.ranges, vec![RangeRef::parse("A1:D20").unwrap()]);
    assert_eq!((f.priority, f.stop_if_true), (2, true));
    let CfRule::Expression { formula, style } = &f.rule else { panic!("{:?}", f.rule) };
    assert_eq!(formula, "MOD(ROW(),2)=0");
    assert_eq!(style.fill, Fill::solid(Color::Rgb(0x00FF00)));
}

#[test]
fn xlsb_color_scale_data_bar_and_icon_set() {
    let mut scale = record(469, &[]);
    for v in [cfvo(2, 0.0), cfvo(5, 50.0), cfvo(3, 0.0)] {
        scale.extend(v);
    }
    for c in [0xF8696B, 0xFFEB84, 0x63BE7B] {
        scale.extend(record(564, &rgb(c)));
    }
    scale.extend(record(470, &[]));
    let mut bar = record(467, &[10, 90, 0]);
    bar.extend(cfvo(2, 0.0));
    bar.extend(cfvo(1, 100.0));
    bar.extend(record(564, &rgb(0x638EC6)));
    bar.extend(record(468, &[]));
    let mut icons = 0u32.to_le_bytes().to_vec(); // KPI3ARROWS
    icons.extend(0b100u16.to_le_bytes()); // fReverse
    let mut set = record(465, &icons);
    for p in [0.0, 33.0, 67.0] {
        set.extend(cfvo(4, p));
    }
    set.extend(record(466, &[]));
    let rules = [
        rule_records(&Rule { priority: 1, ..Rule::new(3, 2) }, &scale),
        rule_records(&Rule { priority: 2, ..Rule::new(4, 3) }, &bar),
        rule_records(&Rule { priority: 3, ..Rule::new(6, 4) }, &set),
    ];
    let (wb, report) = read(&cond_format(&[(0, 9, 1, 1)], &rules), &[]);
    assert!(!report.warnings.iter().any(|w| w.contains("skipped")), "{:?}", report.warnings);
    let cf = &wb.sheets[0].cond_formats;
    assert_eq!(cf.len(), 3);
    assert_eq!(
        cf[0].rule,
        CfRule::ColorScale {
            stops: vec![
                (CfValueKind::Min, Color::Rgb(0xF8696B)),
                (CfValueKind::Percentile(50.0), Color::Rgb(0xFFEB84)),
                (CfValueKind::Max, Color::Rgb(0x63BE7B)),
            ]
        }
    );
    assert_eq!(
        cf[1].rule,
        CfRule::DataBar { min: CfValueKind::Min, max: CfValueKind::Number(100.0), color: Color::Rgb(0x638EC6), gradient: true, show_value: false }
    );
    assert_eq!(
        cf[2].rule,
        CfRule::IconSet {
            set: "3Arrows".into(),
            thresholds: vec![CfValueKind::Percent(0.0), CfValueKind::Percent(33.0), CfValueKind::Percent(67.0)],
            reverse: true,
            show_value: true,
        }
    );
    assert_eq!(cf.iter().map(|f| f.priority).collect::<Vec<_>>(), [1, 2, 3]);
    assert!(cf.iter().all(|f| f.ranges == vec![RangeRef::new(CellRef::new(0, 1), CellRef::new(9, 1))]));
}

#[test]
fn xlsb_template_rules_map_like_the_xlsx_reader() {
    let rules = [
        rule_records(&Rule { dxf: 0, param: 2, text: Some("ab"), formulas: [int(1), vec![]], ..Rule::new(2, 8) }, &[]),
        rule_records(&Rule { dxf: 0, ..Rule::new(2, 0x1B) }, &[]),
        rule_records(&Rule { dxf: 0, param: 3, flags: 0b1_1000, ..Rule::new(5, 5) }, &[]),
        rule_records(&Rule { dxf: 0, param: 1, ..Rule::new(2, 0x1A) }, &[]),
        rule_records(&Rule { dxf: 0, param: 2, formulas: [int(1), vec![]], ..Rule::new(2, 0x12) }, &[]),
    ];
    let (wb, _) = read(&cond_format(&[(0, 9, 0, 0)], &rules), &[fill_dxf(0xFF0000)]);
    let kinds: Vec<_> = wb.sheets[0].cond_formats.iter().map(|f| f.rule.clone()).collect();
    let style = Box::new(gridcraft_model::Style { fill: Fill::solid(Color::Rgb(0xFF0000)), ..Default::default() });
    assert_eq!(
        kinds,
        vec![
            CfRule::BeginsWith { text: "ab".into(), style: style.clone() },
            CfRule::Duplicate { style: style.clone() },
            CfRule::Top10 { bottom: true, percent: true, rank: 3, style: style.clone() },
            CfRule::AboveAverage { below: true, equal: false, std_dev: 1, style: style.clone() },
            CfRule::TimePeriod { period: "last7Days".into(), style },
        ]
    );
}

#[test]
fn xlsb_rules_with_undecodable_formulas_are_skipped_and_others_kept() {
    let bad = Rule { dxf: 0, priority: 1, formulas: [vec![0xFF], vec![]], ..Rule::new(2, 1) };
    let good = Rule { dxf: 0, priority: 2, formulas: [mod_row_even(), vec![]], ..Rule::new(2, 1) };
    let mut tail = cond_format(&[(0, 9, 0, 0)], &[rule_records(&bad, &[]), rule_records(&good, &[])]);
    let bad_dv = Dv { flags: 7, ranges: &[(0, 0, 0, 0)], strings: [None; 4], f1: vec![0xFF], f2: vec![] };
    let good_dv = Dv { flags: 1, ranges: &[(1, 1, 1, 1)], strings: [None; 4], f1: int(1), f2: int(2) };
    tail.extend(dvals(&[dval(&bad_dv), dval(&good_dv)]));
    let (wb, report) = read(&tail, &[fill_dxf(0xFF0000)]);
    let cf = &wb.sheets[0].cond_formats;
    assert_eq!(cf.len(), 1);
    assert_eq!(cf[0].priority, 2);
    assert_eq!(wb.sheets[0].validations.len(), 1);
    assert_eq!(wb.sheets[0].validations[0].f2.as_deref(), Some("2"));
    let skipped: Vec<_> = report.warnings.iter().filter(|w| w.contains("skipped:")).collect();
    assert_eq!(skipped.len(), 2, "{:?}", report.warnings);
    assert!(skipped.iter().any(|w| w.contains("1 XLSB conditional formatting rule(s)")));
    assert!(skipped.iter().any(|w| w.contains("1 XLSB data validation(s)")));
}

#[test]
fn xlsb_truncated_or_malformed_rule_records_never_fail_the_import() {
    let rule = cf_rule(&Rule { dxf: 7, formulas: [int(5), int(9)], param: 1, ..Rule::new(1, 0) });
    let dv = dval(&Dv { flags: 1, ranges: &[(1, 9, 2, 2)], strings: [Some("t"), None, None, None], f1: int(1), f2: int(10) });
    for end in 0..rule.len() {
        let tail = cond_format(&[(0, 0, 0, 0)], &[[record(463, &rule[..end]), record(464, &[])].concat()]);
        let (wb, _) = read(&tail, &[vec![0, 0, 0, 0, 5, 0]]);
        assert!(wb.sheets[0].cond_formats.is_empty(), "prefix {end}");
    }
    for end in 0..dv.len() {
        let (wb, _) = read(&dvals(&[dv[..end].to_vec()]), &[]);
        assert!(wb.sheets[0].validations.is_empty(), "prefix {end}");
    }
    // A full rule pointing past the dxf list keeps no formatting; out-of-grid ranges are dropped.
    let tail = cond_format(&[(0, 0, 0, 0), (u32::MAX, u32::MAX, 0, 0)], &[record(463, &rule), record(464, &[])]);
    let (wb, report) = read(&tail, &[vec![0, 0, 0, 0, 1, 0, 0, 0, 3, 0]]);
    assert_eq!(wb.sheets[0].cond_formats.len(), 1);
    assert_eq!(wb.sheets[0].cond_formats[0].ranges.len(), 1);
    assert!(report.warnings.iter().any(|w| w.contains("damaged XLSB differential format")), "{:?}", report.warnings);
    // An unterminated rule and stray sub-records outside a rule are skipped.
    let mut tail = record(461, &[&[0u8; 8][..], &sqrfx(&[(0, 0, 0, 0)])].concat());
    tail.extend(cfvo(1, 1.0));
    tail.extend(record(463, &rule));
    let (wb, report) = read(&tail, &[]);
    assert!(wb.sheets[0].cond_formats.is_empty());
    assert!(report.warnings.iter().any(|w| w.contains("unterminated rule")), "{:?}", report.warnings);
}
