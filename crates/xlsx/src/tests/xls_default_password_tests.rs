//! Legacy `.xls` workbooks encrypted with Excel's default password (`VelvetSweatshop`) open
//! without a prompt: XOR obfuscation, RC4 and RC4 CryptoAPI ([MS-XLS] 2.2.10, 2.4.117;
//! [MS-OFFCRYPTO] 2.3.5-2.3.7). Every workbook is built and encrypted here; no Excel-produced file.
//!
//! Stubs: each test states its case from the task spec table; replace the bodies, keep the names.

#[test]
fn md5_rfc1321_vectors() {
    panic!("acceptance test not written yet: md5_rfc1321_vectors (see the task spec table)");
}

#[test]
fn rc4_cryptoapi_default_password_opens() {
    panic!("acceptance test not written yet: rc4_cryptoapi_default_password_opens (see the task spec table)");
}

#[test]
fn rc4_cryptoapi_40bit_default_password_opens() {
    panic!("acceptance test not written yet: rc4_cryptoapi_40bit_default_password_opens (see the task spec table)");
}

#[test]
fn rc4_md5_default_password_opens() {
    panic!("acceptance test not written yet: rc4_md5_default_password_opens (see the task spec table)");
}

#[test]
fn xor_default_password_opens_biff8() {
    panic!("acceptance test not written yet: xor_default_password_opens_biff8 (see the task spec table)");
}

#[test]
fn xor_default_password_opens_biff5() {
    panic!("acceptance test not written yet: xor_default_password_opens_biff5 (see the task spec table)");
}

#[test]
fn boundsheet_offset_stays_plain() {
    panic!("acceptance test not written yet: boundsheet_offset_stays_plain (see the task spec table)");
}

#[test]
fn other_password_still_refused() {
    panic!("acceptance test not written yet: other_password_still_refused (see the task spec table)");
}

#[test]
fn hostile_filepass_never_panics() {
    panic!("acceptance test not written yet: hostile_filepass_never_panics (see the task spec table)");
}
