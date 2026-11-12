//! CSV import and export (RFC 4180 with the usual real-world leniency). Decoding, delimiter
//! sniffing, the record parser and field quoting are `craft-csv`'s; this module turns records into
//! typed cells (with each column's date order) and writes a sheet.

use std::sync::Arc;

use gridcraft_core::date::{days_in_month_in, serial_from_ymd};
use gridcraft_core::parse::{Parsed, parse_input};
use gridcraft_core::{CellRef, DateSystem, MAX_COLS, MAX_ROWS, Value, col_to_letters};
use gridcraft_model::{Cell, NumFmt, Sheet, StyleId, Workbook};

use crate::{CsvOptions, IoError, ReadReport};

/// The largest CSV or tab-separated file written (bytes).
pub const MAX_CSV_BYTES: usize = 512 << 20;

pub use craft_csv::{decode_text, sniff_delimiter};

/// Windows-1252 code points for bytes 0x80–0x9F (others map to Latin-1).
pub(crate) const CP1252: [u32; 32] = [
    0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F, 0x90, 0x2018, 0x2019,
    0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
];

/// Windows-1256 code points for bytes 0x80–0xFF (unicode.org `CP1256.TXT`).
pub(crate) const CP1256: [u32; 128] = [
    0x20AC, 0x067E, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0679, 0x2039, 0x0152, 0x0686, 0x0698, 0x0688, 0x06AF, 0x2018,
    0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x06A9, 0x2122, 0x0691, 0x203A, 0x0153, 0x200C, 0x200D, 0x06BA, 0x00A0, 0x060C, 0x00A2, 0x00A3,
    0x00A4, 0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x06BE, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5,
    0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x061B, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x061F, 0x06C1, 0x0621, 0x0622, 0x0623, 0x0624, 0x0625, 0x0626, 0x0627,
    0x0628, 0x0629, 0x062A, 0x062B, 0x062C, 0x062D, 0x062E, 0x062F, 0x0630, 0x0631, 0x0632, 0x0633, 0x0634, 0x0635, 0x0636, 0x00D7, 0x0637, 0x0638,
    0x0639, 0x063A, 0x0640, 0x0641, 0x0642, 0x0643, 0x00E0, 0x0644, 0x00E2, 0x0645, 0x0646, 0x0647, 0x0648, 0x00E7, 0x00E8, 0x00E9, 0x00EA, 0x00EB,
    0x0649, 0x064A, 0x00EE, 0x00EF, 0x064B, 0x064C, 0x064D, 0x064E, 0x00F4, 0x064F, 0x0650, 0x00F7, 0x0651, 0x00F9, 0x0652, 0x00FB, 0x00FC, 0x200E,
    0x200F, 0x06D2,
];

/// Windows-1251 (Cyrillic) code points for bytes 0x80–0xFF (unicode.org `CP1251.TXT`); 0x98 is unassigned.
const CP1251: [u32; 128] = [
    0x0402, 0x0403, 0x201A, 0x0453, 0x201E, 0x2026, 0x2020, 0x2021, 0x20AC, 0x2030, 0x0409, 0x2039, 0x040A, 0x040C, 0x040B, 0x040F, 0x0452, 0x2018,
    0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0x0459, 0x203A, 0x045A, 0x045C, 0x045B, 0x045F, 0x00A0, 0x040E, 0x045E, 0x0408,
    0x00A4, 0x0490, 0x00A6, 0x00A7, 0x0401, 0x00A9, 0x0404, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x0407, 0x00B0, 0x00B1, 0x0406, 0x0456, 0x0491, 0x00B5,
    0x00B6, 0x00B7, 0x0451, 0x2116, 0x0454, 0x00BB, 0x0458, 0x0405, 0x0455, 0x0457, 0x0410, 0x0411, 0x0412, 0x0413, 0x0414, 0x0415, 0x0416, 0x0417,
    0x0418, 0x0419, 0x041A, 0x041B, 0x041C, 0x041D, 0x041E, 0x041F, 0x0420, 0x0421, 0x0422, 0x0423, 0x0424, 0x0425, 0x0426, 0x0427, 0x0428, 0x0429,
    0x042A, 0x042B, 0x042C, 0x042D, 0x042E, 0x042F, 0x0430, 0x0431, 0x0432, 0x0433, 0x0434, 0x0435, 0x0436, 0x0437, 0x0438, 0x0439, 0x043A, 0x043B,
    0x043C, 0x043D, 0x043E, 0x043F, 0x0440, 0x0441, 0x0442, 0x0443, 0x0444, 0x0445, 0x0446, 0x0447, 0x0448, 0x0449, 0x044A, 0x044B, 0x044C, 0x044D,
    0x044E, 0x044F,
];

/// Mac Roman (code page 10000) code points for bytes 0x80–0xFF (unicode.org Apple `ROMAN.TXT`; 0xDB is the euro sign).
const MAC_ROMAN: [u32; 128] = [
    0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1, 0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8, 0x00EA, 0x00EB,
    0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3, 0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC, 0x2020, 0x00B0, 0x00A2, 0x00A3,
    0x00A7, 0x2022, 0x00B6, 0x00DF, 0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8, 0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5,
    0x2202, 0x2211, 0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x03A9, 0x00E6, 0x00F8, 0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB,
    0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153, 0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA, 0x00FF, 0x0178,
    0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02, 0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1, 0x00CB, 0x00C8, 0x00CD, 0x00CE,
    0x00CF, 0x00CC, 0x00D3, 0x00D4, 0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC, 0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD,
    0x02DB, 0x02C7,
];

/// Windows-1250 (Central European) code points for bytes 0x80–0xFF (unicode.org `CP1250.TXT`); undefined bytes are U+FFFD.
const CP1250: [u32; 128] = [
    0x20AC, 0xFFFD, 0x201A, 0xFFFD, 0x201E, 0x2026, 0x2020, 0x2021, 0xFFFD, 0x2030, 0x0160, 0x2039, 0x015A, 0x0164, 0x017D, 0x0179, 0xFFFD, 0x2018,
    0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0x0161, 0x203A, 0x015B, 0x0165, 0x017E, 0x017A, 0x00A0, 0x02C7, 0x02D8, 0x0141,
    0x00A4, 0x0104, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x015E, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x017B, 0x00B0, 0x00B1, 0x02DB, 0x0142, 0x00B4, 0x00B5,
    0x00B6, 0x00B7, 0x00B8, 0x0105, 0x015F, 0x00BB, 0x013D, 0x02DD, 0x013E, 0x017C, 0x0154, 0x00C1, 0x00C2, 0x0102, 0x00C4, 0x0139, 0x0106, 0x00C7,
    0x010C, 0x00C9, 0x0118, 0x00CB, 0x011A, 0x00CD, 0x00CE, 0x010E, 0x0110, 0x0143, 0x0147, 0x00D3, 0x00D4, 0x0150, 0x00D6, 0x00D7, 0x0158, 0x016E,
    0x00DA, 0x0170, 0x00DC, 0x00DD, 0x0162, 0x00DF, 0x0155, 0x00E1, 0x00E2, 0x0103, 0x00E4, 0x013A, 0x0107, 0x00E7, 0x010D, 0x00E9, 0x0119, 0x00EB,
    0x011B, 0x00ED, 0x00EE, 0x010F, 0x0111, 0x0144, 0x0148, 0x00F3, 0x00F4, 0x0151, 0x00F6, 0x00F7, 0x0159, 0x016F, 0x00FA, 0x0171, 0x00FC, 0x00FD,
    0x0163, 0x02D9,
];

/// Windows-1253 (Greek) code points for bytes 0x80–0xFF (unicode.org `CP1253.TXT`); undefined bytes are U+FFFD.
const CP1253: [u32; 128] = [
    0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0xFFFD, 0x2030, 0xFFFD, 0x2039, 0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD, 0x2018,
    0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0xFFFD, 0x203A, 0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD, 0x00A0, 0x0385, 0x0386, 0x00A3,
    0x00A4, 0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0xFFFD, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x2015, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x0384, 0x00B5,
    0x00B6, 0x00B7, 0x0388, 0x0389, 0x038A, 0x00BB, 0x038C, 0x00BD, 0x038E, 0x038F, 0x0390, 0x0391, 0x0392, 0x0393, 0x0394, 0x0395, 0x0396, 0x0397,
    0x0398, 0x0399, 0x039A, 0x039B, 0x039C, 0x039D, 0x039E, 0x039F, 0x03A0, 0x03A1, 0xFFFD, 0x03A3, 0x03A4, 0x03A5, 0x03A6, 0x03A7, 0x03A8, 0x03A9,
    0x03AA, 0x03AB, 0x03AC, 0x03AD, 0x03AE, 0x03AF, 0x03B0, 0x03B1, 0x03B2, 0x03B3, 0x03B4, 0x03B5, 0x03B6, 0x03B7, 0x03B8, 0x03B9, 0x03BA, 0x03BB,
    0x03BC, 0x03BD, 0x03BE, 0x03BF, 0x03C0, 0x03C1, 0x03C2, 0x03C3, 0x03C4, 0x03C5, 0x03C6, 0x03C7, 0x03C8, 0x03C9, 0x03CA, 0x03CB, 0x03CC, 0x03CD,
    0x03CE, 0xFFFD,
];

/// Windows-1254 (Turkish) code points for bytes 0x80–0xFF (unicode.org `CP1254.TXT`); undefined bytes are U+FFFD.
const CP1254: [u32; 128] = [
    0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD, 0xFFFD, 0xFFFD, 0xFFFD, 0x2018,
    0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0xFFFD, 0x0178, 0x00A0, 0x00A1, 0x00A2, 0x00A3,
    0x00A4, 0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5,
    0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF, 0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5, 0x00C6, 0x00C7,
    0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC, 0x00CD, 0x00CE, 0x00CF, 0x011E, 0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7, 0x00D8, 0x00D9,
    0x00DA, 0x00DB, 0x00DC, 0x0130, 0x015E, 0x00DF, 0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5, 0x00E6, 0x00E7, 0x00E8, 0x00E9, 0x00EA, 0x00EB,
    0x00EC, 0x00ED, 0x00EE, 0x00EF, 0x011F, 0x00F1, 0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7, 0x00F8, 0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x0131,
    0x015F, 0x00FF,
];

/// Windows-1257 (Baltic) code points for bytes 0x80–0xFF (unicode.org `CP1257.TXT`); undefined bytes are U+FFFD.
const CP1257: [u32; 128] = [
    0x20AC, 0xFFFD, 0x201A, 0xFFFD, 0x201E, 0x2026, 0x2020, 0x2021, 0xFFFD, 0x2030, 0xFFFD, 0x2039, 0xFFFD, 0x00A8, 0x02C7, 0x00B8, 0xFFFD, 0x2018,
    0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0xFFFD, 0x203A, 0xFFFD, 0x00AF, 0x02DB, 0xFFFD, 0x00A0, 0xFFFD, 0x00A2, 0x00A3,
    0x00A4, 0xFFFD, 0x00A6, 0x00A7, 0x00D8, 0x00A9, 0x0156, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00C6, 0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5,
    0x00B6, 0x00B7, 0x00F8, 0x00B9, 0x0157, 0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00E6, 0x0104, 0x012E, 0x0100, 0x0106, 0x00C4, 0x00C5, 0x0118, 0x0112,
    0x010C, 0x00C9, 0x0179, 0x0116, 0x0122, 0x0136, 0x012A, 0x013B, 0x0160, 0x0143, 0x0145, 0x00D3, 0x014C, 0x00D5, 0x00D6, 0x00D7, 0x0172, 0x0141,
    0x015A, 0x016A, 0x00DC, 0x017B, 0x017D, 0x00DF, 0x0105, 0x012F, 0x0101, 0x0107, 0x00E4, 0x00E5, 0x0119, 0x0113, 0x010D, 0x00E9, 0x017A, 0x0117,
    0x0123, 0x0137, 0x012B, 0x013C, 0x0161, 0x0144, 0x0146, 0x00F3, 0x014D, 0x00F5, 0x00F6, 0x00F7, 0x0173, 0x0142, 0x015B, 0x016B, 0x00FC, 0x017C,
    0x017E, 0x02D9,
];

/// Whether [`decode_codepage`] knows a code page (Windows 1250 Central European, 1251 Cyrillic,
/// 1252 Western, 1253 Greek, 1254 Turkish, 1256 Arabic, 1257 Baltic, and 10000 Mac Roman; 367
/// US-ASCII reads as 1252).
pub(crate) fn codepage_supported(codepage: u16) -> bool {
    matches!(codepage, 367 | 1250 | 1251 | 1252 | 1253 | 1254 | 1256 | 1257 | 10000)
}

/// Decodes 8-bit text in a Windows code page; code pages [`codepage_supported`] doesn't know
/// read as Windows-1252.
pub(crate) fn decode_codepage(bytes: &[u8], codepage: u16) -> String {
    let high = |table: &[u32], b: u8| char::from_u32(table.get(usize::from(b - 0x80)).copied().unwrap_or(0xFFFD)).unwrap_or('\u{FFFD}');
    bytes
        .iter()
        .map(|&b| match (codepage, b) {
            (_, 0x00..=0x7F) => b as char,
            (1250, _) => high(&CP1250, b),
            (1251, _) => high(&CP1251, b),
            (1253, _) => high(&CP1253, b),
            (1254, _) => high(&CP1254, b),
            (1256, _) => high(&CP1256, b),
            (1257, _) => high(&CP1257, b),
            (10000, _) => high(&MAC_ROMAN, b),
            (_, 0x80..=0x9F) => high(&CP1252, b),
            _ => b as char,
        })
        .collect()
}

/// [`craft_csv::parse_records`] with a record kept to the grid's [`MAX_COLS`] fields.
fn parse_records(text: &str, delim: char, quote: char, on_record: impl FnMut(Vec<String>) -> bool) {
    craft_csv::parse_records(text, delim, quote, MAX_COLS as usize, on_record);
}

/// How many rows and bytes of a text file are read to decide each column's date order.
const DATE_SAMPLE_ROWS: usize = 200;
const DATE_SAMPLE_BYTES: usize = 1 << 20;

/// What a column's numeric dates say about the order of day and month.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DateRule {
    /// No `d/m/y` or `d-m-y` value in the sample: read as typed input does.
    NoDates,
    /// A value reads only month first (`12/31/2026`), none reads only day first.
    MonthFirst,
    /// A value reads only day first (`31/05/2026`), none reads only month first.
    DayFirst,
    /// Every date is ambiguous (`03/04/2026`): keep the input parser's month-first reading.
    Ambiguous,
    /// Values read only day first and values read only month first: the dates stay text.
    Conflict,
}

impl DateRule {
    fn of(day_first: usize, month_first: usize, ambiguous: usize) -> Self {
        match (day_first > 0, month_first > 0, ambiguous > 0) {
            (true, true, _) => Self::Conflict,
            (true, false, _) => Self::DayFirst,
            (false, true, _) => Self::MonthFirst,
            (false, false, true) => Self::Ambiguous,
            (false, false, false) => Self::NoDates,
        }
    }
}

/// The evidence a column's sample gives for its date order.
#[derive(Clone, Default)]
struct DateEvidence {
    day_first: usize,
    month_first: usize,
    ambiguous: usize,
    day_first_example: Option<String>,
    month_first_example: Option<String>,
    ambiguous_example: Option<String>,
}

/// A numeric `d/m/y` or `d-m-y` field, with the serial it has read each way and its time of day.
struct NumericDate {
    day_first: Option<f64>,
    month_first: Option<f64>,
    time: Option<f64>,
}

impl NumericDate {
    /// The field under the column's rule: a date when that reading is valid, else text.
    fn read(&self, field: &str, rule: DateRule) -> Parsed {
        let serial = match rule {
            DateRule::DayFirst => self.day_first,
            DateRule::MonthFirst | DateRule::Ambiguous => self.month_first,
            DateRule::Conflict | DateRule::NoDates => None,
        };
        match serial {
            Some(serial) => Parsed {
                value: Value::number(serial + self.time.unwrap_or(0.0)),
                format: Some(if self.time.is_some() { "m/d/yyyy h:mm" } else { "m/d/yyyy" }),
            },
            None => Parsed { value: Value::text(field), format: None },
        }
    }
}

/// A numeric date written with `/` or `-`, with the serial it has read day first and month first.
/// `None` when the field isn't such a date (including ISO `yyyy-mm-dd`, month names and dots,
/// which read one way and keep the input parser).
fn numeric_date(field: &str, sys: DateSystem) -> Option<NumericDate> {
    let text = field.trim();
    let (date, time) = split_time(text, sys);
    // Typed input reads a trailing comma after a date (`3/4/2026,`); do the same here.
    let date = date.trim_end_matches(',');
    let mut parts = date.split(['/', '-']);
    let (a, b, c) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let (a, b, c) = (a.trim(), b.trim(), c.trim());
    // ISO dates are unambiguous; the input parser reads them. A three-digit year is not a
    // date to typed input either.
    if a.len() == 4 || c.len() == 3 {
        return None;
    }
    let a = parse_digits(a)?;
    let b = parse_digits(b)?;
    let year = expand_year(c, parse_digits(c)?);
    let day_first = valid_date(sys, year, b, a);
    let month_first = valid_date(sys, year, a, b);
    if day_first.is_none() && month_first.is_none() {
        return None;
    }
    Some(NumericDate { day_first, month_first, time })
}

/// Splits a field into its date and a time of day, when the tail reads as a time. Both the first
/// and last space are tried, so `03/04/2026 10:30 AM` and `03 / 04 / 2026 10:30` both split.
fn split_time(text: &str, sys: DateSystem) -> (&str, Option<f64>) {
    for at in [text.find(' '), text.rfind(' ')].into_iter().flatten() {
        let (date, time) = text.split_at(at);
        let time = time.trim();
        // Times are short; a long tail is not one, and parsing it would be wasted work.
        if time.len() > 40 {
            continue;
        }
        if let Some(fraction) = time_of_day(time, sys) {
            return (date.trim(), Some(fraction));
        }
    }
    (text, None)
}

fn time_of_day(text: &str, sys: DateSystem) -> Option<f64> {
    match parse_input(text, sys) {
        Parsed { value: Value::Number(fraction), format: Some(code) } if code.contains('h') => Some(fraction),
        _ => None,
    }
}

fn parse_digits(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// Expands a one- or two-digit year the way typed input does (`26` is 2026, `30` is 1930).
fn expand_year(text: &str, year: i64) -> i64 {
    if text.len() <= 2 { if year < 30 { 2000 + year } else { 1900 + year } } else { year }
}

/// The serial for a date the input parser accepts, or `None`.
fn valid_date(sys: DateSystem, year: i64, month: i64, day: i64) -> Option<f64> {
    if !(1..=12).contains(&month) || day < 1 || year < 1900 {
        return None;
    }
    let year = i32::try_from(year).ok()?;
    let month = u32::try_from(month).ok()?;
    if day > i64::from(days_in_month_in(sys, year, month)) {
        return None;
    }
    serial_from_ymd(sys, i64::from(year), i64::from(month), day)
}

/// The first records of the file, bounded by rows and bytes, for date-order inference.
fn sample_records(text: &str, delim: char, quote: char) -> Vec<Vec<String>> {
    let mut cut = text.len().min(DATE_SAMPLE_BYTES);
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let truncated = cut < text.len();
    let sample = text.get(..cut).unwrap_or_default();
    let mut rows = Vec::new();
    parse_records(sample, delim, quote, |record| {
        rows.push(record);
        rows.len() < DATE_SAMPLE_ROWS
    });
    // A cut through the last record leaves a partial field; it says nothing reliable.
    if truncated {
        rows.pop();
    }
    rows
}

/// The date rule for each column, from the sample.
fn infer_date_rules(rows: &[Vec<String>], sys: DateSystem) -> Vec<(DateRule, DateEvidence)> {
    let width = rows.iter().map(Vec::len).max().unwrap_or(0).min(MAX_COLS as usize);
    let mut columns = vec![DateEvidence::default(); width];
    for row in rows {
        for (col, field) in row.iter().enumerate() {
            let Some(column) = columns.get_mut(col) else { continue };
            let Some(date) = numeric_date(field, sys) else { continue };
            let example = || field.trim().chars().take(40).collect::<String>();
            match (date.day_first.is_some(), date.month_first.is_some()) {
                (true, true) => {
                    column.ambiguous += 1;
                    column.ambiguous_example.get_or_insert_with(example);
                }
                (true, false) => {
                    column.day_first += 1;
                    column.day_first_example.get_or_insert_with(example);
                }
                (false, true) => {
                    column.month_first += 1;
                    column.month_first_example.get_or_insert_with(example);
                }
                (false, false) => {}
            }
        }
    }
    columns.into_iter().map(|c| (DateRule::of(c.day_first, c.month_first, c.ambiguous), c)).collect()
}

/// Warns when the sample could not choose a date order: an all-ambiguous column keeps the
/// month-first reading, and a column with both kinds keeps its dates as text.
fn date_warnings(columns: &[(DateRule, DateEvidence)], report: &mut ReadReport) {
    for (col, (rule, evidence)) in columns.iter().enumerate() {
        match rule {
            DateRule::Ambiguous => report.warn(format!(
                "column {}: every date is ambiguous, so it was read month first ({})",
                col_to_letters(col as u32),
                evidence.ambiguous_example.as_deref().unwrap_or("a date")
            )),
            DateRule::Conflict => report.warn(format!(
                "column {}: it holds both day-first and month-first dates ({} and {}), so its dates stay text",
                col_to_letters(col as u32),
                evidence.day_first_example.as_deref().unwrap_or("a day-first date"),
                evidence.month_first_example.as_deref().unwrap_or("a month-first date")
            )),
            DateRule::NoDates | DateRule::MonthFirst | DateRule::DayFirst => {}
        }
    }
}

/// Reads one field with the column's date rule; other values go through typed input.
fn read_field(field: &str, sys: DateSystem, rule: DateRule) -> Parsed {
    match numeric_date(field, sys) {
        Some(date) if rule != DateRule::NoDates => date.read(field, rule),
        _ => parse_input(field, sys),
    }
}

pub fn read_csv(bytes: &[u8], opts: &CsvOptions) -> Result<Workbook, IoError> {
    read_csv_with_report(bytes, opts).map(|(wb, _)| wb)
}

/// [`read_csv`], with the warnings an open shows: a column whose date order had to be guessed,
/// or whose dates stayed text because it holds both orders.
pub fn read_csv_with_report(bytes: &[u8], opts: &CsvOptions) -> Result<(Workbook, ReadReport), IoError> {
    let text = decode_text(bytes);
    let quote = if opts.quote == 0 { '"' } else { opts.quote as char };
    let delim = if opts.delimiter == 0 { sniff_delimiter(&text, quote) } else { opts.delimiter } as char;
    if delim == quote {
        return Err(IoError::Format("the delimiter and quote character must differ".into()));
    }
    let date_columns = if opts.parse_values { infer_date_rules(&sample_records(&text, delim, quote), opts.date_system) } else { vec![] };
    let mut report = ReadReport::default();
    date_warnings(&date_columns, &mut report);
    let mut wb = Workbook::new();
    wb.date_system = opts.date_system;
    wb.csv_delimiter = u8::try_from(delim).ok();
    let mut sheet = Sheet::new("Sheet1");
    let mut styles = std::mem::take(&mut wb.styles);
    let mut fmt_cache: Vec<(&'static str, StyleId)> = vec![];
    let mut row: u32 = 0;
    // The bounding box of the non-empty cells, which is the sheet's used range.
    let mut used: Option<(u32, u32, u32, u32)> = None;
    let mut too_many = false;
    parse_records(&text, delim, quote, |rec| {
        if row >= MAX_ROWS {
            return false;
        }
        // A blank line is an empty row.
        for (col, f) in rec.into_iter().enumerate() {
            if col as u32 >= MAX_COLS {
                break;
            }
            if f.is_empty() {
                continue;
            }
            let pos = CellRef::new(row, col as u32);
            let (r0, c0, c1) = used.map_or((row, col as u32, col as u32), |(r0, c0, _, c1)| (r0, c0.min(col as u32), c1.max(col as u32)));
            used = Some((r0, c0, row, c1));
            let cell = if opts.parse_values {
                let rule = date_columns.get(col).map_or(DateRule::NoDates, |(rule, _)| *rule);
                let p = read_field(&f, opts.date_system, rule);
                let style = match p.format {
                    Some(code) => match fmt_cache.iter().find(|(c, _)| *c == code) {
                        Some((_, id)) => *id,
                        None => {
                            let id = styles.derive(StyleId::DEFAULT, |s| s.num_fmt = NumFmt::new(code));
                            fmt_cache.push((code, id));
                            id
                        }
                    },
                    None => StyleId::DEFAULT,
                };
                Cell { value: p.value, formula: None, style, py: None }
            } else {
                Cell::value(Value::Text(Arc::from(f)))
            };
            sheet.cells.set(pos, cell);
        }
        // Stop at the first record that takes the used range past the limit, before more is stored.
        if let (Some(max), Some((r0, c0, r1, c1))) = (opts.max_used_cells, used)
            && u64::from(r1 - r0 + 1) * u64::from(c1 - c0 + 1) > max
        {
            too_many = true;
            return false;
        }
        row += 1;
        true
    });
    if too_many {
        return Err(IoError::TooLarge(format!("the CSV's used range is over the limit of {} cells", opts.max_used_cells.unwrap_or(0))));
    }
    wb.styles = styles;
    wb.sheets = vec![Arc::new(sheet)];
    Ok((wb, report))
}

/// Writes the sheet as CSV from A1 to the end of its used range, so leading empty columns and rows
/// stay as empty fields and every cell keeps its position (CRLF line ends, UTF-8 without BOM). `cell_text` gives
/// each cell's field text: the caller decides how values look (Save As CSV writes them as
/// displayed, number formats applied); quoting is done here.
///
/// The output is capped at [`MAX_CSV_BYTES`]: a cell far from A1 makes the file as big as the whole
/// rectangle between them (`A1` and `XFD1048576` are 16 GiB of commas).
/// An export that would pass the cap is refused before anything is allocated, or at the first
/// field or line end that takes it past the cap (a field is measured before it is written), with
/// [`IoError::TooLarge`]; accepted exports keep every field in its place.
pub fn write_csv(sheet: &Sheet, delimiter: u8, cell_text: impl FnMut(CellRef) -> String) -> Result<Vec<u8>, IoError> {
    write_csv_limited(sheet, delimiter, MAX_CSV_BYTES, cell_text)
}

/// [`write_csv`] with the output cap `limit` (bytes) given.
pub fn write_csv_limited(sheet: &Sheet, delimiter: u8, limit: usize, mut cell_text: impl FnMut(CellRef) -> String) -> Result<Vec<u8>, IoError> {
    let delim = if delimiter == 0 { ',' } else { delimiter as char };
    let mut out = String::new();
    let Some(r) = sheet.used_range() else { return Ok(Vec::new()) };
    let too_large =
        |what: String| IoError::TooLarge(format!("the CSV would be {what}, over the limit of {} MiB; export a smaller range", limit >> 20));
    // Every row has a delimiter between its fields and a CRLF, whatever the cells hold: a lower
    // bound on the output that needs no cell text.
    let (rows, cols) = (u64::from(r.end.row) + 1, u64::from(r.end.col) + 1);
    let per_row = (cols - 1).saturating_mul(delim.len_utf8() as u64).saturating_add(2);
    let least = rows.saturating_mul(per_row);
    if least > limit as u64 {
        return Err(too_large(format!("at least {} MiB ({rows} rows by {cols} columns)", least >> 20)));
    }
    for row in 0..=r.end.row {
        for col in 0..=r.end.col {
            if col > 0 {
                out.push(delim);
            }
            let t = cell_text(CellRef::new(row, col));
            // The exact size of the field as written (the delimiter was pushed above), so a row
            // never grows past the cap unseen.
            if out.len().saturating_add(craft_csv::field_len(&t, delim)) > limit {
                return Err(too_large(format!("more than {} MiB (stopped at row {})", limit >> 20, row + 1)));
            }
            craft_csv::push_field(&mut out, &t, delim);
        }
        out.push_str("\r\n");
        if out.len() > limit {
            return Err(too_large(format!("more than {} MiB (stopped at row {})", limit >> 20, row + 1)));
        }
    }
    Ok(out.into_bytes())
}
