//! Synthetic files built from the public [MS-XLS] Window1 and [MS-XLSB] BrtBookView layouts.
use super::{make_zip, xls_tests::compound};
use crate::{read_xls, read_xlsb, read_xlsx, write_xlsx};

fn xls_record(id: u16, payload: &[u8]) -> Vec<u8> {
    [id.to_le_bytes().as_slice(), &(payload.len() as u16).to_le_bytes(), payload].concat()
}

fn bof(kind: u16) -> Vec<u8> {
    let mut payload = vec![0; 16];
    payload[..2].copy_from_slice(&0x0600u16.to_le_bytes());
    payload[2..4].copy_from_slice(&kind.to_le_bytes());
    xls_record(0x0809, &payload)
}

fn window1(active: u16) -> Vec<u8> {
    let mut payload = vec![0; 18];
    payload[10..12].copy_from_slice(&active.to_le_bytes());
    payload[14..16].copy_from_slice(&1u16.to_le_bytes());
    xls_record(0x003D, &payload)
}

// Each sheet is (visibility, sheet kind); chart sheets still occupy a file index.
fn xls(sheets: &[(u8, u8)], views: &[u8]) -> Vec<u8> {
    let eof = xls_record(0x000A, &[]);
    let bodies: Vec<_> = sheets.iter().map(|&(_, kind)| [bof(if kind == 2 { 0x0020 } else { 0x0010 }), eof.clone()].concat()).collect();
    let globals = |start: usize| {
        let mut out = bof(0x0005);
        out.extend(views);
        let mut pos = start;
        for (i, &(visibility, kind)) in sheets.iter().enumerate() {
            let mut payload = (pos as u32).to_le_bytes().to_vec();
            payload.extend([visibility, kind, 1, 0, b'A' + i as u8]);
            out.extend(xls_record(0x0085, &payload));
            pos += bodies[i].len();
        }
        out.extend(&eof);
        out
    };
    let mut stream = globals(globals(0).len());
    for body in bodies {
        stream.extend(body);
    }
    compound("Workbook", &stream)
}

fn xlsb_record(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = if id < 128 { vec![id as u8] } else { vec![(id as u8 & 0x7f) | 0x80, (id >> 7) as u8] };
    assert!(payload.len() < 128);
    out.push(payload.len() as u8);
    out.extend(payload);
    out
}

fn wide(text: &str) -> Vec<u8> {
    let units: Vec<_> = text.encode_utf16().collect();
    let mut out = (units.len() as u32).to_le_bytes().to_vec();
    out.extend(units.iter().flat_map(|u| u.to_le_bytes()));
    out
}

fn book_view(active: u32) -> Vec<u8> {
    let mut payload = vec![0; 29];
    payload[24..28].copy_from_slice(&active.to_le_bytes());
    xlsb_record(158, &payload)
}

fn xlsb(sheets: &[(u32, bool)], views: &[u8]) -> Vec<u8> {
    let mut book = xlsb_record(131, &[]);
    book.extend(xlsb_record(135, &[]));
    book.extend(views);
    book.extend(xlsb_record(136, &[]));
    book.extend(xlsb_record(143, &[]));
    let mut rels = String::from(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/>"#,
    );
    for (i, &(visibility, chart)) in sheets.iter().enumerate() {
        let rel = format!("s{i}");
        let path = format!("s{i}.bin");
        let kind = if chart { "chartsheet" } else { "worksheet" };
        let mut payload = visibility.to_le_bytes().to_vec();
        payload.extend((i as u32 + 1).to_le_bytes());
        payload.extend(wide(&rel));
        payload.extend(wide(&format!("S{i}")));
        book.extend(xlsb_record(156, &payload));
        rels.push_str(&format!(
            r#"<Relationship Id="{rel}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{path}"/>"#
        ));
        types.push_str(&format!(r#"<Override PartName="/xl/{path}" ContentType="application/vnd.ms-excel.{kind}"/>"#));
    }
    book.extend(xlsb_record(144, &[]));
    book.extend(xlsb_record(132, &[]));
    rels.push_str("</Relationships>");
    types.push_str("</Types>");
    let sheet = [xlsb_record(129, &[]), xlsb_record(145, &[]), xlsb_record(146, &[]), xlsb_record(130, &[])].concat();
    let mut parts = vec![
        ("[Content_Types].xml".to_string(), types.into_bytes()),
        ("_rels/.rels".to_string(), br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.bin"/></Relationships>"#.to_vec()),
        ("xl/workbook.bin".to_string(), book),
        ("xl/_rels/workbook.bin.rels".to_string(), rels.into_bytes()),
    ];
    for i in 0..sheets.len() {
        parts.push((format!("xl/s{i}.bin"), sheet.clone()));
    }
    make_zip(&parts.iter().map(|(name, data)| (name.as_str(), data.as_slice())).collect::<Vec<_>>())
}

#[test]
fn xls_saved_active_sheet_and_xlsx_conversion() {
    let (wb, _) = read_xls(&xls(&[(0, 0), (0, 0), (0, 0)], &window1(2))).unwrap();
    assert_eq!(wb.active_sheet(), 2);
    let bytes = write_xlsx(&wb).unwrap();
    let (reopened, _) = read_xlsx(&bytes).unwrap();
    assert_eq!(reopened.active_sheet(), 2);
}

#[test]
fn xls_active_sheet_mapping_and_fallbacks() {
    for (sheets, views, expected) in [
        (vec![(0, 0), (1, 0), (0, 0)], window1(1), 0),
        (vec![(1, 0), (0, 0), (0, 0)], vec![], 1),
        (vec![(0, 0), (0, 2), (0, 0)], window1(2), 1),
        (vec![(0, 0), (0, 2), (0, 0)], window1(1), 0),
        (vec![(0, 0), (0, 0)], window1(7), 0),
        (vec![(0, 0), (2, 0)], window1(1), 0),
        (vec![(0, 0), (0, 0)], [window1(1), window1(0)].concat(), 1),
    ] {
        let (wb, _) = read_xls(&xls(&sheets, &views)).unwrap();
        assert_eq!(wb.active_sheet(), expected);
    }
    for len in 0..18 {
        let views = [xls_record(0x003D, &vec![0; len]), window1(1)].concat();
        let (wb, _) = read_xls(&xls(&[(0, 0), (0, 0)], &views)).unwrap();
        assert_eq!(wb.active_sheet(), 0, "truncated Window1 length {len}");
    }
}

#[test]
fn xlsb_active_sheet_mapping_and_fallbacks() {
    for (sheets, views, expected) in [
        (vec![(0, false), (0, false), (0, false)], book_view(1), 1),
        (vec![(0, false), (2, false), (0, false)], book_view(1), 0),
        (vec![(0, false), (0, false)], vec![], 0),
        (vec![(1, false), (0, false)], vec![], 1),
        (vec![(0, false), (1, false)], book_view(1), 0),
        (vec![(0, false), (0, true), (0, false)], book_view(2), 1),
        (vec![(0, false), (0, true), (0, false)], book_view(1), 0),
        (vec![(0, false), (0, false)], book_view(u32::MAX), 0),
        (vec![(0, false), (0, false)], [book_view(1), book_view(0)].concat(), 1),
    ] {
        let (wb, _) = read_xlsb(&xlsb(&sheets, &views)).unwrap();
        assert_eq!(wb.active_sheet(), expected);
    }
    for len in 0..29 {
        let views = [xlsb_record(158, &vec![0; len]), book_view(1)].concat();
        let (wb, _) = read_xlsb(&xlsb(&[(0, false), (0, false)], &views)).unwrap();
        assert_eq!(wb.active_sheet(), 0, "truncated BrtBookView length {len}");
    }
}
