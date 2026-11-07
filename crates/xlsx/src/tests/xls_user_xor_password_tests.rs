//! Legacy `.xls` workbooks obfuscated with XOR ([MS-XLS] 2.2.10, [MS-OFFCRYPTO] 2.3.7 method 1)
//! under a user password ask for it, open with it and refuse a wrong one, BIFF8 and BIFF5.

use super::xls_tests::{compound, u16s, v};
use super::xls_xor_password_tests::{biff5_book, biff8_workbook, filepass8, obfuscate, verifier, xor_key};
use crate::{DecryptError, XlsOpenError, read_xls_with_password};
use gridcraft_core::Value;

const PASSWORD: &str = "hunter2";

fn biff8(pw: &str) -> Vec<u8> {
    compound("Workbook", &obfuscate(&biff8_workbook(&filepass8(pw), 160), pw))
}

fn biff5(pw: &str) -> Vec<u8> {
    compound("Book", &obfuscate(&biff5_book(&u16s(&[xor_key(pw.as_bytes()), verifier(pw.as_bytes())])), pw))
}

#[test]
fn spec_key_matches_the_reader() {
    for pw in ["a", PASSWORD, "VelvetSweatshop", "fifteen-chars!!"] {
        assert_eq!(crate::xls_crypto::xor_key(pw.as_bytes()), Some(xor_key(pw.as_bytes())), "{pw}");
    }
}

#[test]
fn xor_user_password_opens_biff8() {
    let (wb, report) = read_xls_with_password(&biff8(PASSWORD), Some(PASSWORD)).unwrap_or_else(|e| panic!("refused: {e}"));
    assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
    assert_eq!(v(&wb, 0, "A2"), Value::Text("hello".into()));
    assert_eq!(v(&wb, 0, "B1"), Value::Number(84.0));
    assert_eq!(v(&wb, 0, "D160"), Value::Number(159.25));
    assert!(!report.warnings.iter().any(|w| w.contains(PASSWORD) || w.to_lowercase().contains("password")), "{:?}", report.warnings);
}

#[test]
fn xor_user_password_opens_biff5() {
    let (wb, _) = read_xls_with_password(&biff5(PASSWORD), Some(PASSWORD)).unwrap_or_else(|e| panic!("refused: {e}"));
    assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
}

#[test]
fn xor_without_password_asks() {
    for file in [biff8(PASSWORD), biff5(PASSWORD)] {
        let err = read_xls_with_password(&file, None).unwrap_err();
        assert!(matches!(err, XlsOpenError::PasswordRequired), "{err:?}");
    }
}

#[test]
fn xor_wrong_password_is_wrong() {
    for file in [biff8(PASSWORD), biff5(PASSWORD)] {
        for bad in ["Hunter2", "", "hunter", "a password longer than fifteen"] {
            let err = read_xls_with_password(&file, Some(bad)).unwrap_err();
            assert!(matches!(err, XlsOpenError::Decrypt(DecryptError::WrongPassword)), "{bad:?}: {err:?}");
            assert!(!format!("{err} {err:?}").contains(PASSWORD), "{err:?}");
        }
    }
}

#[test]
fn xor_default_password_opens_with_any_password() {
    for file in [biff8("VelvetSweatshop"), biff5("VelvetSweatshop")] {
        for given in [None, Some(PASSWORD)] {
            let (wb, _) = read_xls_with_password(&file, given).unwrap_or_else(|e| panic!("{given:?}: {e}"));
            assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
        }
    }
}

#[test]
fn truncated_xor_filepass_is_damaged_not_a_password_question() {
    let fp = filepass8(PASSWORD);
    for cut in 0..fp.len() {
        let file = compound("Workbook", &biff8_workbook(&fp[..cut], 0));
        for given in [None, Some(PASSWORD)] {
            let err = read_xls_with_password(&file, given).unwrap_err();
            assert!(!matches!(err, XlsOpenError::PasswordRequired | XlsOpenError::Decrypt(DecryptError::WrongPassword)), "{cut} {given:?}: {err:?}");
        }
    }
}
