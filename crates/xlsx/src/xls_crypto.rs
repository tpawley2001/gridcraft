//! Decryption of legacy `.xls` workbooks (FilePass: XOR obfuscation, RC4, RC4 CryptoAPI).

/// MD5 digest (RFC 1321).
pub(crate) fn md5(_data: &[u8]) -> [u8; 16] {
    unimplemented!("RFC 1321 MD5")
}
