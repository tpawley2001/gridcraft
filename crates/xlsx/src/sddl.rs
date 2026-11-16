//! Self-relative Windows security descriptors ([MS-DTYP] 2.4.6) to SDDL (2.5.1).

/// Unsupported ACEs and malformed components must retain the importer's loss warning.
pub(crate) fn security_descriptor_to_sddl(bytes: &[u8]) -> Option<String> {
    let mut header = Reader(bytes.get(..20)?);
    if header.byte()? != 1 {
        return None;
    }
    header.byte()?;
    let control = header.word()?;
    if control & 0x8000 == 0 {
        return None;
    }
    let owner = component(bytes, header.dword()?, sid)?;
    let group = component(bytes, header.dword()?, sid)?;
    let sacl = component(bytes, header.dword()?, acl)?;
    let dacl = component(bytes, header.dword()?, acl)?;
    let mut out = String::new();
    for (label, value) in [("O:", owner), ("G:", group)] {
        if let Some(value) = value {
            out.push_str(label);
            out.push_str(&value);
        }
    }
    for (label, present, flags, value) in
        [("D:", 0x0004, [(0x1000, "P"), (0x0400, "AI"), (0x0100, "AR")], dacl), ("S:", 0x0010, [(0x2000, "P"), (0x0800, "AI"), (0x0200, "AR")], sacl)]
    {
        if control & present != 0
            && let Some(value) = value
        {
            out.push_str(label);
            append_flags(&mut out, control, &flags);
            out.push_str(&value);
        }
    }
    Some(out)
}

// An absent component is valid; a present component must decode successfully.
fn component(bytes: &[u8], offset: u32, decode: fn(&[u8]) -> Option<String>) -> Option<Option<String>> {
    if offset == 0 {
        return Some(None);
    }
    let offset = usize::try_from(offset).ok()?;
    if offset < 20 {
        return None;
    }
    decode(bytes.get(offset..)?).map(Some)
}

struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, size: usize) -> Option<&'a [u8]> {
        let value = self.0.get(..size)?;
        self.0 = self.0.get(size..)?;
        Some(value)
    }
    fn byte(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }
    fn word(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn dword(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}

fn sid(bytes: &[u8]) -> Option<String> {
    let mut reader = Reader(bytes);
    if reader.byte()? != 1 {
        return None;
    }
    let count = reader.byte()?;
    if count > 15 {
        return None;
    }
    let authority = reader.take(6)?.iter().fold(0u64, |value, byte| (value << 8) | u64::from(*byte));
    let mut out = if authority < (1u64 << 32) { format!("S-1-{authority}") } else { format!("S-1-0x{authority:012X}") };
    for _ in 0..count {
        out.push_str(&format!("-{}", reader.dword()?));
    }
    let alias = match out.as_str() {
        "S-1-1-0" => "WD",
        "S-1-3-0" => "CO",
        "S-1-3-1" => "CG",
        "S-1-5-2" => "NU",
        "S-1-5-4" => "IU",
        "S-1-5-6" => "SU",
        "S-1-5-7" => "AN",
        "S-1-5-10" => "PS",
        "S-1-5-11" => "AU",
        "S-1-5-18" => "SY",
        "S-1-5-19" => "LS",
        "S-1-5-20" => "NS",
        "S-1-5-32-544" => "BA",
        "S-1-5-32-545" => "BU",
        "S-1-5-32-546" => "BG",
        "S-1-5-32-547" => "PU",
        "S-1-5-32-551" => "BO",
        "S-1-5-32-555" => "RD",
        _ => return Some(out),
    };
    Some(alias.into())
}

fn append_flags(out: &mut String, bits: u16, flags: &[(u16, &str)]) {
    for &(bit, text) in flags {
        if bits & bit != 0 {
            out.push_str(text);
        }
    }
}

fn rights(mask: u32) -> String {
    let mut remaining = mask;
    let mut out = String::new();
    for (bit, text) in [
        (0x10000000, "GA"),
        (0x80000000, "GR"),
        (0x40000000, "GW"),
        (0x20000000, "GX"),
        (0x20000, "RC"),
        (0x10000, "SD"),
        (0x40000, "WD"),
        (0x80000, "WO"),
        (1, "CC"),
        (2, "DC"),
        (4, "LC"),
        (8, "SW"),
        (0x10, "RP"),
        (0x20, "WP"),
        (0x40, "DT"),
        (0x80, "LO"),
        (0x100, "CR"),
    ] {
        if mask & bit != 0 {
            out.push_str(text);
            remaining &= !bit;
        }
    }
    if remaining != 0 || mask == 0 { format!("0x{mask:x}") } else { out }
}

fn acl(bytes: &[u8]) -> Option<String> {
    let mut header = Reader(bytes);
    if !matches!(header.byte()?, 2 | 4) {
        return None;
    }
    header.byte()?;
    let size = usize::from(header.word()?);
    let count = header.word()?;
    header.word()?;
    let mut entries = Reader(bytes.get(..size)?.get(8..)?);
    let mut out = String::new();
    // Each iteration consumes an ACE bounded by the ACL's u16 size.
    for _ in 0..count {
        let mut ace_header = Reader(entries.0);
        let kind = match ace_header.byte()? {
            0 => "A",
            1 => "D",
            2 => "AU",
            3 => "AL",
            _ => return None,
        };
        let flags = ace_header.byte()?;
        let size = usize::from(ace_header.word()?);
        let mut body = Reader(entries.take(size)?.get(4..)?);
        let mask = body.dword()?;
        let trustee = sid(body.0)?;
        out.push('(');
        out.push_str(kind);
        out.push(';');
        append_flags(&mut out, u16::from(flags), &[(1, "OI"), (2, "CI"), (4, "NP"), (8, "IO"), (0x10, "ID"), (0x40, "SA"), (0x80, "FA")]);
        out.push(';');
        out.push_str(&rights(mask));
        out.push_str(";;;");
        out.push_str(&trustee);
        out.push(')');
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit_descriptor() -> Vec<u8> {
        let mut bytes = vec![1, 0];
        bytes.extend(0xaa10u16.to_le_bytes()); // Self-relative SACL: protected, inherited, requested.
        for offset in [0u32, 0, 20, 0] {
            bytes.extend(offset.to_le_bytes());
        }
        bytes.extend([4, 0, 28, 0, 1, 0, 0, 0]);
        bytes.extend([2, 0xdf, 20, 0]);
        bytes.extend(0xf00f01ffu32.to_le_bytes());
        bytes.extend([1, 1, 0, 0, 0, 0, 0, 5]);
        bytes.extend(18u32.to_le_bytes());
        bytes
    }

    #[test]
    fn audit_flags_and_all_abbreviated_rights() {
        assert_eq!(
            security_descriptor_to_sddl(&audit_descriptor()).as_deref(),
            Some("S:PAIAR(AU;OICINPIOIDSAFA;GAGRGWGXRCSDWDWOCCDCLCSWRPWPDTLOCR;;;SY)")
        );
        assert_eq!(rights(0), "0x0");
        assert_eq!(rights(0x100001), "0x100001");
        assert_eq!(sid(&[1, 0, 0xff, 0xee, 0xdd, 0xcc, 0xbb, 0xaa]).as_deref(), Some("S-1-0xFFEEDDCCBBAA"));
    }

    #[test]
    fn truncated_components_are_rejected() {
        let bytes = audit_descriptor();
        for end in 0..bytes.len() {
            assert_eq!(security_descriptor_to_sddl(&bytes[..end]), None, "prefix length {end}");
        }
        let mut bad = bytes.clone();
        bad[22..24].copy_from_slice(&8u16.to_le_bytes());
        assert_eq!(security_descriptor_to_sddl(&bad), None);
        let mut bad = bytes;
        bad[30..32].copy_from_slice(&3u16.to_le_bytes());
        assert_eq!(security_descriptor_to_sddl(&bad), None);
    }
}
