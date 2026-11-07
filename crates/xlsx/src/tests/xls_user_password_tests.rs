//! .xls workbooks encrypted with a user password ([MS-XLS] 2.4.117 FilePass) open with that
//! password, ask for it when none is given, and refuse a wrong one, as Excel does.

use super::xls_default_password_tests::{Scheme, encrypted_book};
use super::xls_tests::v;
use crate::{DecryptError, XlsOpenError, read_xls, read_xls_with_password};
use gridcraft_core::Value;

const SCHEMES: [Scheme; 3] = [Scheme::CryptoApi(128), Scheme::CryptoApi(40), Scheme::Rc4];

#[test]
fn user_password_opens() {
    for scheme in SCHEMES {
        let file = encrypted_book(scheme, "s3cret!", &[&[42.0, -7.25]], 200);
        let (wb, report) = read_xls_with_password(&file, Some("s3cret!")).unwrap_or_else(|e| panic!("refused: {e}"));
        assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
        assert_eq!(v(&wb, 0, "A2"), Value::Number(-7.25));
        // Past the first and second 1024-byte RC4 blocks.
        assert_eq!(v(&wb, 0, "C1"), Value::Number(0.5));
        assert_eq!(v(&wb, 0, "C200"), Value::Number(199.5));
        assert!(!report.warnings.iter().any(|w| w.contains("s3cret!")), "{:?}", report.warnings);
    }
}

#[test]
fn unicode_password_opens() {
    // RC4 passwords are UTF-16LE ([MS-OFFCRYPTO] 2.3.6.2, 2.3.5.2).
    for scheme in SCHEMES {
        let file = encrypted_book(scheme, "Пароль€", &[&[1.5]], 10);
        let (wb, _) = read_xls_with_password(&file, Some("Пароль€")).unwrap_or_else(|e| panic!("refused: {e}"));
        assert_eq!(v(&wb, 0, "A1"), Value::Number(1.5));
    }
}

#[test]
fn second_sheet_opens() {
    let file = encrypted_book(Scheme::CryptoApi(128), "pw", &[&[1.0], &[2.0, 3.0]], 120);
    let (wb, _) = read_xls_with_password(&file, Some("pw")).unwrap();
    assert_eq!(wb.sheets.len(), 2);
    assert_eq!(v(&wb, 1, "A2"), Value::Number(3.0));
}

#[test]
fn no_password_asks_for_one() {
    for scheme in SCHEMES {
        let file = encrypted_book(scheme, "s3cret!", &[&[42.0]], 10);
        let err = read_xls_with_password(&file, None).unwrap_err();
        assert!(matches!(err, XlsOpenError::PasswordRequired), "{err:?}");
    }
}

#[test]
fn wrong_password_is_wrong() {
    for scheme in SCHEMES {
        let file = encrypted_book(scheme, "s3cret!", &[&[42.0]], 10);
        for bad in ["S3cret!", "", "s3cret", "VelvetSweatshop"] {
            let err = read_xls_with_password(&file, Some(bad)).unwrap_err();
            assert!(matches!(err, XlsOpenError::Decrypt(DecryptError::WrongPassword)), "{bad:?}: {err:?}");
            assert!(!format!("{err} {err:?}").contains("s3cret"), "{err:?}");
        }
    }
}

#[test]
fn default_password_opens_without_asking() {
    for scheme in SCHEMES {
        let file = encrypted_book(scheme, "VelvetSweatshop", &[&[42.0]], 10);
        for given in [None, Some("anything")] {
            let (wb, _) = read_xls_with_password(&file, given).unwrap_or_else(|e| panic!("{given:?}: {e}"));
            assert_eq!(v(&wb, 0, "A1"), Value::Number(42.0));
        }
    }
}

#[test]
fn read_xls_still_refuses_user_password() {
    let file = encrypted_book(Scheme::Rc4, "s3cret!", &[&[42.0]], 10);
    let err = read_xls(&file).unwrap_err().to_string();
    assert!(err.contains("password-protected"), "{err}");
}

#[test]
fn damaged_filepass_is_not_a_password_question() {
    // An unknown RC4 major version or a truncated FilePass can't be opened by any password:
    // say so at once instead of asking.
    for scheme in [Scheme::CryptoApi(128), Scheme::Rc4] {
        let file = encrypted_book(scheme, "s3cret!", &[&[42.0]], 10);
        // FilePass payload starts after BOF (4 + len) and its own 4-byte header: wEncryptionType u16, then vMajor u16.
        let at = find_filepass(&file).expect("FilePass in the stream");
        let mut bad = file.clone();
        bad[at + 6] = 0x09; // vMajor 9: unknown
        for given in [None, Some("s3cret!")] {
            let err = read_xls_with_password(&bad, given).unwrap_err();
            assert!(
                matches!(err, XlsOpenError::Decrypt(DecryptError::Unsupported(_) | DecryptError::Malformed(_))) || matches!(err, XlsOpenError::Io(_)),
                "{given:?}: {err:?}"
            );
        }
    }
}

/// Byte offset of the FilePass record header (id 0x002F) in the compound file: the record right
/// after the workbook-globals BOF (id 0x0809, length 16, BIFF8 version 0x0600, type 0x0005).
fn find_filepass(file: &[u8]) -> Option<usize> {
    let bof = file.windows(8).position(|w| w == [0x09, 0x08, 0x10, 0x00, 0x00, 0x06, 0x05, 0x00])?;
    let at = bof + 4 + 16;
    (file.get(at..at + 2)? == [0x2F, 0x00]).then_some(at)
}

#[test]
fn filepass_helper_finds_the_record() {
    for scheme in [Scheme::CryptoApi(128), Scheme::Rc4] {
        let file = encrypted_book(scheme, "s3cret!", &[&[42.0]], 10);
        let at = find_filepass(&file).expect("FilePass right after the BOF");
        // wEncryptionType 1 = RC4 ([MS-XLS] 2.4.117).
        assert_eq!(file[at + 4..at + 6], [1, 0]);
    }
}
