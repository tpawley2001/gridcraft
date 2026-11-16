//! Optional root property streams, decoded from [MS-OLEPS] §§2.21, 2.20 and 2.15.

use std::fmt::Write as _;

use gridcraft_model::DocProps;

use crate::ReadReport;
use crate::cfb::Compound;
use crate::write::XML_DECL;
use crate::xml;

const SUMMARY: [u8; 16] = [0xe0, 0x85, 0x9f, 0xf2, 0xf9, 0x4f, 0x68, 0x10, 0xab, 0x91, 0x08, 0x00, 0x2b, 0x27, 0xb3, 0xd9];
const DOCUMENT: [u8; 16] = [0x02, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10, 0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae];
/// FMTID_UserDefinedProperties: the second set of DocumentSummaryInformation.
const USER: [u8; 16] = [0x05, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10, 0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae];
const FMTID_USER: &str = "{D5CDD505-2E9C-101B-9397-08002B2CF9AE}";
const NS_CUSTOM: &str = "http://schemas.openxmlformats.org/officeDocument/2006/custom-properties";
const NS_VTYPES: &str = "http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes";
const MAX_CHARS: usize = 32_767;
const MAX_PROPERTIES: u32 = 1000;
/// Largest custom properties part built from a file.
const MAX_XML: usize = 16 * 1024 * 1024;

fn bytes<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..at.checked_add(N)?)?.try_into().ok()
}
fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes(data, at)?))
}

/// Reads the built-in properties into `props` and returns the user-defined (custom) properties as
/// a `docProps/custom.xml` part, when the file has any.
pub(super) fn read(cf: &Compound<'_>, props: &mut DocProps, report: &mut ReadReport) -> Option<String> {
    let mut custom = None;
    for (name, fmtid, summary) in [("\u{5}SummaryInformation", SUMMARY, true), ("\u{5}DocumentSummaryInformation", DOCUMENT, false)] {
        if let Ok(Some(data)) = cf.root_stream(name)
            && let Some(found) = read_stream(&data, &fmtid, summary, props)
        {
            custom = Some(found);
        }
    }
    match custom? {
        Ok(Custom { xml, skipped }) => {
            if skipped {
                report.warn("Some custom document properties have a type that can't be kept and were dropped.");
            }
            xml
        }
        Err(()) => {
            report.warn("The custom document properties could not be read and were dropped.");
            None
        }
    }
}

/// A property set: each property's id and the bytes from its value to the end of the set.
struct Section<'a> {
    entries: Vec<(u32, &'a [u8])>,
    codepage: u16,
}

/// The property set whose offset is stored at `offset_at` in the stream.
fn section(data: &[u8], offset_at: usize) -> Option<Section<'_>> {
    let offset = usize::try_from(u32_at(data, offset_at)?).ok()?;
    if offset < 48 {
        return None;
    }
    let size = usize::try_from(u32_at(data, offset)?).ok()?;
    // Some producers overstate Size; keep readable properties within the stream.
    let available = data.get(offset..)?;
    let set = available.get(..size.min(available.len()))?;
    let count = u32_at(set, 4).filter(|n| *n <= MAX_PROPERTIES)?;
    let table_end = 8 + count as usize * 8;
    let entries: Vec<(u32, &[u8])> = set
        .get(8..table_end)?
        .as_chunks::<8>()
        .0
        .iter()
        .filter_map(|entry| {
            let offset = usize::try_from(u32_at(entry, 4)?).ok()?;
            if offset < table_end {
                return None;
            }
            Some((u32_at(entry, 0)?, set.get(offset..)?))
        })
        .collect();
    let codepage = entries
        .iter()
        .find_map(|&(pid, v)| {
            if pid != 1 || u32_at(v, 0)? != 2 {
                return None;
            }
            Some(u16::from_le_bytes(bytes(v, 4)?))
        })
        .unwrap_or(1252);
    Some(Section { entries, codepage })
}

/// Reads the standard property set; for DocumentSummaryInformation, also returns what its
/// optional second, user-defined set holds (`None` when it has none).
fn read_stream(data: &[u8], fmtid: &[u8; 16], summary: bool, props: &mut DocProps) -> Option<Result<Custom, ()>> {
    // These streams put the standard property set first; the optional second set is user-defined.
    let sets = u32_at(data, 24);
    if bytes::<2>(data, 0) != Some([0xfe, 0xff]) || !matches!(sets, Some(1 | 2)) || data.get(28..44) != Some(fmtid.as_slice()) {
        return None;
    }
    if let Some(set) = section(data, 44) {
        read_builtin(&set, summary, props);
    }
    if summary || sets != Some(2) {
        return None;
    }
    if data.get(48..64) != Some(USER.as_slice()) {
        return Some(Err(()));
    }
    Some(section(data, 64).ok_or(()).and_then(|set| user_defined(&set)))
}

fn read_builtin(set: &Section<'_>, summary: bool, props: &mut DocProps) {
    for &(pid, v) in &set.entries {
        let field = match (summary, pid) {
            (true, 2) => &mut props.title,
            (true, 3) => &mut props.subject,
            (true, 4) => &mut props.author,
            (true, 5) => &mut props.keywords,
            (true, 6) => &mut props.description,
            (true, 8) => &mut props.last_modified_by,
            (true, 12) => &mut props.created,
            (true, 13) => &mut props.modified,
            (false, 2) => &mut props.category,
            (false, 14) => &mut props.manager,
            (false, 15) => &mut props.company,
            _ => continue,
        };
        let decoded = if summary && matches!(pid, 12 | 13) { filetime(v) } else { string(v, set.codepage) };
        if let Some(text) = decoded {
            *field = text;
        }
    }
}

/// The user-defined properties as a custom properties part (ECMA-376 Part 1 §22.3).
struct Custom {
    /// `None` when no property could be kept.
    xml: Option<String>,
    /// Some properties had a type (or no name) that can't be kept.
    skipped: bool,
}

/// Names Office reserves in the user-defined set ([MS-OSHARED]): the hyperlink table,
/// a document GUID and the hyperlink base. They are not custom properties, so they are left out
/// without counting as dropped.
const OFFICE_RESERVED: &[&str] = &["_PID_HLINKS", "_PID_GUID", "_PID_LINKBASE"];

/// Added to a linked property's id for the entry naming its source ([MS-OSHARED]).
const LINK_FLAG: u32 = 0x0100_0000;

/// Builds the custom properties part from the user-defined set; `Err` when the set is damaged.
fn user_defined(set: &Section<'_>) -> Result<Custom, ()> {
    // Property 0 is the dictionary ([MS-OLEPS] 2.17), naming each property; 1 is the code page
    // and ids with the high bit set are reserved (locale, behavior).
    let user = |pid: u32| pid > 1 && pid < 0x8000_0000;
    let names = match set.entries.iter().find(|(pid, _)| *pid == 0) {
        Some(&(_, v)) => dictionary(v, set.codepage).ok_or(())?,
        None if set.entries.iter().any(|(pid, _)| user(*pid)) => return Err(()),
        None => vec![],
    };
    let mut xml = format!("{XML_DECL}<Properties xmlns=\"{NS_CUSTOM}\" xmlns:vt=\"{NS_VTYPES}\">");
    let mut next_pid = 2u32;
    let mut skipped = false;
    for &(pid, v) in set.entries.iter().filter(|(pid, _)| user(*pid)) {
        // A linked property ("Link to content") stores its source under `pid | LINK_FLAG`, with no
        // dictionary entry; it is not a property of its own.
        if pid & LINK_FLAG != 0 {
            continue;
        }
        let Some((_, name)) = names.iter().find(|(id, _)| *id == pid) else {
            skipped = true;
            continue;
        };
        if OFFICE_RESERVED.contains(&name.as_str()) {
            continue;
        }
        let Some((tag, value)) = typed_value(v, set.codepage).filter(|_| !name.is_empty()) else {
            skipped = true;
            continue;
        };
        let link = set
            .entries
            .iter()
            .find(|(id, _)| *id == pid | LINK_FLAG)
            .and_then(|&(_, l)| string(l, set.codepage))
            .map(|source| format!(" linkTarget=\"{}\"", xml::esc_attr(&source)))
            .unwrap_or_default();
        let _ = write!(
            xml,
            "<property fmtid=\"{FMTID_USER}\" pid=\"{next_pid}\" name=\"{}\"{link}><vt:{tag}>{}</vt:{tag}></property>",
            xml::esc_attr(name),
            xml::esc(&value)
        );
        next_pid = next_pid.saturating_add(1);
        if xml.len() > MAX_XML {
            return Err(());
        }
    }
    xml.push_str("</Properties>");
    Ok(Custom { xml: (next_pid > 2).then_some(xml), skipped })
}

/// The names of a dictionary property: (property id, name) pairs. A name that can't be decoded is
/// left out, so only the property it names is dropped.
fn dictionary(v: &[u8], codepage: u16) -> Option<Vec<(u32, String)>> {
    let count = u32_at(v, 0).filter(|n| *n <= MAX_PROPERTIES)?;
    let wide = codepage == 1200;
    let mut at = 4usize;
    let mut out = vec![];
    for _ in 0..count {
        let pid = u32_at(v, at)?;
        let len = usize::try_from(u32_at(v, at.checked_add(4)?)?).ok()?;
        at = at.checked_add(8)?;
        let byte_len = if wide { len.checked_mul(2)? } else { len };
        let end = at.checked_add(byte_len)?;
        if let Some(name) = decode(v.get(at..end)?, wide, codepage) {
            out.push((pid, name));
        }
        // Unicode names are padded to a multiple of 4 bytes.
        at = if wide { end.checked_next_multiple_of(4)? } else { end };
    }
    Some(out)
}

/// A user-defined property value as its `vt:` element name and text; `None` for other types.
fn typed_value(v: &[u8], codepage: u16) -> Option<(&'static str, String)> {
    Some(match u32_at(v, 0)? {
        0x1e | 0x1f => ("lpwstr", string(v, codepage)?),
        0x03 => ("i4", i32::from_le_bytes(bytes(v, 4)?).to_string()),
        0x05 => {
            let n = f64::from_le_bytes(bytes(v, 4)?);
            let text = match n {
                n if n.is_nan() => "NaN".to_string(),
                f64::INFINITY => "INF".to_string(),
                f64::NEG_INFINITY => "-INF".to_string(),
                n => n.to_string(),
            };
            ("r8", text)
        }
        0x0b => ("bool", (u16::from_le_bytes(bytes(v, 4)?) != 0).to_string()),
        0x40 => ("filetime", filetime(v)?),
        _ => return None,
    })
}

fn string(v: &[u8], codepage: u16) -> Option<String> {
    let kind = u32_at(v, 0)?;
    let len = usize::try_from(u32_at(v, 4)?).ok()?;
    if !matches!(kind, 0x1e | 0x1f) {
        return None;
    }
    let byte_len = if kind == 0x1f { len.checked_mul(2)? } else { len };
    let data = v.get(8..8usize.checked_add(byte_len)?)?;
    decode(data, kind == 0x1f || codepage == 1200, codepage)
}

/// Property-set text: UTF-16 when `wide`, otherwise in the set's code page; NULs trimmed, capped.
fn decode(data: &[u8], wide: bool, codepage: u16) -> Option<String> {
    let text: String = if wide {
        if !data.len().is_multiple_of(2) {
            return None;
        }
        char::decode_utf16(data.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes))
            .map(|c| c.unwrap_or('\u{fffd}'))
            .take(MAX_CHARS + 1)
            .collect()
    } else {
        match codepage {
            0 | 1252 => data
                .iter()
                .take(MAX_CHARS + 1)
                .map(|&b| {
                    if (0x80..=0x9f).contains(&b) {
                        char::from_u32(crate::csv::CP1252.get(usize::from(b - 0x80)).copied().unwrap_or(0xfffd)).unwrap_or('\u{fffd}')
                    } else {
                        char::from(b)
                    }
                })
                .collect(),
            65001 => String::from_utf8_lossy(data.get(..data.len().min((MAX_CHARS + 1) * 4))?).chars().take(MAX_CHARS + 1).collect(),
            cp if crate::csv::codepage_supported(cp) => {
                let bytes = data.get(..data.len().min(MAX_CHARS + 1))?;
                crate::csv::decode_codepage(bytes, cp)
            }
            // A double-byte code page has no table here: only its ASCII range reads.
            _ if data.iter().all(|b| *b < 0x80) => data.iter().take(MAX_CHARS + 1).map(|&b| char::from(b)).collect(),
            _ => return None,
        }
    };
    Some(text.trim_end_matches('\0').chars().take(MAX_CHARS).collect())
}

fn filetime(v: &[u8]) -> Option<String> {
    if u32_at(v, 0)? != 0x40 {
        return None;
    }
    let ticks = u64::from_le_bytes(bytes(v, 4)?);
    if ticks == 0 {
        return None;
    }
    let seconds = i64::try_from(ticks / 10_000_000).ok()?.checked_sub(11_644_473_600)?;
    // Core's calendar conversion uses chrono; split whole seconds without floating-point serials.
    let (year, month, day) = gridcraft_core::date::civil_from_days(seconds.div_euclid(86_400));
    if !(1601..=9999).contains(&year) {
        return None;
    }
    let time = seconds.rem_euclid(86_400);
    Some(format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", time / 3600, time / 60 % 60, time % 60))
}
