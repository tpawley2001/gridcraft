//! Optional root property streams, decoded from [MS-OLEPS] §§2.21, 2.20 and 2.15.

use gridcraft_model::DocProps;

use crate::cfb::Compound;

const SUMMARY: [u8; 16] = [0xe0, 0x85, 0x9f, 0xf2, 0xf9, 0x4f, 0x68, 0x10, 0xab, 0x91, 0x08, 0x00, 0x2b, 0x27, 0xb3, 0xd9];
const DOCUMENT: [u8; 16] = [0x02, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10, 0x93, 0x97, 0x08, 0x00, 0x2b, 0x2c, 0xf9, 0xae];
const MAX_CHARS: usize = 32_767;

fn bytes<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..at.checked_add(N)?)?.try_into().ok()
}
fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes(data, at)?))
}

pub(super) fn read(cf: &Compound<'_>, props: &mut DocProps) {
    for (name, fmtid, summary) in [("\u{5}SummaryInformation", SUMMARY, true), ("\u{5}DocumentSummaryInformation", DOCUMENT, false)] {
        if let Ok(Some(data)) = cf.root_stream(name) {
            read_stream(&data, &fmtid, summary, props);
        }
    }
}

fn read_stream(data: &[u8], fmtid: &[u8; 16], summary: bool, props: &mut DocProps) {
    // These streams put the standard property set first; the optional second set is user-defined.
    if bytes::<2>(data, 0) != Some([0xfe, 0xff]) || !matches!(u32_at(data, 24), Some(1 | 2)) || data.get(28..44) != Some(fmtid.as_slice()) {
        return;
    }
    let Some(set) = (|| {
        let offset = usize::try_from(u32_at(data, 44)?).ok()?;
        if offset < 48 {
            return None;
        }
        let size = usize::try_from(u32_at(data, offset)?).ok()?;
        data.get(offset..offset.checked_add(size)?)
    })() else {
        return;
    };
    let Some(count) = u32_at(set, 4).filter(|n| *n <= 1000) else { return };
    let table_end = 8 + count as usize * 8;
    let Some(entries) = set.get(8..table_end) else { return };
    let value = |entry: &[u8]| -> Option<&[u8]> {
        let offset = usize::try_from(u32_at(entry, 4)?).ok()?;
        if offset < table_end {
            return None;
        }
        set.get(offset..)
    };
    let codepage = entries
        .chunks_exact(8)
        .find_map(|entry| {
            if u32_at(entry, 0)? != 1 {
                return None;
            }
            let v = value(entry)?;
            if u32_at(v, 0)? != 2 {
                return None;
            }
            Some(u16::from_le_bytes(bytes(v, 4)?))
        })
        .unwrap_or(1252);
    for entry in entries.chunks_exact(8) {
        let Some(pid) = u32_at(entry, 0) else { continue };
        let Some(v) = value(entry) else { continue };
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
            (false, 15) => &mut props.company,
            _ => continue,
        };
        let decoded = if summary && matches!(pid, 12 | 13) { filetime(v) } else { string(v, codepage) };
        if let Some(text) = decoded {
            *field = text;
        }
    }
}

fn string(v: &[u8], codepage: u16) -> Option<String> {
    let kind = u32_at(v, 0)?;
    let len = usize::try_from(u32_at(v, 4)?).ok()?;
    let wide = kind == 0x1f || (kind == 0x1e && codepage == 1200);
    if !matches!(kind, 0x1e | 0x1f) {
        return None;
    }
    let byte_len = if kind == 0x1f { len.checked_mul(2)? } else { len };
    let data = v.get(8..8usize.checked_add(byte_len)?)?;
    let text: String = if wide {
        if data.len() % 2 != 0 {
            return None;
        }
        char::decode_utf16(data.chunks_exact(2).filter_map(|c| Some(u16::from_le_bytes(c.try_into().ok()?))))
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
