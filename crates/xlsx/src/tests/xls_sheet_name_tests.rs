//! Synthetic BIFF8 names and BOF compatibility regressions.
use super::xls_tests::{bof, cat, compound, eof, rec, u16s};
use crate::read_xls;

fn workbook(names: &[&str], version: u16) -> Vec<u8> {
    let boundsheet = |pos: u32, name: &str| {
        let text: Vec<_> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        rec(0x0085, &cat(&[&pos.to_le_bytes(), &[0, 0, name.chars().count() as u8, 1], &text]))
    };
    let sheets = cat(&[&rec(0x0809, &u16s(&[version, 0x0010, 0, 0])), &eof()]);
    let head = bof(0x0005);
    let mut pos = head.len() + names.iter().map(|n| boundsheet(0, n).len()).sum::<usize>() + eof().len();
    let mut out = head;
    for name in names {
        out.extend(boundsheet(pos as u32, name));
        pos += sheets.len();
    }
    out.extend(eof());
    for _ in names {
        out.extend(&sheets);
    }
    compound("Workbook", &out)
}

#[test]
fn biff8_repairs_invalid_and_duplicate_names_with_one_warning() {
    let long = "界".repeat(40);
    let (wb, report) = read_xls(&workbook(&["A:/?*[]\\B", "ab", "", &long, &long], 0x0600)).unwrap();
    assert_eq!(
        wb.sheets.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
        ["AB", "ab (2)", "Sheet3", &"界".repeat(31), &format!("{} (2)", "界".repeat(25))]
    );
    assert_eq!(report.warnings.iter().filter(|w| w.contains("sheet name")).count(), 1);
}

#[test]
fn biff8_globals_still_reject_biff5_sheet_bofs() {
    let error = read_xls(&workbook(&["Data"], 0x0500)).unwrap_err();
    assert!(error.to_string().contains("no worksheets that can be read"));
}
