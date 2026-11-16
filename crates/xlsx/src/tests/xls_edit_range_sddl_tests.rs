//! Allow Edit Ranges permissions from `.xls` (FeatProtection SDContainer, [MS-XLS] 2.5.124 / 2.5.234) and
//! `.xlsb` (RangeProtectionTitleSDRel, [MS-XLSB] 2.5.113) are binary self-relative security descriptors
//! ([MS-DTYP] 2.4.6); `.xlsx` keeps them as SDDL text ([MS-DTYP] 2.5.1) in `protectedRange@securityDescriptor`.
use super::xls_protection_tests::{read, rec};
use super::xlsb_protection_tests::fixture;
use crate::{read_xlsb, read_xlsx, write_xlsx};
use fixture::{file, record, wide};
use gridcraft_model::Workbook;

fn sid(authority: u64, subs: &[u32]) -> Vec<u8> {
    let mut out = vec![1, subs.len() as u8];
    out.extend(&authority.to_be_bytes()[2..]);
    for s in subs {
        out.extend(s.to_le_bytes());
    }
    out
}

/// (AceType, AceFlags, Mask, SID)
type Ace = (u8, u8, u32, Vec<u8>);

fn acl(aces: &[Ace]) -> Vec<u8> {
    let mut body = Vec::new();
    for (t, f, mask, s) in aces {
        body.push(*t);
        body.push(*f);
        body.extend(((8 + s.len()) as u16).to_le_bytes());
        body.extend(mask.to_le_bytes());
        body.extend(s);
    }
    let mut out = vec![2, 0];
    out.extend(((8 + body.len()) as u16).to_le_bytes());
    out.extend((aces.len() as u16).to_le_bytes());
    out.extend([0, 0]);
    out.extend(body);
    out
}

/// Self-relative descriptor: header, owner, group, DACL.
fn descriptor(owner: Option<Vec<u8>>, group: Option<Vec<u8>>, extra_control: u16, dacl: Option<Vec<u8>>) -> Vec<u8> {
    let mut tail: Vec<u8> = Vec::new();
    let mut place = |part: &Option<Vec<u8>>| -> u32 {
        match part {
            Some(p) => {
                let off = 20 + tail.len() as u32;
                tail.extend(p);
                off
            }
            None => 0,
        }
    };
    let (o, g, d) = (place(&owner), place(&group), place(&dacl));
    let control = 0x8000 | extra_control | if dacl.is_some() { 0x0004 } else { 0 };
    let mut out = vec![1, 0];
    out.extend(control.to_le_bytes());
    for v in [o, g, 0, d] {
        out.extend(v.to_le_bytes());
    }
    out.extend(tail);
    out
}

fn everyone() -> Vec<u8> {
    sid(1, &[0])
}

/// Feat (0x0868) with FeatProtection: one range B2:C5, no password, the given SD.
fn feat(sd: &[u8]) -> Vec<u8> {
    let mut out = vec![0; 12];
    out[..2].copy_from_slice(&0x0868u16.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend([0; 5]);
    out.extend(1u16.to_le_bytes());
    out.extend([0; 6]);
    for v in [1u16, 4, 1, 2] {
        out.extend(v.to_le_bytes());
    }
    out.extend(1u32.to_le_bytes()); // fSD
    out.extend(0u32.to_le_bytes()); // no password
    out.extend(6u16.to_le_bytes());
    out.push(0);
    out.extend(b"Budget");
    out.extend((sd.len() as u32).to_le_bytes());
    out.extend(sd);
    out
}

fn xls_sddl(sd: &[u8]) -> (Option<String>, usize) {
    let range = [rec(0x0012, &1u16.to_le_bytes()), rec(0x0868, &feat(sd))].concat();
    let (wb, report) = read(&[], &[range]);
    let warned = report.warnings.iter().filter(|w| w.contains("Windows user permissions")).count();
    assert_eq!(wb.sheets[0].edit_ranges.len(), 1, "the edit range itself is always kept");
    roundtrip(&wb);
    (wb.sheets[0].edit_ranges[0].security_descriptor.clone(), warned)
}

fn roundtrip(wb: &Workbook) {
    let (back, _) = read_xlsx(&write_xlsx(wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].edit_ranges, wb.sheets[0].edit_ranges);
}

#[test]
fn excel_allow_entry_matches_excel_xlsx_text() {
    let user = sid(5, &[21, 2006190760, 3459553193, 1651965558, 540628]);
    let sd = descriptor(Some(everyone()), Some(everyone()), 0, Some(acl(&[(0, 0, 0x1, user)])));
    assert_eq!(xls_sddl(&sd), (Some("O:WDG:WDD:(A;;CC;;;S-1-5-21-2006190760-3459553193-1651965558-540628)".into()), 0));
}

#[test]
fn excel_deny_entry_matches_excel_xlsx_text() {
    let user = sid(5, &[21, 2328606873, 448790680, 2149751550, 1003]);
    let sd = descriptor(Some(everyone()), Some(everyone()), 0, Some(acl(&[(1, 0, 0x1, user)])));
    assert_eq!(xls_sddl(&sd), (Some("O:WDG:WDD:(D;;CC;;;S-1-5-21-2328606873-448790680-2149751550-1003)".into()), 0));
}

#[test]
fn aliases_flags_rights_and_protected_dacl() {
    let admins = sid(5, &[32, 544]);
    let users = sid(5, &[32, 545]);
    let other = sid(5, &[21, 1, 2, 3, 1000]);
    let aces = [(0, 0, 0x1, sid(5, &[18])), (0, 0x03, 0x3, admins.clone()), (1, 0x10, 0x0010_0001, other)];
    let sd = descriptor(Some(admins), Some(users), 0x1000, Some(acl(&aces)));
    assert_eq!(xls_sddl(&sd), (Some("O:BAG:BUD:P(A;;CC;;;SY)(A;OICI;CCDC;;;BA)(D;ID;0x100001;;;S-1-5-21-1-2-3-1000)".into()), 0));
}

#[test]
fn no_owner_or_group_writes_only_the_dacl() {
    let sd = descriptor(None, None, 0, Some(acl(&[(0, 0, 0x1, everyone())])));
    assert_eq!(xls_sddl(&sd), (Some("D:(A;;CC;;;WD)".into()), 0));
}

#[test]
fn unsupported_or_broken_descriptors_still_warn() {
    // Object ACE (type 5) is not converted.
    let object = descriptor(Some(everyone()), Some(everyone()), 0, Some(acl(&[(5, 0, 0x1, everyone())])));
    // Revision 0.
    let mut bad_revision = descriptor(Some(everyone()), None, 0, None);
    bad_revision[0] = 0;
    // Owner offset past the end.
    let mut bad_offset = descriptor(Some(everyone()), None, 0, None);
    bad_offset[4..8].copy_from_slice(&200u32.to_le_bytes());
    for sd in [object, bad_revision, bad_offset] {
        assert_eq!(xls_sddl(&sd), (None, 1), "{sd:?}");
    }
}

#[test]
fn xlsb_range_protection_keeps_the_descriptor() {
    let user = sid(5, &[21, 2006190760, 3459553193, 1651965558, 540628]);
    let sd = descriptor(Some(everyone()), Some(everyone()), 0, Some(acl(&[(0, 0, 0x1, user)])));
    // BrtRangeProtection (536): legacy password 0, one UncheckedRfX, title, cbSD + SD.
    let mut payload = 0u16.to_le_bytes().to_vec();
    payload.extend(1u32.to_le_bytes());
    for v in [1u32, 4, 1, 2] {
        payload.extend(v.to_le_bytes());
    }
    payload.extend(wide("Budget"));
    payload.extend((sd.len() as u32).to_le_bytes());
    payload.extend(&sd);
    let mut records = record(535, &fixture::sheet_payload(0, [1; 16]));
    records.extend(record(536, &payload));
    let (wb, report) = read_xlsb(&file(&[], &records)).unwrap();
    assert_eq!(
        wb.sheets[0].edit_ranges[0].security_descriptor.as_deref(),
        Some("O:WDG:WDD:(A;;CC;;;S-1-5-21-2006190760-3459553193-1651965558-540628)")
    );
    assert!(!report.warnings.iter().any(|w| w.contains("Windows user permissions")));
    roundtrip(&wb);
}
