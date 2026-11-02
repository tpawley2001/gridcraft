//! Synthetic OLEPS property streams and OPC parts, built from the public schemas.
use gridcraft_model::DocProps;

use super::{make_zip, xls_tests, xlsb_tests};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};

const SUMMARY: [u8; 16] = [0xe0, 0x85, 0x9f, 0xf2, 0xf9, 0x4f, 0x68, 0x10, 0xab, 0x91, 8, 0, 0x2b, 0x27, 0xb3, 0xd9];
const DOCUMENT: [u8; 16] = [2, 0xd5, 0xcd, 0xd5, 0x9c, 0x2e, 0x1b, 0x10, 0x93, 0x97, 8, 0, 0x2b, 0x2c, 0xf9, 0xae];

fn typed(kind: u32, data: &[u8]) -> Vec<u8> {
    [kind.to_le_bytes().as_slice(), data].concat()
}
fn text(kind: u32, data: &[u8]) -> Vec<u8> {
    let count = if kind == 0x1f { data.len() / 2 } else { data.len() };
    typed(kind, &[&(count as u32).to_le_bytes()[..], data].concat())
}
fn wide(kind: u32, s: &str) -> Vec<u8> {
    text(kind, &s.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect::<Vec<_>>())
}
fn stream(fmtid: [u8; 16], properties: &[(u32, Vec<u8>)]) -> Vec<u8> {
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
    let mut out = vec![0; 28];
    out[..2].copy_from_slice(&0xfffeu16.to_le_bytes());
    out[24..28].copy_from_slice(&1u32.to_le_bytes());
    out.extend(fmtid);
    out.extend(48u32.to_le_bytes());
    out.extend(set);
    out
}
fn xls(summary: Option<&[u8]>, document: Option<&[u8]>) -> Vec<u8> {
    let base = xls_tests::one_sheet_with(&[]);
    let compound = crate::cfb::Compound::open(&base).unwrap();
    let book = compound.root_stream("Workbook").unwrap().unwrap();
    let mut streams = vec![("Workbook", book.as_slice())];
    if let Some(data) = summary {
        streams.push(("\u{5}SummaryInformation", data));
    }
    if let Some(data) = document {
        streams.push(("\u{5}DocumentSummaryInformation", data));
    }
    crate::testing::compound_file(&streams)
}
fn full_xls() -> Vec<u8> {
    let mut properties = vec![(1, typed(2, &1252u16.to_le_bytes()))];
    for pid in [2, 3, 4, 5, 6, 8] {
        properties.push((pid, text(0x1e, b"Caf\xe9\0")));
    }
    properties.push((12, typed(0x40, &133233498000000000u64.to_le_bytes())));
    properties.push((13, typed(0x40, &128120832000000000u64.to_le_bytes())));
    xls(Some(&stream(SUMMARY, &properties)), Some(&stream(DOCUMENT, &[(2, text(0x1e, b"Finance\0")), (15, text(0x1e, b"Company\0"))])))
}

#[test]
fn xls_all_properties_and_1252() {
    let (wb, _) = read_xls(&full_xls()).unwrap();
    assert_eq!(
        wb.props,
        DocProps {
            title: "Café".into(),
            subject: "Café".into(),
            author: "Café".into(),
            keywords: "Café".into(),
            description: "Café".into(),
            last_modified_by: "Café".into(),
            category: "Finance".into(),
            company: "Company".into(),
            created: "2023-03-15T10:30:00Z".into(),
            modified: "2007-01-01T00:00:00Z".into(),
            ..DocProps::default()
        }
    );
}
#[test]
fn xls_unicode_codepages_and_string_types() {
    for kind in [0x1e, 0x1f] {
        let data = stream(SUMMARY, &[(2, wide(kind, "日本語 🦀")), (4, wide(kind, "Zoë")), (1, typed(2, &1200u16.to_le_bytes()))]);
        let (wb, _) = read_xls(&xls(Some(&data), None)).unwrap();
        assert_eq!(wb.props.title, "日本語 🦀");
        assert_eq!(wb.props.author, "Zoë");
    }
    for cp in [0u16, 1252, 65001] {
        let value = if cp == 65001 { "Café\0".as_bytes() } else { b"Caf\xe9\0" };
        let data = stream(SUMMARY, &[(1, typed(2, &cp.to_le_bytes())), (2, text(0x1e, value))]);
        assert_eq!(read_xls(&xls(Some(&data), None)).unwrap().0.props.title, "Café");
    }
}
#[test]
fn xls_missing_properties() {
    assert_eq!(read_xls(&xls(None, None)).unwrap().0.props, DocProps::default());
}
#[test]
fn xls_damaged_properties_are_optional() {
    let good = stream(SUMMARY, &[(2, text(0x1e, b"Title\0"))]);
    let mut bad_offset = good.clone();
    bad_offset[60..64].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut huge_count = good.clone();
    huge_count[52..56].copy_from_slice(&1001u32.to_le_bytes());
    let mut bad_order = good.clone();
    bad_order[..2].fill(0);
    let mut bad_length = good.clone();
    bad_length[68..72].copy_from_slice(&u32::MAX.to_le_bytes());
    for data in [bad_offset, huge_count, bad_order, bad_length] {
        assert_eq!(read_xls(&xls(Some(&data), None)).unwrap().0.props, DocProps::default());
    }
    for end in 0..good.len() {
        assert_eq!(read_xls(&xls(Some(&good[..end]), None)).unwrap().0.props, DocProps::default());
    }
}
#[test]
fn xls_unsupported_types_zero_dates_and_string_cap() {
    let data = stream(SUMMARY, &[(2, typed(3, &42u32.to_le_bytes())), (12, typed(0x40, &0u64.to_le_bytes())), (4, wide(0x1f, &"a".repeat(40_000)))]);
    let props = read_xls(&xls(Some(&data), None)).unwrap().0.props;
    assert!(props.title.is_empty());
    assert!(props.created.is_empty());
    assert_eq!(props.author.len(), 32_767);
}
#[test]
fn xls_save_as_xlsx_keeps_properties() {
    let (wb, _) = read_xls(&full_xls()).unwrap();
    let bytes = write_xlsx(&wb).unwrap();
    let (saved, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(saved.props, wb.props);
}
#[test]
fn xlsb_properties_use_root_relationships_and_xlsx_filters() {
    let roots = xlsb_tests::ROOT.replace("</Relationships>", r#"<Relationship Id="core" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="metadata/core.xml"/><Relationship Id="app" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="metadata/app.xml"/></Relationships>"#);
    let book = xlsb_tests::book(false);
    let sheet = xlsb_tests::worksheet(&[], &[]);
    let strings = xlsb_tests::sst();
    let data = make_zip(&[
        ("[Content_Types].xml", xlsb_tests::TYPES.as_bytes()), ("_rels/.rels", roots.as_bytes()),
        ("records/book.bin", &book), ("records/_rels/book.bin.rels", xlsb_tests::RELS.as_bytes()),
        ("data/one.bin", &sheet), ("data/two.bin", &sheet), ("text/strings.bin", &strings),
        ("metadata/core.xml", br#"<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/"><dc:title>Budget</dc:title><dc:creator>Ada</dc:creator><dcterms:created>2023-03-15T10:30:00Z</dcterms:created><dcterms:modified>invalid</dcterms:modified><dc:language>bad language!</dc:language></cp:coreProperties>"#),
        ("metadata/app.xml", br#"<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Company>Example</Company></Properties>"#),
    ]);
    let props = read_xlsb(&data).unwrap().0.props;
    assert_eq!(props.title, "Budget");
    assert_eq!(props.author, "Ada");
    assert_eq!(props.created, "2023-03-15T10:30:00Z");
    assert_eq!(props.company, "Example");
    assert!(props.modified.is_empty());
    assert!(props.language.is_empty());
}
