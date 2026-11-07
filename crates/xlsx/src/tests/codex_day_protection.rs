//! Synthetic XLSX acceptance tests; no vendor-produced fixtures.
use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};

fn part(bytes: &[u8], name: &str) -> El {
    parse(&Package::open(bytes).unwrap().read(name).unwrap().unwrap()).unwrap()
}
fn save(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}
fn twice(bytes: &[u8], name: &str) -> El {
    part(&save(&save(bytes)), name)
}

fn workbook(body: &str) -> El {
    twice(&minimal("<sheetData/>", &[], "", body), "xl/workbook.xml")
}
#[test]
fn window_only_lock_survives_without_becoming_structure_lock() {
    let x = workbook(r#"<workbookProtection lockWindows="true" workbookPassword="CBEB"/>"#);
    let p = x.child("workbookProtection").unwrap();
    assert_eq!(p.attr_bool("lockWindows"), Some(true));
    assert!(!p.flag("lockStructure", false));
    assert_eq!(u16::from_str_radix(p.attr("workbookPassword").unwrap(), 16).unwrap(), 0xCBEB);
    assert_eq!(x.kids("workbookProtection").count(), 1);
}
#[test]
fn revision_lock_and_its_password_are_not_workbook_password() {
    let x = workbook(r#"<workbookProtection lockRevision="1" revisionsPassword="1234"/>"#);
    let p = x.child("workbookProtection").unwrap();
    assert_eq!(p.attr_bool("lockRevision"), Some(true));
    assert_eq!(u16::from_str_radix(p.attr("revisionsPassword").unwrap(), 16).unwrap(), 0x1234);
    assert!(p.attr("workbookPassword").is_none());
}
#[test]
fn false_locks_keep_both_passwords_without_activating_them() {
    let x = workbook(r#"<workbookProtection lockStructure="0" lockWindows="0" lockRevision="0" workbookPassword="CBEB" revisionsPassword="1234"/>"#);
    let p = x.child("workbookProtection").unwrap();
    for k in ["lockStructure", "lockWindows", "lockRevision"] {
        assert!(!p.flag(k, false));
    }
    assert_eq!(u16::from_str_radix(p.attr("workbookPassword").unwrap(), 16).unwrap(), 0xCBEB);
    assert_eq!(u16::from_str_radix(p.attr("revisionsPassword").unwrap(), 16).unwrap(), 0x1234);
}
#[test]
fn independent_iso_groups_survive_twice() {
    let x = workbook(
        r#"<workbookProtection lockWindows="1" lockRevision="1" workbookAlgorithmName="SHA-512" workbookHashValue="AQIDBA==" workbookSaltValue="BQYHCA==" workbookSpinCount="1000" revisionsAlgorithmName="SHA-256" revisionsHashValue="CQoLDA==" revisionsSaltValue="DQ4PEA==" revisionsSpinCount="2000"/>"#,
    );
    let p = x.child("workbookProtection").unwrap();
    for (k, v) in [
        ("workbookAlgorithmName", "SHA-512"),
        ("workbookHashValue", "AQIDBA=="),
        ("workbookSaltValue", "BQYHCA=="),
        ("workbookSpinCount", "1000"),
        ("revisionsAlgorithmName", "SHA-256"),
        ("revisionsHashValue", "CQoLDA=="),
        ("revisionsSaltValue", "DQ4PEA=="),
        ("revisionsSpinCount", "2000"),
    ] {
        assert_eq!(p.attr(k), Some(v), "{k}");
    }
}
#[test]
fn absent_protection_stays_absent() {
    assert!(workbook("").child("workbookProtection").is_none());
}
