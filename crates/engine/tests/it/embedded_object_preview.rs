//! An embedded OLE object keeps the preview picture saved with it, which Excel draws (and prints)
//! when the source program isn't running. Two `.xlsx` shapes: Office 2010+ `objectPr r:id`
//! (ECMA-376 Part 1, 18.3.1.56) pointing at an image part, and an older object found only through
//! its legacy VML shape's `v:imagedata o:relid`. A preview part that is missing leaves `None`.

use std::io::Write;

use gridcraft_model::{EmbeddedObjectKind, Workbook};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake-preview-1";
const PNG2: &[u8] = b"\x89PNG\r\n\x1a\nfake-preview-2";

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, kind, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{R}/{kind}" Target="{target}"/>"#));
    }
    s + "</Relationships>"
}

/// Sheet1: objectPr preview (rId3 -> image1.png). Sheet2: VML-only preview (image2.png).
/// Sheet3: objectPr whose image relationship is missing.
fn xlsx() -> Vec<u8> {
    let anchor = format!(
        r#"<anchor moveWithCells="1"><from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></from><to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></to></anchor>"#
    );
    let ole_pr = |img: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{MAIN}" xmlns:r="{R}" xmlns:xdr="{XDR}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:x14ac="http://schemas.microsoft.com/office/spreadsheetml/2009/9/ac" mc:Ignorable="x14ac"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><legacyDrawing r:id="rId1"/><oleObjects><mc:AlternateContent><mc:Choice Requires="x14"><oleObject progId="Package" shapeId="1025" r:id="rId2"><objectPr defaultSize="0" autoPict="0" r:id="{img}">{anchor}</objectPr></oleObject></mc:Choice><mc:Fallback><oleObject progId="Package" shapeId="1025" r:id="rId2"/></mc:Fallback></mc:AlternateContent></oleObjects></worksheet>"#
        )
    };
    let sheet1 = ole_pr("rId3");
    let sheet3 = ole_pr("rId9");
    let sheet2 = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="{MAIN}" xmlns:r="{R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><legacyDrawing r:id="rId1"/><oleObjects><oleObject progId="Word.Document.8" shapeId="1025" r:id="rId2"/></oleObjects></worksheet>"#
    );
    let vml = |relid: &str| {
        format!(
            r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shapetype id="_x0000_t75" coordsize="21600,21600" o:spt="75" o:preferrelative="t" path="m@4@5l@4@11@9@11@9@5xe" filled="f" stroked="f"/><v:shape id="_x0000_s1025" type="#_x0000_t75" style="position:absolute;margin-left:48pt;margin-top:15pt;width:144pt;height:105pt;z-index:1"><v:imagedata o:relid="{relid}" o:title=""/><x:ClientData ObjectType="Pict"><x:SizeWithCells/><x:Anchor>1, 0, 1, 0, 4, 0, 8, 0</x:Anchor><x:CF>Pict</x:CF></x:ClientData></v:shape></xml>"#
        )
    };
    let workbook = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="Pr" sheetId="1" r:id="rId1"/><sheet name="Vml" sheetId="2" r:id="rId2"/><sheet name="Missing" sheetId="3" r:id="rId3"/></sheets></workbook>"#
    );
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "worksheet", "worksheets/sheet2.xml"), ("rId3", "worksheet", "worksheets/sheet3.xml")]);
    let sheet_rels = |n: u32, img: Option<&str>| {
        let mut v = vec![("rId1", "vmlDrawing".to_string(), format!("../drawings/vmlDrawing{n}.vml")), ("rId2", "oleObject".to_string(), format!("../embeddings/oleObject{n}.bin"))];
        if let Some(i) = img {
            v.push(("rId3", "image".to_string(), format!("../media/{i}")));
        }
        let items: Vec<(&str, &str, &str)> = v.iter().map(|(a, b, c)| (*a, b.as_str(), c.as_str())).collect();
        rels(&items)
    };
    let types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet3.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;
    let parts: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".into(), types.as_bytes().to_vec()),
        ("_rels/.rels".into(), rels(&[("rId1", "officeDocument", "xl/workbook.xml")]).into_bytes()),
        ("xl/workbook.xml".into(), workbook.into_bytes()),
        ("xl/_rels/workbook.xml.rels".into(), wb_rels.into_bytes()),
        ("xl/worksheets/sheet1.xml".into(), sheet1.into_bytes()),
        ("xl/worksheets/sheet2.xml".into(), sheet2.into_bytes()),
        ("xl/worksheets/sheet3.xml".into(), sheet3.into_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels".into(), sheet_rels(1, Some("image1.png")).into_bytes()),
        ("xl/worksheets/_rels/sheet2.xml.rels".into(), sheet_rels(2, None).into_bytes()),
        ("xl/worksheets/_rels/sheet3.xml.rels".into(), sheet_rels(3, None).into_bytes()),
        ("xl/drawings/vmlDrawing1.vml".into(), vml("rId1").into_bytes()),
        ("xl/drawings/vmlDrawing2.vml".into(), vml("rId1").into_bytes()),
        ("xl/drawings/vmlDrawing3.vml".into(), vml("rId7").into_bytes()),
        ("xl/drawings/_rels/vmlDrawing1.vml.rels".into(), rels(&[("rId1", "image", "../media/image1.png")]).into_bytes()),
        ("xl/drawings/_rels/vmlDrawing2.vml.rels".into(), rels(&[("rId1", "image", "../media/image2.png")]).into_bytes()),
        ("xl/embeddings/oleObject1.bin".into(), b"ole1".to_vec()),
        ("xl/embeddings/oleObject2.bin".into(), b"ole2".to_vec()),
        ("xl/embeddings/oleObject3.bin".into(), b"ole3".to_vec()),
        ("xl/media/image1.png".into(), PNG.to_vec()),
        ("xl/media/image2.png".into(), PNG2.to_vec()),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (path, bytes) in parts {
        z.start_file(path, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(&bytes).unwrap();
    }
    z.finish().unwrap().into_inner()
}

fn book() -> Workbook {
    gridcraft_xlsx::read_xlsx(&xlsx()).unwrap().0
}

#[test]
fn object_pr_preview_is_read() {
    let wb = book();
    let objs = &wb.sheets[0].embedded_objects;
    assert_eq!(objs.len(), 1);
    assert_eq!(objs[0].kind, EmbeddedObjectKind::Ole);
    assert!(objs[0].print);
    let p = objs[0].preview.as_ref().expect("objectPr preview");
    assert_eq!(p.data.as_slice(), PNG);
    assert_eq!(p.mime, "image/png");
}

#[test]
fn legacy_vml_imagedata_preview_is_read() {
    let wb = book();
    let objs = &wb.sheets[1].embedded_objects;
    assert_eq!(objs.len(), 1);
    let p = objs[0].preview.as_ref().expect("VML imagedata preview");
    assert_eq!(p.data.as_slice(), PNG2);
    assert_eq!(p.mime, "image/png");
}

#[test]
fn a_missing_preview_part_leaves_the_object_in_place_without_one() {
    let wb = book();
    let objs = &wb.sheets[2].embedded_objects;
    assert_eq!(objs.len(), 1, "the object keeps its place");
    assert_eq!(objs[0].preview, None);
}

#[test]
fn previews_survive_save_and_reopen() {
    let wb = book();
    let (back, _) = gridcraft_xlsx::read_xlsx(&gridcraft_xlsx::write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].embedded_objects[0].preview.as_ref().map(|p| p.data.as_slice()), Some(PNG));
    assert_eq!(back.sheets[1].embedded_objects[0].preview.as_ref().map(|p| p.data.as_slice()), Some(PNG2));
}
