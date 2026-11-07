//! .xls document properties in 8-bit code pages other than 1252, and linked custom properties
//! ("Link to content"), built in code from [MS-OLEPS] and [MS-OSHARED].
use gridcraft_model::Workbook;

use super::binary_custom_props_tests::{custom_part, stream as user_stream};
use super::binary_doc_props::{SUMMARY, stream, text, typed, wide, xls};
use crate::xml::{self, El};
use crate::{read_xls, read_xlsx, write_xlsx};

const LINK: u32 = 0x0100_0000;

fn codepage(cp: u16) -> (u32, Vec<u8>) {
    (1, typed(2, &cp.to_le_bytes()))
}
/// A dictionary ([MS-OLEPS] 2.17) of 8-bit names, each with its terminating NUL.
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
fn read(user: &[(u32, Vec<u8>)]) -> (Workbook, crate::ReadReport) {
    read_xls(&xls(None, Some(&user_stream(Some(user))))).unwrap()
}
fn custom_warnings(report: &crate::ReadReport) -> Vec<String> {
    report.warnings.iter().filter(|w| w.contains("custom document properties")).cloned().collect()
}
/// (pid, name, linkTarget, value element, value text) of each property.
fn properties(wb: &Workbook) -> Vec<(String, String, Option<String>, String, String)> {
    let part = custom_part(wb).expect("custom properties part");
    let root: El = xml::parse(&part).unwrap();
    root.kids("property")
        .map(|p| {
            let v = p.children.first().unwrap();
            (p.attr("pid").unwrap().into(), p.attr("name").unwrap().into(), p.attr("linkTarget").map(Into::into), v.name.clone(), v.text.clone())
        })
        .collect()
}

#[test]
fn code_page_1250_names_and_values_decode() {
    // "Odděléní" and "Martina Benešová | Actum" in Windows-1250.
    let user = [
        codepage(1250),
        (0, dictionary(&[(2, b"_AuthorEmailDisplayName"), (3, b"Odd\xecl\xe9n\xed")])),
        (2, text(0x1e, b"Martina Bene\x9aov\xe1 | Actum\0")),
        (3, typed(3, &7i32.to_le_bytes())),
    ];
    let (wb, report) = read(&user);
    assert!(custom_warnings(&report).is_empty(), "{:?}", report.warnings);
    let got = properties(&wb);
    let got: Vec<_> = got.iter().map(|(p, n, _, t, v)| (p.as_str(), n.as_str(), t.as_str(), v.as_str())).collect();
    assert_eq!(got, [("2", "_AuthorEmailDisplayName", "lpwstr", "Martina Benešová | Actum"), ("3", "Odděléní", "i4", "7")]);
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(custom_part(&back), custom_part(&wb));
}

#[test]
fn other_supported_code_pages_decode_through_the_shared_tables() {
    // 0xC0: Cyrillic A (1251), Greek capital iota with dialytika (1253), A grave (1254); 0x8E Mac Roman e acute.
    for (cp, byte, want) in [(1251u16, 0xc0u8, "\u{410}"), (1253, 0xc0, "\u{390}"), (1254, 0xc0, "\u{C0}"), (10000, 0x8e, "\u{E9}")] {
        let user = [codepage(cp), (0, dictionary(&[(2, &[b'N', byte])])), (2, text(0x1e, &[b'v', byte, 0]))];
        let (wb, report) = read(&user);
        assert!(custom_warnings(&report).is_empty(), "code page {cp}: {:?}", report.warnings);
        let got = properties(&wb);
        assert_eq!((got[0].1.as_str(), got[0].4.as_str()), (format!("N{want}").as_str(), format!("v{want}").as_str()), "code page {cp}");
    }
}

#[test]
fn ascii_text_in_an_unsupported_double_byte_code_page_is_kept() {
    let user = [codepage(936), (0, dictionary(&[(2, b"KSOProductBuildVer")])), (2, text(0x1e, b"2052-10.1.0.7698\0"))];
    let (wb, report) = read(&user);
    assert!(custom_warnings(&report).is_empty(), "{:?}", report.warnings);
    let got = properties(&wb);
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].1.as_str(), got[0].3.as_str(), got[0].4.as_str()), ("KSOProductBuildVer", "lpwstr", "2052-10.1.0.7698"));
}

#[test]
fn office_reserved_blob_in_code_page_936_gives_no_part_and_no_warning() {
    let blob = typed(0x41, &[&8u32.to_le_bytes()[..], &[1, 2, 3, 4, 5, 6, 7, 8]].concat());
    let (wb, report) = read(&[codepage(936), (0, dictionary(&[(2, b"_PID_HLINKS")])), (2, blob)]);
    assert!(custom_warnings(&report).is_empty(), "{:?}", report.warnings);
    assert_eq!(custom_part(&wb), None);
}

#[test]
fn undecodable_text_in_an_unsupported_code_page_drops_only_that_property() {
    let user = [
        codepage(936),
        (0, dictionary(&[(2, b"Plain"), (3, b"\xc4\xe3\xba\xc3"), (4, b"Value")])),
        (2, typed(3, &1i32.to_le_bytes())),
        (3, typed(3, &2i32.to_le_bytes())),
        (4, text(0x1e, b"\xd6\xd0\0")),
    ];
    let (wb, report) = read(&user);
    let got = properties(&wb);
    let got: Vec<_> = got.iter().map(|(p, n, _, _, v)| (p.as_str(), n.as_str(), v.as_str())).collect();
    assert_eq!(got, [("2", "Plain", "1")]);
    assert_eq!(custom_warnings(&report), ["Some custom document properties have a type that can't be kept and were dropped."]);
}

#[test]
fn built_in_text_fields_decode_in_code_page_1250() {
    // Title "Škoda", author "Říční".
    let data = stream(SUMMARY, &[(1, typed(2, &1250u16.to_le_bytes())), (2, text(0x1e, b"\x8akoda\0")), (4, text(0x1e, b"\xd8\xed\xe8n\xed\0"))]);
    let wb = read_xls(&xls(Some(&data), None)).unwrap().0;
    assert_eq!(wb.props.title, "Škoda");
    assert_eq!(wb.props.author, "Říční");
}

#[test]
fn linked_properties_keep_their_link_source() {
    let user = [
        codepage(1252),
        (0, dictionary(&[(2, b"Ind_Nao"), (3, b"Ind_Sim"), (4, b"Plain")])),
        (2, typed(5, &0.0f64.to_le_bytes())),
        (LINK | 2, text(0x1e, b"Prop_Ind_Nao\0")),
        (3, typed(5, &1.5f64.to_le_bytes())),
        (LINK | 3, wide(0x1f, "Prop_Ind_Sim")),
        (4, typed(3, &4i32.to_le_bytes())),
    ];
    let (wb, report) = read(&user);
    assert!(custom_warnings(&report).is_empty(), "{:?}", report.warnings);
    let got = properties(&wb);
    let got: Vec<_> = got.iter().map(|(p, n, l, t, v)| (p.as_str(), n.as_str(), l.as_deref(), t.as_str(), v.as_str())).collect();
    assert_eq!(
        got,
        [("2", "Ind_Nao", Some("Prop_Ind_Nao"), "r8", "0"), ("3", "Ind_Sim", Some("Prop_Ind_Sim"), "r8", "1.5"), ("4", "Plain", None, "i4", "4"),]
    );
    // Saved as .xlsx and read back, the links are still there.
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(custom_part(&back), custom_part(&wb));
}

#[test]
fn link_entry_without_its_property_is_ignored_silently() {
    let user = [codepage(1252), (0, dictionary(&[(2, b"Kept")])), (2, text(0x1e, b"x\0")), (LINK | 9, text(0x1e, b"Orphan\0"))];
    let (wb, report) = read(&user);
    assert!(custom_warnings(&report).is_empty(), "{:?}", report.warnings);
    let got = properties(&wb);
    assert_eq!(got.len(), 1);
    assert_eq!((got[0].1.as_str(), got[0].2.as_deref()), ("Kept", None));
}

#[test]
fn escaped_link_target() {
    let user = [codepage(1252), (0, dictionary(&[(2, b"P")])), (2, typed(3, &1i32.to_le_bytes())), (LINK | 2, text(0x1e, b"A&B\"<\0"))];
    let (wb, _) = read(&user);
    assert_eq!(properties(&wb)[0].2.as_deref(), Some("A&B\"<"));
    let part = String::from_utf8(custom_part(&wb).unwrap()).unwrap();
    assert!(part.contains("linkTarget=\"A&amp;B&quot;&lt;\""), "{part}");
}
