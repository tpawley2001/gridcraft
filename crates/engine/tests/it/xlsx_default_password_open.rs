//! `.xlsx` workbooks encrypted with Excel's default password (`VelvetSweatshop`) open without
//! asking for a password, as Excel opens them and as legacy `.xls` files with that password
//! already open. Workbooks encrypted with any other password still ask for it.

use gridcraft_core::{CellRef, Value};
use gridcraft_engine::{Session, io};
use gridcraft_model::Workbook;
use serde_json::json;

const DEFAULT: &str = "VelvetSweatshop";

/// A workbook with 42 in A1, saved as a plain `.xlsx` package.
fn package() -> Vec<u8> {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("cell.set", json!({"cell": "A1", "input": "42"})).unwrap();
    io::save_bytes(&s.doc().unwrap().wb, "book.xlsx").unwrap()
}

fn a1(wb: &Workbook) -> Value {
    wb.sheet(0).unwrap().value(CellRef::parse("A1").unwrap())
}

#[test]
fn agile_default_password_opens_without_a_password() {
    let bytes = gridcraft_xlsx::testing::encrypt_workbook(&package(), DEFAULT);
    let (wb, warnings) = io::open_bytes("Book.xlsx", &bytes).expect("Excel opens it without a prompt");
    assert_eq!(a1(&wb), Value::Number(42.0));
    assert!(!warnings.iter().any(|w| w == io::DECRYPTED_NOTICE), "{warnings:?}");
}

#[test]
fn standard_default_password_opens_without_a_password() {
    let bytes = gridcraft_xlsx::testing::encrypt_workbook_standard(&package(), DEFAULT);
    let (wb, warnings) = io::open_bytes("Book.xlsx", &bytes).expect("Excel opens it without a prompt");
    assert_eq!(a1(&wb), Value::Number(42.0));
    assert!(!warnings.iter().any(|w| w == io::DECRYPTED_NOTICE), "{warnings:?}");
}

#[test]
fn default_password_file_opens_even_when_a_password_is_given() {
    let bytes = gridcraft_xlsx::testing::encrypt_workbook_standard(&package(), DEFAULT);
    let (wb, warnings) = io::open_bytes_with_password("Book.xlsx", &bytes, Some("something-else")).expect("opens");
    assert_eq!(a1(&wb), Value::Number(42.0));
    assert!(!warnings.iter().any(|w| w == io::DECRYPTED_NOTICE), "{warnings:?}");
}

#[test]
fn file_open_command_opens_a_default_password_workbook() {
    let bytes = gridcraft_xlsx::testing::encrypt_workbook(&package(), DEFAULT);
    let mut s = Session::new();
    let r = s.execute("file.open", json!({"name": "Book.xlsx", "base64": io::base64_encode(&bytes)})).expect("no password prompt");
    assert_eq!(r["title"], "Book.xlsx");
    assert_eq!(a1(&s.doc().unwrap().wb), Value::Number(42.0));
    assert!(!s.take_ui_requests().iter().any(|q| matches!(q, gridcraft_engine::UiRequest::Message(_))));
}

#[test]
fn other_passwords_still_ask_and_still_check() {
    const PW: &str = "synthetic-pw-1115";
    let bytes = gridcraft_xlsx::testing::encrypt_workbook_standard(&package(), PW);

    let err = io::open_bytes("Secret.xlsx", &bytes).unwrap_err().to_string();
    assert!(err.contains(io::PASSWORD_REQUIRED), "{err}");

    let err = io::open_bytes_with_password("Secret.xlsx", &bytes, Some("not-it")).unwrap_err().to_string();
    assert!(err.contains(io::WRONG_PASSWORD), "{err}");

    let (wb, warnings) = io::open_bytes_with_password("Secret.xlsx", &bytes, Some(PW)).expect("right password");
    assert_eq!(a1(&wb), Value::Number(42.0));
    assert_eq!(warnings.first().map(String::as_str), Some(io::DECRYPTED_NOTICE));
}
