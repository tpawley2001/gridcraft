//! Synthetic BrtRangeProtection / BrtRangeProtectionIso ([MS-XLSB] 2.4.763 / 765).
#[path = "xlsb_protection_fixture.rs"]
mod fixture;
use crate::{read_xlsb, read_xlsx, write_xlsx};
use fixture::{file, record, wide};
use gridcraft_core::{CellRef, RangeRef};
use gridcraft_model::{EditRange, PasswordHash, Workbook, password::legacy_hash};

fn payload(iso: bool, password: u16, refs: &[[u32; 4]], sd: bool) -> Vec<u8> {
    let mut out = if iso { 100_000u32.to_le_bytes().to_vec() } else { password.to_le_bytes().to_vec() };
    out.extend((refs.len() as u32).to_le_bytes());
    for reference in refs {
        for value in reference {
            out.extend(value.to_le_bytes());
        }
    }
    out.extend(wide("Budget"));
    out.extend((if sd { 20u32 } else { 0 }).to_le_bytes());
    if sd {
        out.extend([0; 20]);
    }
    if iso {
        out.extend(64u32.to_le_bytes());
        out.extend([0; 64]);
        out.extend(16u32.to_le_bytes());
        out.extend([1; 16]);
        out.extend(wide("SHA-512"));
    }
    out
}

fn roundtrip(wb: &Workbook) {
    let (back, _) = read_xlsx(&write_xlsx(wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].edit_ranges, wb.sheets[0].edit_ranges);
}

#[test]
fn xlsb_edit_ranges_passwords_references_and_permissions() {
    let hash = legacy_hash("abc");
    assert!(PasswordHash::Legacy(hash).verify("abc").unwrap());
    for (id, password) in [(536, 0), (536, hash), (680, 0)] {
        for refs in [vec![[1, 4, 1, 2]], vec![[1, 4, 1, 2], [0, 8, 4, 4]]] {
            for sd in [false, true] {
                let mut records = record(535, &fixture::sheet_payload(0, [1; 16]));
                records.extend(record(id, &payload(id == 680, password, &refs, sd)));
                if id == 680 {
                    records.extend(record(536, &payload(false, 0, &refs, sd)));
                }
                let (wb, report) = read_xlsb(&file(&[], &records)).unwrap();
                let expected_password = if id == 680 {
                    Some(PasswordHash::Iterated {
                        algorithm: "SHA-512".into(),
                        spin_count: 100_000,
                        hash: gridcraft_model::encode_base64(&[0; 64]),
                        salt: gridcraft_model::encode_base64(&[1; 16]),
                    })
                } else {
                    (password != 0).then_some(PasswordHash::Legacy(password))
                };
                assert_eq!(
                    wb.sheets[0].edit_ranges,
                    vec![EditRange {
                        title: "Budget".into(),
                        ranges: refs.iter().map(|[r0, r1, c0, c1]| RangeRef::new(CellRef::new(*r0, *c0), CellRef::new(*r1, *c1))).collect(),
                        password: expected_password,
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
fn xlsb_edit_ranges_malformed_records_are_atomic() {
    for id in [536, 680] {
        let complete = payload(id == 680, legacy_hash("abc"), &[[1, 4, 1, 2]], true);
        for len in 0..complete.len() {
            let records = [record(id, &complete[..len]), record(id, &complete[..len])].concat();
            let (wb, report) = read_xlsb(&file(&[], &records)).unwrap();
            assert!(wb.sheets[0].edit_ranges.is_empty(), "record {id}, length {len}");
            assert_eq!(report.warnings.iter().filter(|w| w.contains("Allow Edit Ranges")).count(), 1);
            roundtrip(&wb);
        }
    }
    for refs in [vec![], vec![[5, 1, 0, 0]], vec![[0, 0, 0, 16384]]] {
        let (wb, report) = read_xlsb(&file(&[], &record(536, &payload(false, 0, &refs, false)))).unwrap();
        assert!(wb.sheets[0].edit_ranges.is_empty());
        assert_eq!(report.warnings.iter().filter(|w| w.contains("Allow Edit Ranges")).count(), 1);
        roundtrip(&wb);
    }
}
