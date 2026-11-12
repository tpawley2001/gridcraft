//! A drawing anchor that only points at a legacy VML shape (`com14:compatSp`, the DrawingML
//! placeholder of an OLE object or control) is saved only while the shape it names is saved too:
//! an anchor naming a shape the file no longer has makes Excel repair the file.

use super::{CT, make_zip, minimal};
use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// A plain rectangle anchor and an OLE placeholder anchor naming VML shape `_x0000_s1025`.
fn drawing() -> String {
    concat!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
        r#"<xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>3</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>"#,
        r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="Start Button"/><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1219200" cy="381000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:sp><xdr:clientData/></xdr:twoCellAnchor>"#,
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" Requires="a14">"#,
        r#"<xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>5</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>5</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>"#,
        r#"<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="1025" name="Object 1" hidden="1"/><xdr:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></xdr:cNvGraphicFramePr></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm>"#,
        r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/drawing/2010/compatibility"><com14:compatSp xmlns:com14="http://schemas.microsoft.com/office/drawing/2010/compatibility" spid="_x0000_s1025"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:twoCellAnchor>"#,
        r#"</mc:Choice><mc:Fallback/></mc:AlternateContent></xdr:wsDr>"#
    )
    .to_string()
}

const DRAWING_CT: &str =
    r#"<Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/>"#;

/// Rebuilds `base` with `[Content_Types].xml` replaced by `ct`.
fn with_content_types(base: &[u8], ct: &str) -> Vec<u8> {
    let mut zip = Package::open(base).unwrap();
    let mut files: Vec<(String, Vec<u8>)> = vec![("[Content_Types].xml".into(), ct.as_bytes().to_vec())];
    for n in zip.names() {
        if n != "[Content_Types].xml" {
            let data = zip.read(&n).unwrap().unwrap();
            files.push((n, data));
        }
    }
    make_zip(&files.iter().map(|(n, d)| (n.as_str(), d.as_slice())).collect::<Vec<_>>())
}

fn drawings_of(out: &[u8]) -> String {
    let mut pkg = Package::open(out).unwrap();
    let mut all = String::new();
    for n in pkg.names() {
        if n.starts_with("xl/drawings/") && n.ends_with(".xml") && !n.contains("_rels") {
            all.push_str(&String::from_utf8(pkg.read(&n).unwrap().unwrap()).unwrap());
        }
    }
    all
}

/// The sheet keeps no `<oleObjects>`, `<controls>` or VML drawing, so the placeholder names a
/// shape the saved file doesn't have: it is dropped (with a warning), the rectangle stays.
#[test]
fn an_orphan_compat_sp_anchor_is_dropped_on_save() {
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
    let d = drawing();
    let base = minimal(
        r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", &rels), ("xl/drawings/drawing1.xml", &d)],
        "",
        "",
    );
    let bytes = with_content_types(&base, &CT.replace("</Types>", &format!("{DRAWING_CT}</Types>")));
    let (wb, report) = read_xlsx(&bytes).unwrap();
    assert!(
        report.warnings.iter().any(|w| w.contains("markup-compatibility") && w.contains("dropped")),
        "the dropped placeholder is reported: {:?}",
        report.warnings
    );
    let out = write_xlsx(&wb).unwrap();
    let drawings = drawings_of(&out);
    assert!(!drawings.contains("compatSp"), "no placeholder for a shape that isn't saved: {drawings}");
    assert!(drawings.contains("Start Button"), "the rectangle beside it is kept: {drawings}");
    // And the result reads back and saves the same way again.
    let (wb2, _) = read_xlsx(&out).unwrap();
    let again = drawings_of(&write_xlsx(&wb2).unwrap());
    assert!(!again.contains("compatSp"), "{again}");
    assert!(again.contains("Start Button"), "{again}");
}

/// The same placeholder beside a kept OLE object whose VML shape `_x0000_s1025` is saved: the
/// placeholder, the `<oleObjects>` entry and the shape all survive.
#[test]
fn a_compat_sp_anchor_whose_ole_object_is_kept_survives() {
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/drawing" Target="../drawings/drawing1.xml"/><Relationship Id="rId2" Type="{REL}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/><Relationship Id="rId3" Type="{REL}/oleObject" Target="../embeddings/oleObject1.bin"/></Relationships>"#
    );
    let vml = concat!(
        r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout>"#,
        r##"<v:shape id="_x0000_s1025" type="#_x0000_t75" style="position:absolute;margin-left:300pt;margin-top:15pt;width:96pt;height:60pt"><x:ClientData ObjectType="Pict"><x:Anchor>5, 0, 1, 0, 7, 0, 5, 0</x:Anchor><x:CF>Pict</x:CF></x:ClientData></v:shape></xml>"##
    );
    let d = drawing();
    let base = minimal(
        r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/><legacyDrawing r:id="rId2"/><oleObjects><oleObject progId="Package" shapeId="1025" r:id="rId3"/></oleObjects>"#,
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", &rels),
            ("xl/drawings/drawing1.xml", &d),
            ("xl/drawings/vmlDrawing1.vml", vml),
            ("xl/embeddings/oleObject1.bin", "not really an OLE compound file"),
        ],
        "",
        "",
    );
    let ct = CT.replace(
        "</Types>",
        &format!(
            r#"<Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/>{DRAWING_CT}</Types>"#
        ),
    );
    let bytes = with_content_types(&base, &ct);
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let drawings = drawings_of(&out);
    assert_eq!(drawings.matches("compatSp").count(), 1, "the placeholder of the kept object stays: {drawings}");
    assert!(drawings.contains("spid=\"_x0000_s1025\""), "{drawings}");
    assert!(drawings.contains("Start Button"), "{drawings}");
    let mut pkg = Package::open(&out).unwrap();
    let sheet = String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap();
    assert!(sheet.contains("<oleObject ") && sheet.contains("shapeId=\"1025\""), "the OLE object is kept: {sheet}");
    let names = pkg.names();
    let vml_name = names.iter().find(|n| n.ends_with(".vml")).expect("the VML drawing is kept").clone();
    let vml = String::from_utf8(pkg.read(&vml_name).unwrap().unwrap()).unwrap();
    assert!(vml.contains("_x0000_s1025"), "the shape the placeholder names is saved: {vml}");
}
