//! Custom document properties of .xls (a user-defined OLEPS property set) and .xlsb (an OPC part),
//! built in code from the public schemas.
use gridcraft_model::Workbook;

use super::binary_doc_props::{DOCUMENT, text, typed, wide, xls};
use super::{make_zip, xlsb_tests};
use crate::xml::{self, El};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};

const USER: [u8; 16] = [5, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10, 0x93, 0x97, 8, 0, 0x2b, 0x2c, 0xf9, 0xae];
const CUSTOM: &str = "docProps/custom.xml";
const CT: &str = "application/vnd.openxmlformats-officedocument.custom-properties+xml";

fn set(properties: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut set = vec![0; 8 + properties.len() * 8];
    set[4..8].copy_from_slice(&(properties.len() as u32).to_le_bytes());
    for (i, (pid, value)) in properties.iter().enumerate() {
        let offset = set.len() as u32;
        set[8 + i * 8..12 + i * 8].copy_from_slice(&pid.to_le_bytes());
        set[12 + i * 8..16 + i * 8].copy_from_slice(&offset.to_le_bytes());
        set.extend(value);
        while !set.len().is_multiple_of(4) {
            set.push(0);
        }
    }
    let size = set.len() as u32;
    set[..4].copy_from_slice(&size.to_le_bytes());
    set
}
/// A DocumentSummaryInformation stream with the standard set and, when given, the user-defined one.
pub(super) fn stream(user: Option<&[(u32, Vec<u8>)]>) -> Vec<u8> {
    let first = set(&[(2, text(0x1e, b"Finance\0"))]);
    let mut out = vec![0; 28];
    out[..2].copy_from_slice(&0xfffeu16.to_le_bytes());
    out[24..28].copy_from_slice(&(1 + u32::from(user.is_some())).to_le_bytes());
    out.extend(DOCUMENT);
    let header = if user.is_some() { 68u32 } else { 48 };
    out.extend(header.to_le_bytes());
    if user.is_some() {
        out.extend(USER);
        out.extend((header + first.len() as u32).to_le_bytes());
    }
    out.extend(&first);
    if let Some(user) = user {
        out.extend(set(user));
    }
    out
}
/// A dictionary ([MS-OLEPS] 2.17) of code-page names.
fn dictionary(names: &[(u32, &[u8])]) -> Vec<u8> {
    let mut out = (names.len() as u32).to_le_bytes().to_vec();
    for (pid, name) in names {
        out.extend(pid.to_le_bytes());
        out.extend((name.len() as u32 + 1).to_le_bytes());
        out.extend(*name);
        out.push(0);
    }
    out
}
/// A dictionary of UTF-16 names (code page 1200), each padded to a multiple of 4 bytes.
fn wide_dictionary(names: &[(u32, &str)]) -> Vec<u8> {
    let mut out = (names.len() as u32).to_le_bytes().to_vec();
    for (pid, name) in names {
        let units: Vec<u16> = name.encode_utf16().chain([0]).collect();
        out.extend(pid.to_le_bytes());
        out.extend((units.len() as u32).to_le_bytes());
        out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
    }
    out
}
fn codepage(cp: u16) -> (u32, Vec<u8>) {
    (1, typed(2, &cp.to_le_bytes()))
}
/// 100 ns ticks since 1601-01-01 UTC.
fn filetime(year: i32, month: u32, day: u32, h: u32, m: u32, s: u32) -> u64 {
    let base = days_from_civil(1601, 1, 1);
    let days = days_from_civil(year, month, day) - base;
    let seconds = days * 86_400 + i64::from(h * 3600 + m * 60 + s);
    u64::try_from(seconds).unwrap() * 10_000_000
}
/// Days since 1970-01-01 of a proleptic Gregorian date.
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
pub(super) fn custom_part(wb: &Workbook) -> Option<Vec<u8>> {
    let p = wb.preserved.as_deref()?;
    p.parts.iter().find(|q| q.name == CUSTOM).map(|q| q.data.to_vec())
}
/// (pid, name, value element, value text) of each property.
fn properties(data: &[u8]) -> Vec<(String, String, String, String)> {
    let root: El = xml::parse(data).unwrap();
    assert_eq!(root.name, "Properties");
    root.kids("property")
        .map(|p| {
            assert_eq!(p.attr("fmtid"), Some("{D5CDD505-2E9C-101B-9397-08002B2CF9AE}"));
            let v = p.children.first().unwrap();
            (p.attr("pid").unwrap().into(), p.attr("name").unwrap().into(), v.name.clone(), v.text.clone())
        })
        .collect()
}
fn read(user: Option<&[(u32, Vec<u8>)]>) -> (Workbook, crate::ReadReport) {
    read_xls(&xls(None, Some(&stream(user)))).unwrap()
}
fn custom_warnings(report: &crate::ReadReport) -> usize {
    report.warnings.iter().filter(|w| w.contains("custom document properties")).count()
}

#[test]
fn xlsb_custom_part_is_kept_byte_for_byte() {
    let part = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="2" name="Client"><vt:lpwstr>Acme</vt:lpwstr></property><property fmtid="{D5CDD505-2E9C-101B-9397-08002B2CF9AE}" pid="3" name="Revision"><vt:i4>3</vt:i4></property></Properties>"#;
    let roots = xlsb_tests::ROOT.replace("</Relationships>", r#"<Relationship Id="cust" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties" Target="docProps/custom.xml"/></Relationships>"#);
    let types = xlsb_tests::TYPES.replace("</Types>", &format!(r#"<Override PartName="/docProps/custom.xml" ContentType="{CT}"/></Types>"#));
    let sheet = xlsb_tests::worksheet(&[], &[]);
    let data = make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", roots.as_bytes()),
        ("records/book.bin", &xlsb_tests::book(false)),
        ("records/_rels/book.bin.rels", xlsb_tests::RELS.as_bytes()),
        ("data/one.bin", &sheet),
        ("data/two.bin", &sheet),
        ("text/strings.bin", &xlsb_tests::sst()),
        ("docProps/custom.xml", part),
    ]);
    let (wb, _) = read_xlsb(&data).unwrap();
    assert_eq!(custom_part(&wb).as_deref(), Some(&part[..]));
    let saved = write_xlsx(&wb).unwrap();
    let (back, _) = read_xlsx(&saved).unwrap();
    assert_eq!(custom_part(&back).as_deref(), Some(&part[..]));
    // An .xlsx with the same part keeps the same bytes.
    let mut plain = Workbook::new();
    plain.preserved = wb.preserved.clone();
    let (xlsx, _) = read_xlsx(&write_xlsx(&plain).unwrap()).unwrap();
    assert_eq!(custom_part(&xlsx), custom_part(&back));
    assert_eq!(properties(&custom_part(&back).unwrap()).len(), 2);

    // A part with the wrong content type is not kept, with a warning; the workbook still opens.
    let wrong = data_with_type(&roots, "application/xml", part, &sheet);
    let (wb, report) = read_xlsb(&wrong).unwrap();
    assert_eq!(custom_part(&wb), None);
    assert_eq!(custom_warnings(&report), 1);
}
fn data_with_type(roots: &str, ct: &str, part: &[u8], sheet: &[u8]) -> Vec<u8> {
    let types = xlsb_tests::TYPES.replace("</Types>", &format!(r#"<Override PartName="/docProps/custom.xml" ContentType="{ct}"/></Types>"#));
    make_zip(&[
        ("[Content_Types].xml", types.as_bytes()),
        ("_rels/.rels", roots.as_bytes()),
        ("records/book.bin", &xlsb_tests::book(false)),
        ("records/_rels/book.bin.rels", xlsb_tests::RELS.as_bytes()),
        ("data/one.bin", sheet),
        ("data/two.bin", sheet),
        ("text/strings.bin", &xlsb_tests::sst()),
        ("docProps/custom.xml", part),
    ])
}

pub(super) fn all_types() -> Vec<(u32, Vec<u8>)> {
    let due = filetime(2026, 10, 31, 9, 15, 0);
    vec![
        codepage(1252),
        (0, dictionary(&[(2, b"Client"), (3, b"Revision"), (4, b"Rate"), (5, b"Approved"), (6, b"Due"), (7, b"Note")])),
        (2, text(0x1e, b"Acme\0")),
        (3, typed(3, &3i32.to_le_bytes())),
        (4, typed(5, &0.25f64.to_le_bytes())),
        (5, typed(0x0b, &[0xff, 0xff, 0, 0])),
        (6, typed(0x40, &due.to_le_bytes())),
        (7, wide(0x1f, "Größe")),
    ]
}

#[test]
fn xls_all_value_types() {
    let (wb, report) = read(Some(&all_types()));
    assert_eq!(wb.props.category, "Finance");
    assert_eq!(custom_warnings(&report), 0);
    let got = properties(&custom_part(&wb).unwrap());
    let want = [
        ("Client", "lpwstr", "Acme"),
        ("Revision", "i4", "3"),
        ("Rate", "r8", "0.25"),
        ("Approved", "bool", "true"),
        ("Due", "filetime", "2026-10-31T09:15:00Z"),
        ("Note", "lpwstr", "Größe"),
    ];
    assert_eq!(got.len(), want.len());
    for (i, ((pid, name, tag, value), (n, t, v))) in got.iter().zip(want).enumerate() {
        assert_eq!(pid, &(i + 2).to_string());
        assert_eq!((name.as_str(), tag.as_str(), value.as_str()), (n, t, v));
    }
    // Saved as .xlsx and read back, the part is unchanged.
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(custom_part(&back), custom_part(&wb));
    assert_eq!(back.props.category, "Finance");
}

#[test]
fn xls_false_and_unicode_dictionary() {
    let user = [codepage(1200), (0, wide_dictionary(&[(2, "Größe"), (3, "Off")])), (2, wide(0x1f, "x")), (3, typed(0x0b, &[0, 0, 0, 0]))];
    let got = properties(&custom_part(&read(Some(&user)).0).unwrap());
    let got: Vec<_> = got.iter().map(|(_, n, _, v)| (n.as_str(), v.as_str())).collect();
    assert_eq!(got, [("Größe", "x"), ("Off", "false")]);
}

#[test]
fn xls_names_and_values_are_escaped() {
    let user = [codepage(1252), (0, dictionary(&[(2, b"A&B <x>")])), (2, text(0x1e, b"\"q\"\0"))];
    let (wb, _) = read(Some(&user));
    let part = custom_part(&wb).unwrap();
    let got = properties(&part);
    assert_eq!((got[0].1.as_str(), got[0].3.as_str()), ("A&B <x>", "\"q\""));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(custom_part(&back), Some(part));
}

#[test]
fn xls_unknown_type_is_skipped_with_one_warning() {
    let user = [
        codepage(1252),
        (0, dictionary(&[(2, b"One"), (3, b"Clip"), (4, b"Two"), (5, b"Clip2")])),
        (2, typed(3, &1i32.to_le_bytes())),
        (3, typed(0x47, &[0; 8])),
        (4, typed(3, &2i32.to_le_bytes())),
        (5, typed(0x47, &[0; 8])),
    ];
    let (wb, report) = read(Some(&user));
    let got = properties(&custom_part(&wb).unwrap());
    let got: Vec<_> = got.iter().map(|(p, n, _, v)| (p.as_str(), n.as_str(), v.as_str())).collect();
    assert_eq!(got, [("2", "One", "1"), ("3", "Two", "2")]);
    assert_eq!(custom_warnings(&report), 1);
}

#[test]
fn xls_without_user_section_has_no_part_and_no_warning() {
    let (wb, report) = read(None);
    assert_eq!(wb.props.category, "Finance");
    assert_eq!(custom_part(&wb), None);
    assert!(wb.preserved.is_none());
    assert_eq!(custom_warnings(&report), 0);
}

#[test]
fn xls_damaged_user_section_keeps_built_in_fields() {
    // Properties without a dictionary can't be named.
    let (wb, report) = read(Some(&[codepage(1252), (2, text(0x1e, b"Acme\0"))]));
    assert_eq!(custom_part(&wb), None);
    assert_eq!(custom_warnings(&report), 1);
    assert_eq!(wb.props.category, "Finance");
    // A dictionary claiming more names than it holds.
    let mut dict = dictionary(&[(2, b"Client")]);
    dict[..4].copy_from_slice(&5u32.to_le_bytes());
    let (wb, report) = read(Some(&[codepage(1252), (0, dict), (2, text(0x1e, b"Acme\0"))]));
    assert_eq!(custom_part(&wb), None);
    assert_eq!(custom_warnings(&report), 1);
    assert_eq!(wb.props.category, "Finance");
}

#[test]
fn xls_truncated_streams_never_panic() {
    let data = stream(Some(&all_types()));
    for end in 0..data.len() {
        let (wb, _) = read_xls(&xls(None, Some(&data[..end]))).unwrap();
        if let Some(part) = custom_part(&wb) {
            assert!(properties(&part).len() <= 6);
        }
    }
    // Hostile offsets and counts.
    for at in (0..data.len().saturating_sub(4)).step_by(4) {
        let mut bad = data.clone();
        bad[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        read_xls(&xls(None, Some(&bad))).unwrap();
    }
}
