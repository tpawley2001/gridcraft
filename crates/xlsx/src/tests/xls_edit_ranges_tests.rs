//! Synthetic Feat / FeatProtection records ([MS-XLS] 2.4.111, 2.5.124).
use super::xls_protection_tests::{read, rec};
use crate::{read_xlsx, write_xlsx};
use gridcraft_core::{CellRef, RangeRef, Value};
use gridcraft_model::{EditRange, PasswordHash, Workbook, password::legacy_hash};

fn feat(isf: u16, refs: &[[u16; 4]], password: u16, sd: bool) -> Vec<u8> {
    let mut out = vec![0; 12];
    out[..2].copy_from_slice(&0x0868u16.to_le_bytes());
    out.extend(isf.to_le_bytes());
    out.extend([0; 5]);
    out.extend((refs.len() as u16).to_le_bytes());
    out.extend([0; 6]); // cbFeatData is zero for protection, then reserved3.
    for reference in refs {
        for value in reference {
            out.extend(value.to_le_bytes());
        }
    }
    out.extend(u32::from(sd).to_le_bytes());
    out.extend(u32::from(password).to_le_bytes());
    out.extend(6u16.to_le_bytes());
    out.push(0);
    out.extend(b"Budget");
    if sd {
        out.extend(20u32.to_le_bytes());
        out.extend([0; 20]);
    }
    out
}

fn roundtrip(wb: &Workbook) {
    let (back, _) = read_xlsx(&write_xlsx(wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].edit_ranges, wb.sheets[0].edit_ranges);
}

#[test]
fn xls_edit_ranges_passwords_references_and_permissions() {
    let hash = legacy_hash("abc");
    assert!(PasswordHash::Legacy(hash).verify("abc").unwrap());
    for password in [0, hash] {
        for refs in [vec![[1, 4, 1, 2]], vec![[1, 4, 1, 2], [0, 8, 4, 4]]] {
            for sd in [false, true] {
                let range = [rec(0x0012, &1u16.to_le_bytes()), rec(0x0868, &feat(2, &refs, password, sd))].concat();
                // Two sheets exercise workbook-wide warning deduplication.
                let (wb, report) = read(&[], &[range.clone(), range]);
                let ranges = refs
                    .iter()
                    .map(|[r0, r1, c0, c1]| RangeRef::new(CellRef::new(u32::from(*r0), u32::from(*c0)), CellRef::new(u32::from(*r1), u32::from(*c1))))
                    .collect();
                assert_eq!(
                    wb.sheets[0].edit_ranges,
                    vec![EditRange {
                        title: "Budget".into(),
                        ranges,
                        password: (password != 0).then_some(PasswordHash::Legacy(password)),
                        security_descriptor: None,
                    }]
                );
                assert_eq!(report.warnings.iter().filter(|w| w.contains("Windows user permissions")).count(), usize::from(sd));
                roundtrip(&wb);
            }
        }
    }
}

#[test]
fn xls_edit_ranges_ignore_other_features_and_empty_references() {
    for payload in [feat(3, &[[1, 4, 1, 2]], 0, false), feat(2, &[], 0, false)] {
        let (wb, report) = read(&[], &[rec(0x0868, &payload)]);
        assert!(wb.sheets[0].edit_ranges.is_empty());
        assert!(!report.warnings.iter().any(|w| w.contains("Allow Edit Ranges")));
        roundtrip(&wb);
    }
}

#[test]
fn xls_edit_ranges_truncation_is_atomic_and_cells_still_load() {
    let payload = feat(2, &[[1, 4, 1, 2]], legacy_hash("abc"), true);
    let number = rec(0x0203, &[&[0u8; 6][..], &42f64.to_le_bytes()].concat());
    for len in 0..payload.len() {
        let (wb, report) = read(&[], &[[rec(0x0868, &payload[..len]), number.clone()].concat()]);
        assert!(wb.sheets[0].edit_ranges.is_empty(), "length {len}");
        assert_eq!(report.warnings.iter().filter(|w| w.contains("Allow Edit Ranges")).count(), 1, "{:?}", report.warnings);
        assert_eq!(wb.sheets[0].value(CellRef::new(0, 0)), Value::number(42.0));
        roundtrip(&wb);
    }
}
