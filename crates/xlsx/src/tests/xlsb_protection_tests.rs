use crate::{read_xlsb, read_xlsx, write_xlsx};
use gridcraft_model::{CalcMode, PasswordHash};
#[path = "xlsb_protection_fixture.rs"]
mod fixture;
use fixture::{file, record, sheet_payload, wide};

fn iso_data(hash: &[u8], salt: &[u8], algorithm: &str) -> Vec<u8> {
    let mut out = (hash.len() as u32).to_le_bytes().to_vec();
    out.extend(hash);
    out.extend((salt.len() as u32).to_le_bytes());
    out.extend(salt);
    out.extend(wide(algorithm));
    out
}
fn expected_iso() -> PasswordHash {
    PasswordHash::Iterated {
        algorithm: "SHA-512".into(),
        spin_count: 100_000,
        hash: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".into(),
        salt: "AQEBAQEBAQEBAQEBAQEBAQ==".into(),
    }
}
fn sheet_iso() -> Vec<u8> {
    let mut payload = 100_000u32.to_le_bytes().to_vec();
    payload.extend(sheet_payload(0, [1; 16]).into_iter().skip(2));
    payload.extend(iso_data(&[0; 64], &[1; 16], "SHA-512"));
    record(678, &payload)
}

#[test]
fn xlsb_sheet_legacy_permissions() {
    let mut flags = [1; 16];
    flags[3] = 0;
    flags[12] = 0;
    let (wb, _) = read_xlsb(&file(&[], &record(535, &sheet_payload(0xCC1A, flags)))).unwrap();
    let p = wb.sheets[0].protection.as_ref().unwrap();
    assert_eq!(p.password, Some(PasswordHash::Legacy(0xCC1A)));
    assert!(p.format_cells && p.sort);
    assert!(!p.select_locked && !p.insert_rows);
    let (again, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(again.sheets[0].protection, wb.sheets[0].protection);
}
#[test]
fn xlsb_sheet_selection_and_absent_protection() {
    let mut flags = [0; 16];
    flags[0] = 1;
    let (wb, _) = read_xlsb(&file(&[], &record(535, &sheet_payload(0, flags)))).unwrap();
    let p = wb.sheets[0].protection.as_ref().unwrap();
    assert!(p.select_locked && p.select_unlocked);
    assert_eq!(p.password, None);
    for tail in [vec![], record(535, &sheet_payload(123, [0; 16]))] {
        let (wb, _) = read_xlsb(&file(&[], &tail)).unwrap();
        assert_eq!(wb.sheets[0].protection, None);
    }
}
#[test]
fn xlsb_sheet_iso_wins_in_either_order() {
    let legacy = record(535, &sheet_payload(0, [1; 16]));
    for records in [sheet_iso(), [sheet_iso(), legacy.clone()].concat(), [legacy, sheet_iso()].concat()] {
        let (wb, _) = read_xlsb(&file(&[], &records)).unwrap();
        assert_eq!(wb.sheets[0].protection.as_ref().unwrap().password, Some(expected_iso()));
    }
}
#[test]
fn xlsb_book_legacy_and_iso() {
    let legacy = record(534, &[0xAF, 0x83, 0, 0, 1, 0]);
    let (wb, _) = read_xlsb(&file(&legacy, &[])).unwrap();
    assert!(wb.protected_structure);
    assert_eq!(wb.structure_password, Some(PasswordHash::Legacy(0x83AF)));
    let mut iso = 100_000u32.to_le_bytes().to_vec();
    iso.extend(0u32.to_le_bytes());
    iso.extend(1u16.to_le_bytes());
    iso.extend(iso_data(&[0; 64], &[1; 16], "SHA-512"));
    iso.extend(iso_data(&[], &[], ""));
    for records in [[record(677, &iso), legacy.clone()].concat(), [legacy, record(677, &iso)].concat()] {
        let (wb, _) = read_xlsb(&file(&records, &[])).unwrap();
        assert!(wb.protected_structure);
        assert_eq!(wb.structure_password, Some(expected_iso()));
    }
}
fn calc(mode: u32, count: u32, delta: f64, flags: u16) -> Vec<u8> {
    let mut payload = 0u32.to_le_bytes().to_vec();
    payload.extend(mode.to_le_bytes());
    payload.extend(count.to_le_bytes());
    payload.extend(delta.to_le_bytes());
    payload.extend(1u32.to_le_bytes()); // cUserThreadCount, followed by the flags.
    payload.extend(flags.to_le_bytes());
    record(157, &payload)
}
#[test]
fn xlsb_calculation_options() {
    let (wb, _) = read_xlsb(&file(&calc(0, 50, 0.01, 4), &[])).unwrap();
    assert_eq!(wb.calc.mode, CalcMode::Manual);
    assert_eq!(wb.calc.max_iterations, 50);
    assert_eq!(wb.calc.max_change, 0.01);
    assert!(wb.calc.iterative && wb.calc.precision_as_displayed);
    for (mode, expected) in [(1, CalcMode::Automatic), (2, CalcMode::AutomaticExceptTables)] {
        let (wb, _) = read_xlsb(&file(&calc(mode, 100, 0.001, 8), &[])).unwrap();
        assert_eq!(wb.calc.mode, expected);
        assert!(!wb.calc.iterative && !wb.calc.precision_as_displayed);
    }
}
#[test]
fn xlsb_malformed_protection_is_skipped_atomically() {
    let baseline = read_xlsb(&file(&[], &[])).unwrap().1.warnings.len();
    for payload in [vec![], sheet_payload(0xCC1A, [1; 16])[..65].to_vec()] {
        let (wb, report) = read_xlsb(&file(&[], &record(535, &payload))).unwrap();
        assert_eq!(wb.sheets[0].protection, None);
        assert_eq!(report.warnings.len(), baseline + 1);
    }
    for (id, payload) in [(534, vec![1]), (677, vec![0; 10]), (157, vec![0; 25])] {
        let (wb, report) = read_xlsb(&file(&record(id, &payload), &[])).unwrap();
        assert!(!wb.protected_structure);
        assert_eq!(wb.calc, Default::default());
        assert_eq!(report.warnings.len(), baseline + 1);
    }
    let (wb, report) = read_xlsb(&file(&[], &record(678, &[0; 68]))).unwrap();
    assert_eq!(wb.sheets[0].protection, None);
    assert_eq!(report.warnings.len(), baseline + 1);
}
