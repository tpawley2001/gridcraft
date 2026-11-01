//! `.xls` Excel 2007+ conditional formats (CondFmt12 / CF12 / CFEx, [MS-XLS] 2.4.42-2.4.43,
//! 2.4.57), each compared with what the `.xlsx` reader makes of the equivalent `<cfRule>`.

use super::xls_tests::{cat, cf, cond_fmt, one_sheet_with, ptg_ref, ptg_str, rec, u16s};
use crate::{read_xls, read_xlsx};
use gridcraft_model::{CfRule, CondFormat};

/// FrtRefHeader(U): rt, grbitFrt, an empty Ref8.
fn frt(rt: u16, frt_ref: bool) -> Vec<u8> {
    cat(&[&u16s(&[rt, u16::from(frt_ref)]), &[0; 8]])
}

/// CondFmt12 (0x0879) with `nID` = `id` over `refs` (rwFirst, rwLast, colFirst, colLast).
fn cond_fmt12(id: u16, ccf: u16, refs: &[[u16; 4]]) -> Vec<u8> {
    let mut b = frt(0x0879, true);
    b.extend(u16s(&[ccf, id << 1]));
    b.extend(u16s(&refs[0]));
    b.extend((refs.len() as u16).to_le_bytes());
    for r in refs {
        b.extend(u16s(r));
    }
    rec(0x0879, &b)
}

struct Cf12<'a> {
    ct: u8,
    cp: u8,
    rgce1: &'a [u8],
    stop: bool,
    priority: u16,
    template: u16,
    params: [u8; 16],
    rgb_ct: &'a [u8],
}

impl Default for Cf12<'_> {
    fn default() -> Self {
        Cf12 { ct: 2, cp: 0, rgce1: &[], stop: false, priority: 1, template: 1, params: [0; 16], rgb_ct: &[] }
    }
}

/// CF12 (0x087A) without formatting (an empty DXFN12).
fn cf12(r: &Cf12<'_>) -> Vec<u8> {
    let mut b = frt(0x087A, false);
    b.extend([r.ct, r.cp]);
    b.extend(u16s(&[r.rgce1.len() as u16, 0]));
    b.extend(0u32.to_le_bytes()); // cbDxf
    b.extend([0, 0]); // reserved
    b.extend(r.rgce1);
    b.extend(0u16.to_le_bytes()); // fmlaActive
    b.push(if r.stop { 0x02 } else { 0 });
    b.extend(u16s(&[r.priority, r.template]));
    b.push(16);
    b.extend(r.params);
    b.extend(r.rgb_ct);
    rec(0x087A, &b)
}

/// CFEx (0x087B) extending classic CF record `icf` of CondFmt `id`.
fn cf_ex(id: u16, icf: u16, template: u8, priority: u16, stop: bool, params: [u8; 16]) -> Vec<u8> {
    let mut b = frt(0x087B, true);
    b.extend(0u32.to_le_bytes()); // fIsCF12
    b.extend(u16s(&[id, icf]));
    b.extend([0, template]);
    b.extend(priority.to_le_bytes());
    b.push(0x01 | if stop { 0x02 } else { 0 }); // fActive, fStopIfTrue
    b.push(0); // fHasDXF
    b.push(16);
    b.extend(params);
    rec(0x087B, &b)
}

/// CFVO: type with a number (or none for min/max).
fn cfvo(ty: u8, n: Option<f64>) -> Vec<u8> {
    let mut b = vec![ty, 0, 0];
    if let Some(n) = n {
        b.extend(n.to_le_bytes());
    }
    b
}

/// CFColor, RGB.
fn rgb(c: u32) -> Vec<u8> {
    let mut b = 2u32.to_le_bytes().to_vec();
    b.extend([(c >> 16) as u8, (c >> 8) as u8, c as u8, 0xFF]);
    b.extend(0f64.to_le_bytes());
    b
}

/// A DXFN with every block absent: no formatting.
fn empty_dxfn() -> Vec<u8> {
    cat(&[&0x003F_FFFFu32.to_le_bytes(), &0u16.to_le_bytes()])
}

fn params(first: &[u8]) -> [u8; 16] {
    let mut p = [0; 16];
    p[..first.len()].copy_from_slice(first);
    p
}

/// The rules of a one-sheet `.xls` made of `records`, with the read warnings.
fn xls_rules(records: &[u8]) -> (Vec<CondFormat>, Vec<String>) {
    let (wb, report) = read_xls(&one_sheet_with(records)).unwrap();
    (wb.sheets[0].cond_formats.clone(), report.warnings)
}

/// What the `.xlsx` reader makes of one `<conditionalFormatting>`.
fn xlsx_rule(sqref: &str, rule: &str) -> CondFormat {
    let body = format!(r#"<sheetData/><conditionalFormatting sqref="{sqref}">{rule}</conditionalFormatting>"#);
    let (wb, _) = read_xlsx(&super::minimal(&body, &[], "", "")).unwrap();
    assert_eq!(wb.sheets[0].cond_formats.len(), 1, "{rule}");
    wb.sheets[0].cond_formats[0].clone()
}

fn gradient_3() -> Vec<u8> {
    let mut b = vec![0, 0, 0, 3, 3, 0x03];
    for (v, at) in [(cfvo(2, None), 0.0), (cfvo(5, Some(50.0)), 0.5), (cfvo(3, None), 1.0f64)] {
        b.extend(v);
        b.extend(at.to_le_bytes());
    }
    for (at, c) in [(0.0f64, 0xF8696B), (0.5, 0xFFEB84), (1.0, 0x63BE7B)] {
        b.extend(at.to_le_bytes());
        b.extend(rgb(c));
    }
    b
}

fn databar_blue() -> Vec<u8> {
    cat(&[&[0, 0, 0, 0x02, 0, 100], &rgb(0x638EC6), &cfvo(2, None), &cfvo(3, None)])
}

fn arrows_reversed() -> Vec<u8> {
    let mut b = vec![0, 0, 0, 3, 0, 0x04];
    for p in [0.0, 33.0, 67.0] {
        b.extend(cfvo(4, Some(p)));
        b.extend([1, 0, 0, 0, 0]);
    }
    b
}

#[test]
fn colour_scale_data_bar_icon_set_top_and_average_match_xlsx() {
    let gradient = gradient_3();
    let bar = databar_blue();
    let icons = arrows_reversed();
    let top = cat(&[&u16s(&[4]), &[0, 0x03], &u16s(&[10])]);
    let truth = [0x1D, 0x01];
    let records = cat(&[
        &cond_fmt12(1, 1, &[[0, 9, 0, 0]]),
        &cf12(&Cf12 { ct: 3, priority: 1, template: 2, rgb_ct: &gradient, ..Default::default() }),
        &cond_fmt12(2, 1, &[[0, 9, 1, 1]]),
        &cf12(&Cf12 { ct: 4, priority: 2, template: 3, rgb_ct: &bar, ..Default::default() }),
        &cond_fmt12(3, 1, &[[0, 9, 2, 2]]),
        &cf12(&Cf12 { ct: 6, priority: 3, template: 4, rgb_ct: &icons, ..Default::default() }),
        &cond_fmt12(4, 1, &[[0, 9, 3, 3]]),
        &cf12(&Cf12 { ct: 5, priority: 4, template: 5, params: params(&[0x03, 10]), rgb_ct: &top, ..Default::default() }),
        &cond_fmt12(5, 1, &[[0, 9, 4, 4]]),
        &cf12(&Cf12 { ct: 2, priority: 5, template: 0x19, rgce1: &truth, ..Default::default() }),
    ]);
    let (rules, warnings) = xls_rules(&records);
    assert_eq!(rules.len(), 5, "{warnings:?}");
    assert!(!warnings.iter().any(|w| w.contains("conditional format")), "{warnings:?}");
    let expected = [
        xlsx_rule(
            "A1:A10",
            r#"<cfRule type="colorScale" priority="1"><colorScale><cfvo type="min"/><cfvo type="percentile" val="50"/><cfvo type="max"/><color rgb="FFF8696B"/><color rgb="FFFFEB84"/><color rgb="FF63BE7B"/></colorScale></cfRule>"#,
        ),
        xlsx_rule(
            "B1:B10",
            r#"<cfRule type="dataBar" priority="2"><dataBar><cfvo type="min"/><cfvo type="max"/><color rgb="FF638EC6"/></dataBar></cfRule>"#,
        ),
        xlsx_rule(
            "C1:C10",
            r#"<cfRule type="iconSet" priority="3"><iconSet iconSet="3Arrows" reverse="1"><cfvo type="percent" val="0"/><cfvo type="percent" val="33"/><cfvo type="percent" val="67"/></iconSet></cfRule>"#,
        ),
        xlsx_rule("D1:D10", r#"<cfRule type="top10" priority="4" percent="1" rank="10"/>"#),
        xlsx_rule("E1:E10", r#"<cfRule type="aboveAverage" priority="5"><formula>TRUE</formula></cfRule>"#),
    ];
    for (got, want) in rules.iter().zip(&expected) {
        assert_eq!(got, want);
    }
}

/// `NOT(ISERROR(SEARCH("abc",F1)))`, as Excel writes a "contains text" rule.
fn contains_abc() -> Vec<u8> {
    cat(&[&ptg_str("abc"), &ptg_ref(0, 5), &[0x42, 2, 82, 0], &[0x41, 3, 0], &[0x41, 38, 0]])
}

#[test]
fn cfex_template_makes_a_classic_formula_rule_contains_text() {
    let records =
        cat(&[&cond_fmt(1, &[[0, 9, 5, 5]]), &cf(2, 0, &empty_dxfn(), &contains_abc(), &[]), &cf_ex(0, 0, 0x08, 6, false, params(&[0, 0]))]);
    let (rules, warnings) = xls_rules(&records);
    assert_eq!(rules.len(), 1, "{warnings:?}");
    let want = xlsx_rule(
        "F1:F10",
        r#"<cfRule type="containsText" priority="6" operator="containsText" text="abc"><formula>NOT(ISERROR(SEARCH("abc",F1)))</formula></cfRule>"#,
    );
    assert_eq!(rules[0], want);
    assert!(matches!(&rules[0].rule, CfRule::ContainsText { text, .. } if text == "abc"));
}

#[test]
fn cf12_text_template_reads_begins_with() {
    let rgce = contains_abc();
    let records = cat(&[
        &cond_fmt12(1, 1, &[[0, 9, 5, 5]]),
        &cf12(&Cf12 { ct: 2, priority: 1, template: 0x08, rgce1: &rgce, params: params(&[2, 0]), ..Default::default() }),
    ]);
    let (rules, warnings) = xls_rules(&records);
    let want = xlsx_rule(
        "F1:F10",
        r#"<cfRule type="beginsWith" priority="1" operator="beginsWith" text="abc"><formula>LEFT(F1,3)="abc"</formula></cfRule>"#,
    );
    assert_eq!(rules, vec![want], "{warnings:?}");
}

#[test]
fn cfex_gives_a_classic_rule_priority_and_stop_if_true() {
    let records = cat(&[&cond_fmt(1, &[[0, 9, 6, 6]]), &cf(2, 0, &empty_dxfn(), &[0x1D, 0x01], &[]), &cf_ex(0, 0, 0x01, 2, true, [0; 16])]);
    let (rules, warnings) = xls_rules(&records);
    let want = xlsx_rule("G1:G10", r#"<cfRule type="expression" priority="2" stopIfTrue="1"><formula>TRUE</formula></cfRule>"#);
    assert_eq!(rules, vec![want], "{warnings:?}");
}

#[test]
fn cfex_announcing_a_cf12_rule_adds_it_to_the_classic_ranges() {
    let gradient = gradient_3();
    let records = cat(&[
        &cond_fmt(1, &[[0, 9, 0, 0]]),
        &cf(2, 0, &empty_dxfn(), &[0x1D, 0x01], &[]),
        &rec(0x087B, &cat(&[&frt(0x087B, true), &1u32.to_le_bytes(), &0u16.to_le_bytes()])),
        &cf12(&Cf12 { ct: 3, priority: 7, template: 2, rgb_ct: &gradient, ..Default::default() }),
    ]);
    let (rules, warnings) = xls_rules(&records);
    assert_eq!(rules.len(), 2, "{warnings:?}");
    assert!(rules.iter().all(|r| r.ranges == rules[0].ranges));
    assert!(matches!(&rules[1].rule, CfRule::ColorScale { stops } if stops.len() == 3));
    assert_eq!(rules[1].priority, 7);
}

#[test]
fn truncated_cf12_is_skipped_with_a_warning_and_other_rules_kept() {
    let bar = databar_blue();
    let good = cf12(&Cf12 { ct: 4, priority: 2, template: 3, rgb_ct: &bar, ..Default::default() });
    let gradient = gradient_3();
    let whole = cf12(&Cf12 { ct: 3, priority: 1, template: 2, rgb_ct: &gradient, ..Default::default() });
    let payload = &whole[4..whole.len() - 20];
    let truncated = rec(0x087A, payload);
    let records = cat(&[&cond_fmt12(1, 2, &[[0, 9, 0, 0]]), &truncated, &good]);
    let (rules, warnings) = xls_rules(&records);
    assert_eq!(rules.len(), 1, "{warnings:?}");
    assert!(matches!(rules[0].rule, CfRule::DataBar { .. }));
    assert!(warnings.iter().any(|w| w.contains("Excel 2007+ conditional formats")), "{warnings:?}");
    assert!(!warnings.iter().any(|w| w.contains("conditional formats in .xls files are not imported yet")), "{warnings:?}");
    // Cut anywhere, a CF12 or CFEx record is skipped without panicking.
    for rec_bytes in [&whole, &good, &cf_ex(0, 0, 0x08, 6, false, [0; 16])] {
        for n in 0..rec_bytes.len() - 4 {
            let cut = rec(u16::from_le_bytes([rec_bytes[0], rec_bytes[1]]), &rec_bytes[4..4 + n]);
            let records =
                cat(&[&cond_fmt(1, &[[0, 0, 0, 0]]), &cf(2, 0, &empty_dxfn(), &[0x1D, 0x01], &[]), &cond_fmt12(1, 1, &[[0, 0, 1, 1]]), &cut]);
            let (rules, _) = xls_rules(&records);
            assert!(!rules.is_empty());
        }
    }
}

#[test]
fn unknown_cf12_kinds_warn_once_per_kind() {
    let records = cat(&[
        &cond_fmt12(1, 2, &[[0, 9, 0, 0]]),
        &cf12(&Cf12 { ct: 9, ..Default::default() }),
        &cf12(&Cf12 { ct: 9, priority: 2, ..Default::default() }),
        &cf12(&Cf12 { ct: 6, priority: 3, rgb_ct: &[0, 0, 0, 3, 40, 0], ..Default::default() }),
    ]);
    let (rules, warnings) = xls_rules(&records);
    assert!(rules.is_empty());
    assert_eq!(warnings.iter().filter(|w| w.contains("unknown rule type")).count(), 1, "{warnings:?}");
    assert_eq!(warnings.iter().filter(|w| w.contains("unknown icon sets")).count(), 1, "{warnings:?}");
}
