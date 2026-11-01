//! Synthetic BIFF8 protection records, based on [MS-XLS] 2.4.112, 2.4.191,
//! 2.4.207 and 2.5.104.
use gridcraft_model::{PasswordHash, SheetProtection};
use std::io::{Cursor, Read};

use super::xls_tests::compound;
use crate::{ReadReport, read_xls, read_xlsx, write_xlsx};

fn rec(id: u16, data: &[u8]) -> Vec<u8> {
    [id.to_le_bytes().as_slice(), &(data.len() as u16).to_le_bytes(), data].concat()
}

fn bof(kind: u16) -> Vec<u8> {
    rec(0x0809, &[0x0600u16.to_le_bytes().as_slice(), &kind.to_le_bytes(), &[0; 12]].concat())
}

fn word(id: u16, value: u16) -> Vec<u8> {
    rec(id, &value.to_le_bytes())
}

fn feat(isf: u16, presence: u32, data: &[u8]) -> Vec<u8> {
    let mut header = vec![0; 12];
    header[..2].copy_from_slice(&0x0867u16.to_le_bytes());
    rec(0x0867, &[header.as_slice(), &isf.to_le_bytes(), &[1], &presence.to_le_bytes(), data].concat())
}

fn read(globals: &[u8], sheets: &[Vec<u8>]) -> (gridcraft_model::Workbook, ReadReport) {
    let boundsheet = |offset: u32, index: usize| {
        let name = format!("Sheet{index}");
        rec(0x0085, &[offset.to_le_bytes().as_slice(), &[0, 0, name.len() as u8, 0], name.as_bytes()].concat())
    };
    let mut prefix = bof(5);
    prefix.extend(globals);
    let mut offset = prefix.len() + sheets.iter().enumerate().map(|(i, _)| boundsheet(0, i).len()).sum::<usize>() + 4;
    for (i, sheet) in sheets.iter().enumerate() {
        prefix.extend(boundsheet(offset as u32, i));
        offset += 20 + sheet.len() + 4;
    }
    prefix.extend(rec(0x000A, &[]));
    for sheet in sheets {
        prefix.extend(bof(0x0010));
        prefix.extend(sheet);
        prefix.extend(rec(0x000A, &[]));
    }
    read_xls(&compound("Workbook", &prefix)).unwrap()
}

fn defaults(password: Option<PasswordHash>) -> SheetProtection {
    SheetProtection { password, select_locked: true, select_unlocked: true, ..SheetProtection::default() }
}

#[test]
fn sheet_password_and_default_actions() {
    // Password can precede Protect; unrelated protection records have no effect.
    let records = [word(0x0013, 0xCC1A), word(0x0063, 1), word(0x00DD, 1), word(0x0012, 1)].concat();
    let (wb, _) = read(&[], &[records, vec![]]);
    assert_eq!(wb.sheets[0].protection, Some(defaults(Some(PasswordHash::Legacy(0xCC1A)))));
    assert_eq!(wb.sheets[1].protection, None);
    assert!(!wb.protected_structure);
    assert_eq!(wb.structure_password, None);
}

#[test]
fn enhanced_actions_before_or_after_protect() {
    // MS-XLS: cbHdrData is a presence sentinel, not a byte count.
    let enhanced = feat(2, u32::MAX, &0x4C04u32.to_le_bytes());
    for records in [[word(0x0012, 1), enhanced.clone()].concat(), [enhanced, word(0x0012, 1)].concat()] {
        let (wb, _) = read(&[], &[records]);
        assert_eq!(wb.sheets[0].protection, Some(SheetProtection { format_cells: true, sort: true, ..defaults(None) }));
    }
}

#[test]
fn absent_enhanced_data_uses_defaults() {
    for enhanced in [feat(2, 0, &[]), feat(4, 0, &[])] {
        let (wb, _) = read(&[], &[[word(0x0012, 1), enhanced, word(0x0013, 0)].concat()]);
        assert_eq!(wb.sheets[0].protection, Some(defaults(None)));
    }
}

#[test]
fn every_modelled_action_uses_its_own_bit() {
    for bit in 0..32 {
        let (wb, _) = read(&[], &[[word(0x0012, 1), feat(2, u32::MAX, &(1u32 << bit).to_le_bytes())].concat()]);
        let p = wb.sheets[0].protection.as_ref().unwrap();
        assert_eq!(
            [
                p.format_cells,
                p.format_columns,
                p.format_rows,
                p.insert_columns,
                p.insert_rows,
                p.delete_columns,
                p.delete_rows,
                p.select_locked,
                p.sort,
                p.autofilter,
                p.select_unlocked
            ],
            [2, 3, 4, 5, 6, 8, 9, 10, 11, 12, 14].map(|action| action == bit)
        );
    }
}

#[test]
fn unprotected_sheets_ignore_password_and_actions() {
    for protect in [vec![], word(0x0012, 0)] {
        let (wb, _) = read(&[], &[[protect, word(0x0013, 0xCC1A), feat(2, u32::MAX, &u32::MAX.to_le_bytes())].concat()]);
        assert_eq!(wb.sheets[0].protection, None);
    }
}

#[test]
fn workbook_structure_and_password_are_separate_from_sheets() {
    let (wb, _) = read(&[word(0x0013, 0x83AF), word(0x0012, 1), word(0x0019, 1)].concat(), &[vec![]]);
    assert!(wb.protected_structure);
    assert_eq!(wb.structure_password, Some(PasswordHash::Legacy(0x83AF)));
    assert_eq!(wb.sheets[0].protection, None);
    for records in [vec![], [word(0x0012, 0), word(0x0013, 0)].concat()] {
        let (wb, _) = read(&records, &[vec![]]);
        assert!(!wb.protected_structure);
        assert_eq!(wb.structure_password, None);
    }
}

#[test]
fn malformed_protection_warns_and_keeps_defaults() {
    let complete = feat(2, u32::MAX, &0x4C04u32.to_le_bytes());
    for len in 0..23 {
        let truncated = rec(0x0867, &complete[4..4 + len]);
        let (wb, report) = read(&[], &[[word(0x0012, 1), truncated].concat()]);
        assert_eq!(wb.sheets[0].protection, Some(defaults(None)));
        assert_eq!(report.warnings.iter().filter(|w| w.contains("FeatHdr")).count(), 1, "{:?}", report.warnings);
    }
    for id in [0x0012, 0x0013] {
        let (wb, report) = read(&rec(id, &[1]), &[[word(0x0012, 1), rec(id, &[1])].concat()]);
        assert!(!wb.protected_structure);
        assert_eq!(wb.structure_password, None);
        assert_eq!(wb.sheets[0].protection, Some(defaults(None)));
        // ReadReport deduplicates identical warnings across substreams.
        assert_eq!(report.warnings.len(), 2);
    }
}

#[test]
fn protection_survives_xlsx_save() {
    let (wb, _) = read(&[word(0x0012, 1), word(0x0013, 0x83AF)].concat(), &[[word(0x0012, 1), word(0x0013, 0xCC1A)].concat()]);
    let bytes = write_xlsx(&wb).unwrap();
    let mut zip = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut sheet = String::new();
    zip.by_name("xl/worksheets/sheet1.xml").unwrap().read_to_string(&mut sheet).unwrap();
    assert!(sheet.contains("<sheetProtection"));
    assert!(sheet.contains("password=\"CC1A\""));
    let mut workbook = String::new();
    zip.by_name("xl/workbook.xml").unwrap().read_to_string(&mut workbook).unwrap();
    assert!(workbook.contains("<workbookProtection"));
    assert!(workbook.contains("workbookPassword=\"83AF\""));
    let (back, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(back.sheets[0].protection, wb.sheets[0].protection);
    assert_eq!(back.structure_password, wb.structure_password);
    assert!(back.protected_structure);
}
