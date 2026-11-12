//! A SmartArt graphic in a Strict Open XML workbook (ISO/IEC 29500 Strict: namespaces under
//! `http://purl.oclc.org/ooxml/`) survives open + save: the frame, its four diagram parts, the
//! diagram drawing and the data part's picture are kept and saved as Transitional, as Excel
//! saves them. Packages are built in code from the synthetic parts of `smartart_passthrough`.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use super::smartart_passthrough::{
    COLORS, DATA, DIAGRAM_DRAWING, LAYOUT, MS_DIAGRAM_DRAWING, NS_R, PICTURE, PNG, QUICK_STYLE, SMARTART, anchor, assert_smartart_kept, data_drawing,
    unpack,
};
use crate::{read_xlsx, write_xlsx};

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_DIAGRAM: &str = "http://schemas.openxmlformats.org/drawingml/2006/diagram";
const STRICT_DIAGRAM: &str = "http://purl.oclc.org/ooxml/drawingml/diagram";
const STRICT_HOST: &str = "purl.oclc.org";

/// Transitional -> Strict (ISO/IEC 29500-1 Annex A).
const TO_STRICT: &[(&str, &str)] = &[
    ("http://schemas.openxmlformats.org/officeDocument/2006/relationships", "http://purl.oclc.org/ooxml/officeDocument/relationships"),
    ("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "http://purl.oclc.org/ooxml/spreadsheetml/main"),
    ("http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing", "http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing"),
    ("http://schemas.openxmlformats.org/drawingml/2006/main", "http://purl.oclc.org/ooxml/drawingml/main"),
    (NS_DIAGRAM, STRICT_DIAGRAM),
];

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/diagrams/data1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramData+xml"/><Override PartName="/xl/diagrams/layout1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramLayout+xml"/><Override PartName="/xl/diagrams/quickStyle1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramStyle+xml"/><Override PartName="/xl/diagrams/colors1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.diagramColors+xml"/><Override PartName="/xl/diagrams/drawing1.xml" ContentType="application/vnd.ms-office.drawingml.diagramDrawing+xml"/></Types>"#;

/// The picture only the diagram's data part uses: the 1x1 PNG with a trailing marker chunk.
fn data_picture() -> Vec<u8> {
    let mut v = PNG.to_vec();
    v.extend_from_slice(b"DATA-PICTURE-MARKER");
    v
}

/// Sheet "Data" with a SmartArt graphic whose data part has a picture of its own (and with
/// `picture`, a picture in the same drawing). `strict`: every XML part and relationship uses the
/// Strict namespaces, the `graphicData uri` and `dataModelExt minVer` included, as Excel writes
/// a Strict Open XML Spreadsheet.
fn package(strict: bool, picture: bool) -> Vec<u8> {
    let rels = |items: &[(&str, &str, &str)]| {
        let body: String = items.iter().map(|(id, ty, target)| format!(r#"<Relationship Type="{ty}" Target="{target}" Id="{id}"/>"#)).collect();
        format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#)
    };
    let root_rels = rels(&[("rId1", &format!("{NS_R}/officeDocument"), "xl/workbook.xml")]);
    let conformance = if strict { r#" conformance="strict""# } else { "" };
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"{conformance}><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = rels(&[("rId1", &format!("{NS_R}/worksheet"), "worksheets/sheet1.xml")]);
    let sheet1 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#
    );
    let sheet1_rels = rels(&[("rId1", &format!("{NS_R}/drawing"), "../drawings/drawing1.xml")]);
    let mut anchors = String::new();
    if picture {
        anchors.push_str(&anchor(0, 12, PICTURE));
    }
    anchors.push_str(&anchor(2, 1, SMARTART));
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}">{anchors}</xdr:wsDr>"#
    );
    let drawing_rels = rels(&[
        ("rId1", &format!("{NS_R}/diagramData"), "../diagrams/data1.xml"),
        ("rId2", &format!("{NS_R}/diagramLayout"), "../diagrams/layout1.xml"),
        ("rId3", &format!("{NS_R}/diagramQuickStyle"), "../diagrams/quickStyle1.xml"),
        ("rId4", &format!("{NS_R}/diagramColors"), "../diagrams/colors1.xml"),
        ("rId5", MS_DIAGRAM_DRAWING, "../diagrams/drawing1.xml"),
        ("rId6", &format!("{NS_R}/image"), "../media/image1.png"),
    ]);
    // The data part's second point is filled with a picture related from the data part.
    let blip = format!(
        r#"<dgm:prSet phldrT="[Text]"/><dgm:spPr><a:blipFill><a:blip xmlns:r="{NS_R}" r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></a:blipFill></dgm:spPr>"#
    );
    let data = DATA.replacen(r#"<dgm:prSet phldrT="[Text]"/><dgm:spPr/>"#, &blip, 1);
    assert!(data.contains("r:embed"), "the synthetic data part changed shape");
    let data_rels = rels(&[("rId1", &format!("{NS_R}/image"), "../media/image2.png")]);
    let to_strict = |x: &str| if strict { TO_STRICT.iter().fold(x.to_string(), |x, (t, s)| x.replace(t, s)) } else { x.to_string() };
    let parts: Vec<(&str, String)> = vec![
        ("_rels/.rels", to_strict(&root_rels)),
        ("xl/workbook.xml", to_strict(&workbook)),
        ("xl/_rels/workbook.xml.rels", to_strict(&wb_rels)),
        ("xl/worksheets/sheet1.xml", to_strict(&sheet1)),
        ("xl/worksheets/_rels/sheet1.xml.rels", to_strict(&sheet1_rels)),
        ("xl/drawings/drawing1.xml", to_strict(&drawing)),
        ("xl/drawings/_rels/drawing1.xml.rels", to_strict(&drawing_rels)),
        ("xl/diagrams/data1.xml", to_strict(&data)),
        ("xl/diagrams/_rels/data1.xml.rels", to_strict(&data_rels)),
        ("xl/diagrams/layout1.xml", to_strict(LAYOUT)),
        ("xl/diagrams/quickStyle1.xml", to_strict(QUICK_STYLE)),
        ("xl/diagrams/colors1.xml", to_strict(COLORS)),
        ("xl/diagrams/drawing1.xml", to_strict(DIAGRAM_DRAWING)),
    ];
    let picture2 = data_picture();
    let mut files: Vec<(&str, &[u8])> = vec![("[Content_Types].xml", CONTENT_TYPES.as_bytes())];
    files.extend(parts.iter().map(|(n, x)| (*n, x.as_bytes())));
    files.push(("xl/media/image1.png", PNG));
    files.push(("xl/media/image2.png", &picture2));
    make_zip(&files)
}

fn unpack_bytes(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut parts = HashMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).unwrap();
        let mut v = vec![];
        f.read_to_end(&mut v).unwrap();
        parts.insert(f.name().to_string(), v);
    }
    parts
}

/// The name of the saved diagram data part (the part holding the data marker).
fn data_part(p: &HashMap<String, String>) -> String {
    p.iter().find(|(_, x)| x.contains("DATA-MARKER")).map(|(n, _)| n.clone()).expect("the diagram data part is gone")
}

/// The data part's picture is still related from the data part and has its original bytes.
fn assert_data_picture_kept(out: &[u8]) {
    let p = unpack(out);
    let data = data_part(&p);
    let (dir, file) = data.rsplit_once('/').unwrap();
    let rels = p.get(&format!("{dir}/_rels/{file}.rels")).unwrap_or_else(|| panic!("{data} lost its relationships"));
    assert!(rels.contains(&format!("Type=\"{NS_R}/image\"")), "{rels}");
    assert!(p[&data].contains("r:embed=\"rId1\"") && rels.contains("Id=\"rId1\""), "the data part's picture id changed: {rels}");
    let raw = unpack_bytes(out);
    let picture = data_picture();
    assert!(raw.iter().any(|(n, d)| n.starts_with("xl/media/") && *d == picture), "the data part's picture is gone");
}

/// What Excel's own Transitional save of a Strict workbook looks like: no Strict URI is left in
/// any part, except the data part's `dataModelExt minVer`, which Excel leaves as it was.
fn assert_transitional(out: &[u8]) {
    let p = unpack(out);
    let drawing = &p[&data_drawing(&p)];
    assert!(drawing.contains(&format!("<a:graphicData uri=\"{NS_DIAGRAM}\">")), "graphicData uri: {drawing}");
    assert!(drawing.contains(&format!("xmlns:dgm=\"{NS_DIAGRAM}\"")), "dgm namespace: {drawing}");
    let data = data_part(&p);
    let min_ver = format!("minVer=\"{STRICT_DIAGRAM}\"");
    assert!(p[&data].contains(&min_ver), "minVer is kept as it was: {}", p[&data]);
    for (name, x) in &p {
        if name.ends_with(".png") {
            continue;
        }
        let x = if *name == data { x.replace(&min_ver, "") } else { x.clone() };
        assert!(!x.contains(STRICT_HOST), "{name} still has a Strict URI: {x}");
    }
}

#[test]
fn strict_smartart_is_kept_and_saved_as_transitional() {
    let (wb, _) = read_xlsx(&package(true, false)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_smartart_kept(&out);
    assert_transitional(&out);
    assert_data_picture_kept(&out);
}

#[test]
fn strict_smartart_survives_two_saves() {
    let (wb, _) = read_xlsx(&package(true, false)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (wb2, _) = read_xlsx(&out).unwrap();
    let out2 = write_xlsx(&wb2).unwrap();
    assert_smartart_kept(&out2);
    assert_transitional(&out2);
    assert_data_picture_kept(&out2);
}

#[test]
fn strict_smartart_beside_a_picture_keeps_both() {
    let (wb, _) = read_xlsx(&package(true, true)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_smartart_kept(&out);
    assert_transitional(&out);
    assert_data_picture_kept(&out);
    let p = unpack(&out);
    let drawing = &p[&data_drawing(&p)];
    assert_eq!(drawing.matches("<xdr:pic>").count(), 1, "the picture beside the SmartArt: {drawing}");
    assert_eq!(drawing.matches("<dgm:relIds").count(), 1, "one SmartArt frame: {drawing}");
}

#[test]
fn strict_smartart_is_reported_as_kept_not_skipped() {
    let (_, rep) = read_xlsx(&package(true, false)).unwrap();
    assert!(
        rep.warnings.iter().any(|w| w.contains("SmartArt graphics are not shown; they are kept for saving")),
        "expected the 'kept for saving' warning: {:?}",
        rep.warnings
    );
    assert!(!rep.warnings.iter().any(|w| w.contains("was skipped") || w.contains("dropped")), "{:?}", rep.warnings);
}

/// Control: the same workbook in Transitional form is kept today and stays kept.
#[test]
fn the_same_transitional_workbook_keeps_its_smartart() {
    let (wb, rep) = read_xlsx(&package(false, true)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    assert_smartart_kept(&out);
    assert_data_picture_kept(&out);
    assert!(!rep.warnings.iter().any(|w| w.contains("was skipped") || w.contains("dropped")), "{:?}", rep.warnings);
    let p = unpack(&out);
    assert!(p[&data_part(&p)].contains(&format!("minVer=\"{NS_DIAGRAM}\"")), "minVer unchanged");
}
