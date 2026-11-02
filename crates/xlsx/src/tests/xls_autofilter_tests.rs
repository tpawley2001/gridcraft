//! `.xls` AutoFilter import: AutoFilterInfo ([MS-XLS] 2.4.8), AutoFilter (2.4.6) and the
//! `_FilterDatabase` built-in name (2.4.150), compared with what the same filter in an `.xlsx`
//! gives.

use gridcraft_core::RangeRef;
use gridcraft_core::color::INDEXED;
use gridcraft_model::{AutoFilter, Color, FilterCriterion};

use crate::tests::minimal;
use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_tests::compound;

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &cat(&[&u16s(&[0x0600, dt, 0x0DBB, 0x07CC]), &[0; 8]]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

/// Lbl `_FilterDatabase` (built-in name 0x0D) for the sheet with itab `itab`, over `area`
/// (rwFirst, rwLast, colFirst, colLast).
fn filter_database(itab: u16, area: [u16; 4]) -> Vec<u8> {
    // PtgArea3d (2.5.198.28) through the internal SupBook's first XTI (ixti 0).
    let rgce = cat(&[&[0x3B], &0u16.to_le_bytes(), &u16s(&area)]);
    rec(0x0018, &cat(&[&u16s(&[0x0020]), &[0, 1], &u16s(&[rgce.len() as u16, 0, itab]), &[0; 4], &[0, 0x0D], &rgce]))
}

/// AFDOper (2.5.5): vt, grbitSign and eight value bytes.
fn doper(vt: u8, sign: u8, value: &[u8; 8]) -> Vec<u8> {
    cat(&[&[vt, sign], value])
}

/// The AFDOper that says there is no condition.
fn no_doper() -> Vec<u8> {
    doper(0, 0, &[0; 8])
}

/// AFDOper for an Xnum (2.5.342).
fn num_doper(sign: u8, n: f64) -> Vec<u8> {
    doper(0x04, sign, &n.to_le_bytes())
}

/// AFDOper for a string: AFDOperStr's cch (2.5.8) follows four unused bytes.
fn str_doper(sign: u8, s: &str) -> Vec<u8> {
    let mut value = [0u8; 8];
    value[4] = s.len() as u8;
    value[5] = u8::from(!s.contains(['*', '?']));
    doper(0x06, sign, &value)
}

/// AutoFilter (2.4.6): iEntry, grbit, the two AFDOpers, then the strings of vt = 0x06 dopers.
fn auto_filter(entry: u16, flags: u16, first: &[u8], second: &[u8], strings: &[&str]) -> Vec<u8> {
    let mut p = cat(&[&u16s(&[entry, flags]), first, second]);
    for s in strings {
        p.extend_from_slice(&[0]); // fHighByte: compressed characters
        p.extend_from_slice(s.as_bytes());
    }
    rec(0x009E, &p)
}

fn filter_info(entries: u16) -> Vec<u8> {
    rec(0x009D, &u16s(&[entries]))
}

/// A one-sheet workbook whose globals hold `globals_extra` and whose sheet holds `sheet_records`.
fn xls_file(globals_extra: &[u8], sheet_records: &[u8]) -> Vec<u8> {
    let boundsheet = |pos: u32| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, 6, 0], b"Sheet1"]));
    let globals = |pos: u32| {
        cat(&[
            &bof(0x0005),
            &boundsheet(pos),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &rec(0x0017, &cat(&[&u16s(&[1]), &u16s(&[0, 0, 0])])),
            globals_extra,
            &eof(),
        ])
    };
    let sheet = cat(&[&bof(0x0010), sheet_records, &rec(0x0203, &cat(&[&u16s(&[0, 0, 0]), &7.0f64.to_le_bytes()])), &eof()]);
    compound("Workbook", &cat(&[&globals(globals(0).len() as u32), &sheet]))
}

fn autofilter(file: &[u8]) -> (Option<AutoFilter>, Vec<String>) {
    let (wb, report) = read_xls(file).unwrap();
    (wb.sheets[0].autofilter.clone(), report.warnings)
}

/// The AutoFilter the `.xlsx` reader gives for a sheet holding `body`.
fn from_xlsx(body: &str) -> Option<AutoFilter> {
    let (wb, _) = read_xlsx(&minimal(body, &[], "", "")).unwrap();
    wb.sheets[0].autofilter.clone()
}

/// `$A$1:$C$10` on Sheet1: rows 0-9, columns 0-2.
const A1_C10: [u16; 4] = [0, 9, 0, 2];

#[test]
fn xls_autofilter_range_without_criteria() {
    let file = xls_file(&filter_database(1, A1_C10), &filter_info(3));
    let (af, warnings) = autofilter(&file);
    let af = af.expect("the sheet has an AutoFilter");
    assert_eq!(af.range, RangeRef::parse("A1:C10").unwrap());
    assert!(af.criteria.is_empty());
    assert!(!warnings.iter().any(|w| w.contains("AutoFilter")), "{warnings:?}");
}

#[test]
fn xls_autofilter_two_numeric_conditions_match_the_xlsx_reader() {
    let record = auto_filter(1, 0, &num_doper(6, 10.0), &num_doper(1, 20.5), &[]);
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &record])));
    let want = from_xlsx(
        r#"<autoFilter ref="A1:C10"><filterColumn colId="1"><customFilters and="1"><customFilter operator="greaterThanOrEqual" val="10"/><customFilter operator="lessThan" val="20.5"/></customFilters></filterColumn></autoFilter>"#,
    );
    assert_eq!(af.map(|a| a.criteria), want.map(|a| a.criteria), "{warnings:?}");
    assert!(!warnings.iter().any(|w| w.contains("AutoFilter")), "{warnings:?}");
}

#[test]
fn xls_autofilter_string_condition_keeps_its_wildcard() {
    let record = auto_filter(0, 0, &str_doper(2, "ab*"), &no_doper(), &["ab*"]);
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &record])));
    let got = af.expect("the sheet has an AutoFilter").criteria;
    let want = from_xlsx(
        r#"<autoFilter ref="A1:C10"><filterColumn colId="0"><customFilters><customFilter val="ab*"/></customFilters></filterColumn></autoFilter>"#,
    );
    assert_eq!(Some(got), want.map(|a| a.criteria), "{warnings:?}");
}

#[test]
fn xls_autofilter_top_n() {
    // fTopN, fTop = 0 (the bottom items), fPercent, wTopN = 5.
    let flags = 0x0010 | 0x0040 | (5 << 7);
    let record = auto_filter(2, flags, &no_doper(), &no_doper(), &[]);
    let (af, _) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &record])));
    assert_eq!(af.expect("the sheet has an AutoFilter").criteria, vec![(2, FilterCriterion::Top10 { bottom: true, percent: true, count: 5 })]);
}

#[test]
fn xls_autofilter_blanks_and_non_blanks() {
    let blanks = auto_filter(0, 0, &doper(0x0C, 0, &[0; 8]), &no_doper(), &[]);
    let (af, _) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &blanks])));
    assert_eq!(af.expect("the sheet has an AutoFilter").criteria, vec![(0, FilterCriterion::Values { values: vec![], blanks: true })]);
    let non_blanks = auto_filter(0, 0, &doper(0x0E, 0, &[0; 8]), &no_doper(), &[]);
    let (af, _) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &non_blanks])));
    let want = from_xlsx(
        r#"<autoFilter ref="A1:C10"><filterColumn colId="0"><customFilters><customFilter operator="notEqual" val=" "/></customFilters></filterColumn></autoFilter>"#,
    );
    assert_eq!(af.map(|a| a.criteria), want.map(|a| a.criteria));
}

#[test]
fn xls_autofilter_needs_its_range_and_skips_excel_2007_filters() {
    // AutoFilterInfo with no `_FilterDatabase` name: no AutoFilter, one warning.
    let (af, warnings) = autofilter(&xls_file(&[], &filter_info(3)));
    assert!(af.is_none());
    assert!(warnings.iter().any(|w| w.contains("_FilterDatabase")), "{warnings:?}");
    // The name without AutoFilterInfo is an advanced filter leftover: no AutoFilter.
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &[]));
    assert!(af.is_none());
    assert!(!warnings.iter().any(|w| w.contains("AutoFilter")), "{warnings:?}");
    // A truncated AutoFilter12 column keeps no criterion, with one warning.
    let good = auto_filter(1, 0, &num_doper(4, 5.0), &no_doper(), &[]);
    let filter12 = rec(0x087E, &cat(&[&[0; 12], &u16s(&[0]), &[0; 4], &[0; 24]]));
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &good, &filter12])));
    let want = FilterCriterion::Custom { a: (">".into(), "5".into()), b: None, and: false };
    assert_eq!(af.expect("the sheet has an AutoFilter").criteria, vec![(1, want)]);
    assert!(warnings.iter().any(|w| w.contains("could not be read")), "{warnings:?}");
}

#[test]
fn xls_autofilter_truncated_record_keeps_the_others() {
    let good = auto_filter(2, 0, &num_doper(4, 5.0), &no_doper(), &[]);
    let truncated = rec(0x009E, &u16s(&[0, 0, 0x04]));
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &truncated, &good])));
    let af = af.expect("the sheet has an AutoFilter");
    assert_eq!(af.criteria, vec![(2, FilterCriterion::Custom { a: (">".into(), "5".into()), b: None, and: false })]);
    assert!(warnings.iter().any(|w| w.contains("AutoFilter criteria")), "{warnings:?}");
}

#[test]
fn xls_autofilter_saved_as_xlsx_writes_the_range() {
    let (wb, _) = read_xls(&xls_file(&filter_database(1, A1_C10), &filter_info(3))).unwrap();
    let bytes = write_xlsx(&wb).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut sheet = String::new();
    std::io::Read::read_to_string(&mut z.by_name("xl/worksheets/sheet1.xml").unwrap(), &mut sheet).unwrap();
    assert!(sheet.contains(r#"<autoFilter ref="A1:C10"/>"#), "{sheet}");
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].autofilter, wb.sheets[0].autofilter);
}

#[test]
fn xls_autofilter_belongs_to_the_sheet_its_name_names() {
    let boundsheet = |pos: u32, name: &str| rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, name.len() as u8, 0], name.as_bytes()]));
    let globals = |p1: u32, p2: u32| {
        cat(&[
            &bof(0x0005),
            &boundsheet(p1, "Sheet1"),
            &boundsheet(p2, "Sheet2"),
            &rec(0x01AE, &u16s(&[1, 0x0401])),
            &rec(0x0017, &cat(&[&u16s(&[1]), &u16s(&[0, 0, 0])])),
            &filter_database(2, [0, 4, 0, 0]),
            &eof(),
        ])
    };
    let sheet1 = cat(&[&bof(0x0010), &filter_info(1), &eof()]);
    let sheet2 = cat(&[&bof(0x0010), &filter_info(1), &eof()]);
    let p1 = globals(0, 0).len() as u32;
    let p2 = p1 + sheet1.len() as u32;
    let (wb, report) = read_xls(&compound("Workbook", &cat(&[&globals(p1, p2), &sheet1, &sheet2]))).unwrap();
    assert!(wb.sheets[0].autofilter.is_none(), "Sheet1 has no _FilterDatabase of its own");
    assert_eq!(wb.sheets[1].autofilter.as_ref().map(|a| a.range), Some(RangeRef::parse("A1:A5").unwrap()));
    assert!(report.warnings.iter().any(|w| w.contains("_FilterDatabase")), "{:?}", report.warnings);
}

#[test]
fn xls_autofilter_malformed_payloads_never_fail_the_file() {
    // A name whose rgce is not a single area.
    let bad_name = rec(0x0018, &cat(&[&u16s(&[0x0020]), &[0, 1], &u16s(&[3, 0, 1]), &[0; 4], &[0, 0x0D], &[0x1E, 1, 0]]));
    for payload in [&[][..], &[1][..], &[1, 0][..], &[1, 0, 0][..], &[1, 0, 0, 0, 4, 0][..]] {
        let records = cat(&[&filter_info(3), &rec(0x009E, payload)]);
        let (af, _) = autofilter(&xls_file(&filter_database(1, A1_C10), &records));
        assert!(af.is_some(), "AutoFilterInfo alone still gives the range");
        let (af, _) = autofilter(&xls_file(&bad_name, &records));
        assert!(af.is_none(), "no range, no filter");
    }
    // A string condition whose record ends before its characters.
    let mut short = cat(&[&u16s(&[0, 0]), &str_doper(2, "abc"), &no_doper()]);
    short.truncate(short.len() - 1);
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &rec(0x009E, &short)])));
    assert!(af.expect("the sheet has an AutoFilter").criteria.is_empty());
    assert!(warnings.iter().any(|w| w.contains("AutoFilter criteria")), "{warnings:?}");
}

#[test]
fn xls_autofilter_bool_and_error_conditions_are_skipped() {
    let boolean = doper(0x08, 2, &[1, 0, 0, 0, 0, 0, 0, 0]);
    let record = auto_filter(0, 0, &boolean, &no_doper(), &[]);
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &cat(&[&filter_info(3), &record])));
    assert!(af.expect("the sheet has an AutoFilter").criteria.is_empty());
    assert!(warnings.iter().any(|w| w.contains("Boolean or error values")), "{warnings:?}");
}

/// AutoFilter12 (2.4.7) for column `entry`: ft, cft, cCriteria and cDateGroupings, a sheet
/// AutoFilter (fWorksheetAutoFilter, idList 0xFFFFFFFF), then `rgb`.
fn filter12(entry: u16, ft: u32, cft: u32, criteria: u32, dates: u32, rgb: &[u8]) -> Vec<u8> {
    let counts: Vec<u8> = [ft, cft, criteria, dates].iter().flat_map(|x| x.to_le_bytes()).collect();
    let header = cat(&[&u16s(&[0x087E, 1]), &[0; 8], &u16s(&[entry]), &[0; 4], &counts]);
    rec(0x087E, &cat(&[&header, &u16s(&[0x0008]), &[0; 4], &u32::MAX.to_le_bytes(), &[0; 16], rgb]))
}

/// ContinueFrt12 (2.4.62) carrying one AF12Criteria (2.5.3).
fn continue12(item: &[u8]) -> Vec<u8> {
    rec(0x087F, &cat(&[&u16s(&[0x087F, 0]), &[0; 8], item]))
}

/// AF12Criteria "equals `s`": the AFDOper, then the string.
fn equals(s: &str) -> Vec<u8> {
    cat(&[&str_doper(2, s), &[0], s.as_bytes()])
}

/// A value-list AutoFilter12 for `entry` with its items in ContinueFrt12 records.
fn value_list(entry: u16, items: &[Vec<u8>]) -> Vec<u8> {
    let mut v = filter12(entry, 0, 0, items.len() as u32, 0, &[]);
    for item in items {
        v.extend(continue12(item));
    }
    v
}

/// DXFN12NoCB with a solid fill whose background is automatic, and an XFExtNoFRT giving the
/// cell interior background as RGB `r g b` (FullColorExt, xclrType 2).
fn rgb_fill(r: u8, g: u8, b: u8) -> Vec<u8> {
    let dxfn = cat(&[&(1u32 << 29).to_le_bytes(), &u16s(&[0, 1 << 10, 0x40 << 7 | 0x40])]);
    let ext = cat(&[&u16s(&[0x0005, 20, 2, 0]), &[r, g, b, 0], &[0; 8]]);
    cat(&[&dxfn, &u16s(&[0, 0xFFFF, 0, 1]), &ext])
}

/// DXFN12NoCB with a font whose icvFore is palette index `icv`.
fn font_color(icv: u32) -> Vec<u8> {
    let mut font = [0u8; 118];
    font[80..84].copy_from_slice(&icv.to_le_bytes());
    cat(&[&(1u32 << 26).to_le_bytes(), &u16s(&[0]), &font])
}

fn criteria(records: &[&[u8]]) -> (Vec<(u32, FilterCriterion)>, Vec<String>) {
    let mut sheet = filter_info(3);
    for r in records {
        sheet.extend_from_slice(r);
    }
    let (af, warnings) = autofilter(&xls_file(&filter_database(1, A1_C10), &sheet));
    (af.expect("the sheet has an AutoFilter").criteria, warnings)
}

fn values(v: &[&str], blanks: bool) -> FilterCriterion {
    FilterCriterion::Values { values: v.iter().map(|s| s.to_string()).collect(), blanks }
}

#[test]
fn xls_autofilter12_value_list() {
    let (got, warnings) = criteria(&[&value_list(1, &[equals("North"), equals("South"), equals("West")])]);
    assert_eq!(got, vec![(1, values(&["North", "South", "West"], false))]);
    assert!(!warnings.iter().any(|w| w.contains("AutoFilter")), "{warnings:?}");
    // "Equals empty" is the (Blanks) item.
    let (got, _) = criteria(&[&value_list(0, &[equals("North"), doper(0x0C, 2, &[0; 8])])]);
    assert_eq!(got, vec![(0, values(&["North"], true))]);
    let want = from_xlsx(
        r#"<autoFilter ref="A1:C10"><filterColumn colId="0"><filters blank="1"><filter val="North"/></filters></filterColumn></autoFilter>"#,
    );
    assert_eq!(Some(got), want.map(|a| a.criteria));
}

#[test]
fn xls_autofilter12_colour_filters() {
    let (got, warnings) = criteria(&[&filter12(0, 1, 0, 0, 0, &rgb_fill(0xFF, 0, 0))]);
    assert_eq!(got, vec![(0, FilterCriterion::FillColor(Color::Rgb(0xFF0000)))], "{warnings:?}");
    let (got, warnings) = criteria(&[&filter12(2, 2, 0, 0, 0, &font_color(12))]);
    assert_eq!(got, vec![(2, FilterCriterion::FontColor(Color::Rgb(INDEXED[12])))], "{warnings:?}");
}

#[test]
fn xls_autofilter12_font_colour_uses_the_workbook_palette() {
    // Palette record: 56 entries for indexes 8 onwards; index 12 is the fifth.
    let mut palette = u16s(&[56]);
    for i in 0..56u8 {
        palette.extend_from_slice(&if i == 4 { [0x12, 0x34, 0x56, 0] } else { [0, 0, 0, 0] });
    }
    let sheet = cat(&[&filter_info(3), &filter12(0, 2, 0, 0, 0, &font_color(12))]);
    let (af, _) = autofilter(&xls_file(&cat(&[&rec(0x0092, &palette), &filter_database(1, A1_C10)]), &sheet));
    assert_eq!(af.expect("the sheet has an AutoFilter").criteria, vec![(0, FilterCriterion::FontColor(Color::Rgb(0x123456)))]);
}

#[test]
fn xls_autofilter12_icon_filter() {
    // AF12CellIcon: KPI3TRAFFICLIGHTS1 (3), icon 2.
    let (got, warnings) = criteria(&[&filter12(1, 3, 0, 0, 0, &cat(&[&3u32.to_le_bytes(), &2u32.to_le_bytes()]))]);
    assert_eq!(got, vec![(1, FilterCriterion::Icon { set: "3TrafficLights1".into(), id: Some(2) })], "{warnings:?}");
}

#[test]
fn xls_autofilter12_dynamic_filters() {
    let (got, _) = criteria(&[&filter12(0, 0, 1, 0, 0, &[]), &filter12(1, 0, 2, 0, 0, &[])]);
    assert_eq!(got, vec![(0, FilterCriterion::AboveAverage(true)), (1, FilterCriterion::AboveAverage(false))]);
    // "This month" (cft 0x0F): no criterion, one date-filter warning.
    let (got, warnings) = criteria(&[&filter12(0, 0, 0x0F, 0, 0, &[])]);
    assert!(got.is_empty());
    assert_eq!(warnings.iter().filter(|w| w.contains("date filters in AutoFilters are not supported")).count(), 1, "{warnings:?}");
}

#[test]
fn xls_autofilter12_replaces_the_autofilter_record_for_its_column() {
    let old = auto_filter(1, 0x0001, &str_doper(2, "North"), &str_doper(2, "South"), &["North", "South"]);
    let list = value_list(1, &[equals("North"), equals("South"), equals("West")]);
    let other = auto_filter(2, 0, &num_doper(4, 5.0), &no_doper(), &[]);
    let want =
        vec![(2, FilterCriterion::Custom { a: (">".into(), "5".into()), b: None, and: false }), (1, values(&["North", "South", "West"], false))];
    let (got, _) = criteria(&[&old, &list, &other]);
    assert_eq!(got, want);
    // Whichever comes first.
    let (got, _) = criteria(&[&list, &old, &other]);
    assert_eq!(got, want);
}

#[test]
fn xls_autofilter12_truncated_record_keeps_the_others() {
    let good = auto_filter(2, 0, &num_doper(4, 5.0), &no_doper(), &[]);
    let mut short = filter12(0, 1, 0, 0, 0, &rgb_fill(0xFF, 0, 0));
    short.truncate(short.len() - 10);
    let len = (short.len() - 4) as u16;
    short[2..4].copy_from_slice(&len.to_le_bytes());
    let header_only = rec(0x087E, &[0; 40]);
    // A value list missing one of its ContinueFrt12 records.
    let missing = filter12(1, 0, 0, 3, 0, &[]);
    let missing = cat(&[&missing, &continue12(&equals("North"))]);
    for damaged in [&short, &header_only, &missing] {
        let (got, warnings) = criteria(&[damaged, &good]);
        assert_eq!(got, vec![(2, FilterCriterion::Custom { a: (">".into(), "5".into()), b: None, and: false })]);
        assert_eq!(warnings.iter().filter(|w| w.contains("could not be read")).count(), 1, "{warnings:?}");
    }
}

#[test]
fn xls_autofilter12_saved_as_xlsx_keeps_its_criteria() {
    let sheet = cat(&[
        &filter_info(3),
        &value_list(0, &[equals("North"), equals("South"), equals("West")]),
        &filter12(1, 1, 0, 0, 0, &rgb_fill(0xFF, 0, 0)),
        &filter12(2, 2, 0, 0, 0, &font_color(12)),
    ]);
    let (wb, _) = read_xls(&xls_file(&filter_database(1, A1_C10), &sheet)).unwrap();
    assert_eq!(wb.sheets[0].autofilter.as_ref().map(|a| a.criteria.len()), Some(3));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].autofilter, wb.sheets[0].autofilter);
}
