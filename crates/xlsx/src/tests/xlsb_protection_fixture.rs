//! Synthetic records from [MS-XLSB] 2.3.2, 2.4.311–312, 2.4.318 and 2.4.792–793.
use std::io::Write;

pub fn record(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut out = if id < 128 { vec![id as u8] } else { vec![(id as u8 & 127) | 128, (id >> 7) as u8] };
    let mut len = payload.len();
    loop {
        let byte = (len & 127) as u8;
        len >>= 7;
        out.push(byte | if len == 0 { 0 } else { 128 });
        if len == 0 {
            break;
        }
    }
    out.extend(payload);
    out
}

pub fn wide(text: &str) -> Vec<u8> {
    let mut out = (text.encode_utf16().count() as u32).to_le_bytes().to_vec();
    for unit in text.encode_utf16() {
        out.extend(unit.to_le_bytes());
    }
    out
}

pub fn sheet_payload(password: u16, flags: [u32; 16]) -> Vec<u8> {
    let mut out = password.to_le_bytes().to_vec();
    for flag in flags {
        out.extend(flag.to_le_bytes());
    }
    out
}

pub fn file(book_records: &[u8], sheet_records: &[u8]) -> Vec<u8> {
    let mut book = record(131, &[]);
    book.extend(record(143, &[]));
    let mut bundle = vec![0; 4];
    bundle.extend(1u32.to_le_bytes());
    bundle.extend(wide("one"));
    bundle.extend(wide("Data"));
    book.extend(record(156, &bundle));
    book.extend(record(144, &[]));
    book.extend(book_records);
    book.extend(record(132, &[]));
    let mut sheet = record(129, &[]);
    sheet.extend(record(145, &[]));
    sheet.extend(record(146, &[]));
    sheet.extend(sheet_records);
    sheet.extend(record(130, &[]));
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in [
        ("[Content_Types].xml", br#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/book.bin" ContentType="application/vnd.ms-excel.sheet.binary.macroEnabled.main"/><Override PartName="/sheet.bin" ContentType="application/vnd.ms-excel.worksheet"/></Types>"#.as_slice()),
        ("_rels/.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="book" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="book.bin"/></Relationships>"#.as_slice()),
        ("_rels/book.bin.rels", br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="one" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="sheet.bin"/></Relationships>"#.as_slice()),
        ("book.bin", book.as_slice()), ("sheet.bin", sheet.as_slice()),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
