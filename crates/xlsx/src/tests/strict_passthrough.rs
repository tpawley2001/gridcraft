//! Unmodelled content of Strict Open XML packages (ActiveX controls, OLE objects, printer
//! settings, custom XML) is kept and saved as Transitional; what can't be translated is dropped
//! part by part. Packages are built in code: written Transitional, then respelled as Strict.

use std::collections::HashMap;
use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const ACTIVEX_BIN: &[u8] = b"\xD0\xCF\x11\xE0 ActiveX persistence";
const OLE: &[u8] = b"\xD0\xCF\x11\xE0 embedded object";
const PRINTER: &[u8] = &[7, 0, 255, 1, 128, 3];
const ITEM: &str = r#"<?xml version="1.0"?><data xmlns="urn:example:strict"><v>1</v></data>"#;

/// Transitional -> Strict, most specific first.
const TO_STRICT: &[(&str, &str)] = &[
    (
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps",
        "http://purl.oclc.org/ooxml/officeDocument/relationships/customXmlProperties",
    ),
    ("http://schemas.openxmlformats.org/officeDocument/2006/relationships", "http://purl.oclc.org/ooxml/officeDocument/relationships"),
    ("http://schemas.openxmlformats.org/officeDocument/2006/customXml", "http://purl.oclc.org/ooxml/officeDocument/customXml"),
    ("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "http://purl.oclc.org/ooxml/spreadsheetml/main"),
    ("http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing", "http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing"),
];

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        let ty = if ty.starts_with("http") { ty.to_string() } else { format!("{R}/{ty}") };
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

fn anchor(col_off: &str) -> String {
    format!(
        "<anchor moveWithCells=\"1\"><from><xdr:col>1</xdr:col><xdr:colOff>{col_off}</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></from><to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>3</xdr:row><xdr:rowOff>0</xdr:rowOff></to></anchor>"
    )
}

/// One sheet with an ActiveX button (`<controls>` in `mc:AlternateContent`), an OLE object and
/// printer settings, and a custom XML item. `item_props_ns` is the namespace of the item's
/// properties part; `control_col_off` the button's column offset.
fn package(strict: bool, item_props_ns: &str, control_col_off: &str) -> Vec<u8> {
    let content_types = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="bin" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.printerSettings"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/activeX/activeX1.xml" ContentType="application/vnd.ms-office.activeX+xml"/><Override PartName="/xl/activeX/activeX1.bin" ContentType="application/vnd.ms-office.activeX"/><Override PartName="/xl/embeddings/oleObject1.bin" ContentType="application/vnd.openxmlformats-officedocument.oleObject"/><Override PartName="/customXml/itemProps1.xml" ContentType="application/vnd.openxmlformats-officedocument.customXmlProperties+xml"/></Types>"#;
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{MAIN}" xmlns:r="{R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{MAIN}" xmlns:r="{R}" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" mc:Ignorable="x14"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup orientation="portrait" r:id="rId1"/><legacyDrawing r:id="rId2"/><oleObjects><oleObject progId="Package" shapeId="1025" r:id="rId3"/></oleObjects><mc:AlternateContent><mc:Choice Requires="x14"><controls><mc:AlternateContent><mc:Choice Requires="x14"><control shapeId="1026" r:id="rId4" name="CommandButton1"><controlPr defaultSize="0" autoLine="0">{}</controlPr></control></mc:Choice><mc:Fallback><control shapeId="1026" r:id="rId4" name="CommandButton1"/></mc:Fallback></mc:AlternateContent></controls></mc:Choice></mc:AlternateContent></worksheet>"#,
        anchor(control_col_off)
    );
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shape id="_x0000_s1025" type="#_x0000_t75" style="position:absolute"><x:ClientData ObjectType="Pict"><x:Anchor>1, 0, 5, 0, 3, 0, 7, 0</x:Anchor><x:CF>Pict</x:CF></x:ClientData></v:shape><v:shape id="_x0000_s1026" type="#_x0000_t201" style="position:absolute"><x:ClientData ObjectType="Pict"><x:Anchor>1, 0, 1, 0, 3, 0, 3, 0</x:Anchor></x:ClientData></v:shape></xml>"##;
    let activex = format!(
        r#"<?xml version="1.0"?><ax:ocx ax:classid="{{00000000-1111-2222-3333-444444444444}}" ax:persistence="persistStorage" r:id="rId1" xmlns:ax="http://schemas.microsoft.com/office/2006/activeX" xmlns:r="{R}"/>"#
    );
    let item_props =
        format!(r#"<?xml version="1.0"?><ds:datastoreItem ds:itemID="{{AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE}}" xmlns:ds="{item_props_ns}"/>"#);
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "customXml", "../customXml/item1.xml")]);
    let sheet_rels = rels(&[
        ("rId1", "printerSettings", "../printerSettings/printerSettings1.bin"),
        ("rId2", "vmlDrawing", "../drawings/vmlDrawing1.vml"),
        ("rId3", "oleObject", "../embeddings/oleObject1.bin"),
        ("rId4", "control", "../activeX/activeX1.xml"),
    ]);
    let activex_rels = rels(&[("rId1", "http://schemas.microsoft.com/office/2006/relationships/activeXControlBinary", "activeX1.bin")]);
    let item_rels = rels(&[("rId1", "customXmlProps", "itemProps1.xml")]);
    let parts: Vec<(&str, Vec<u8>)> = vec![
        ("[Content_Types].xml", content_types.as_bytes().to_vec()),
        ("_rels/.rels", root_rels.into_bytes()),
        ("xl/workbook.xml", workbook.into_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.into_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.into_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.into_bytes()),
        ("xl/drawings/vmlDrawing1.vml", vml.as_bytes().to_vec()),
        ("xl/activeX/activeX1.xml", activex.into_bytes()),
        ("xl/activeX/_rels/activeX1.xml.rels", activex_rels.into_bytes()),
        ("xl/activeX/activeX1.bin", ACTIVEX_BIN.to_vec()),
        ("xl/embeddings/oleObject1.bin", OLE.to_vec()),
        ("xl/printerSettings/printerSettings1.bin", PRINTER.to_vec()),
        ("customXml/item1.xml", ITEM.as_bytes().to_vec()),
        ("customXml/_rels/item1.xml.rels", item_rels.into_bytes()),
        ("customXml/itemProps1.xml", item_props.into_bytes()),
    ];
    let parts: Vec<(&str, Vec<u8>)> = parts
        .into_iter()
        .map(|(name, data)| {
            if !strict || name == "[Content_Types].xml" || name.ends_with(".bin") {
                return (name, data);
            }
            let mut s = String::from_utf8(data).unwrap();
            for (t, st) in TO_STRICT {
                s = s.replace(t, st);
            }
            (name, s.into_bytes())
        })
        .collect();
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    make_zip(&refs)
}

const CUSTOM_XML_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/customXml";

fn unpack(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
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

fn text<'a>(parts: &'a HashMap<String, Vec<u8>>, suffix: &str) -> (&'a str, String) {
    let (name, data) = parts.iter().find(|(n, _)| n.ends_with(suffix)).unwrap_or_else(|| panic!("no part ending {suffix}"));
    (name.as_str(), String::from_utf8_lossy(data).into_owned())
}

fn holds(parts: &HashMap<String, Vec<u8>>, data: &[u8]) -> bool {
    parts.values().any(|d| d == data)
}

#[test]
fn activex_ole_printer_and_custom_xml_of_a_strict_workbook_are_saved_as_transitional() {
    let (wb, rep) = read_xlsx(&package(true, CUSTOM_XML_NS, "0")).unwrap();
    assert!(!rep.warnings.iter().any(|w| w.contains("dropped") || w.contains("not kept")), "{:?}", rep.warnings);
    let out = unpack(&write_xlsx(&wb).unwrap());
    for (name, data) in &out {
        assert!(!String::from_utf8_lossy(data).contains("purl.oclc.org"), "{name} still has a Strict URI");
    }
    for bin in [ACTIVEX_BIN, OLE, PRINTER, ITEM.as_bytes()] {
        assert!(holds(&out, bin), "kept part missing: {}", String::from_utf8_lossy(bin));
    }
    let (_, sheet) = text(&out, "worksheets/sheet1.xml");
    assert!(sheet.contains("<control ") && sheet.contains("CommandButton1") && sheet.contains("<oleObject "), "{sheet}");
    let (_, sheet_rels) = text(&out, "worksheets/_rels/sheet1.xml.rels");
    for kind in ["control", "oleObject", "printerSettings"] {
        assert!(sheet_rels.contains(&format!("Type=\"{R}/{kind}\"")), "{kind}: {sheet_rels}");
    }
    let (_, ax) = text(&out, "activeX1.xml");
    assert!(ax.contains(&format!("xmlns:r=\"{R}\"")), "{ax}");
    let (_, ax_rels) = text(&out, "activeX1.xml.rels");
    assert!(ax_rels.contains("http://schemas.microsoft.com/office/2006/relationships/activeXControlBinary"), "{ax_rels}");
    let (_, item_rels) = text(&out, "item1.xml.rels");
    assert!(item_rels.contains(&format!("{R}/customXmlProps")), "{item_rels}");
    let (_, props) = text(&out, "itemProps1.xml");
    assert!(props.contains(CUSTOM_XML_NS), "{props}");
}

#[test]
fn the_same_transitional_workbook_keeps_its_parts_byte_for_byte() {
    let input = package(false, CUSTOM_XML_NS, "0");
    let before = unpack(&input);
    let (wb, rep) = read_xlsx(&input).unwrap();
    assert!(!rep.warnings.iter().any(|w| w.contains("dropped")), "{:?}", rep.warnings);
    let out = unpack(&write_xlsx(&wb).unwrap());
    for name in
        ["xl/activeX/activeX1.xml", "xl/activeX/activeX1.bin", "xl/embeddings/oleObject1.bin", "customXml/item1.xml", "customXml/itemProps1.xml"]
    {
        assert!(holds(&out, &before[name]), "{name} changed or missing");
    }
    assert!(holds(&out, PRINTER));
}

#[test]
fn an_unknown_strict_namespace_is_kept_unchanged_with_one_warning() {
    let ns = "http://purl.oclc.org/ooxml/officeDocument/somethingNew";
    let (wb, rep) = read_xlsx(&package(true, ns, "0")).unwrap();
    let warnings: Vec<&String> = rep.warnings.iter().filter(|w| w.contains(ns)).collect();
    assert_eq!(warnings.len(), 1, "{:?}", rep.warnings);
    assert!(warnings[0].contains("itemProps1.xml"), "{warnings:?}");
    let out = unpack(&write_xlsx(&wb).unwrap());
    let (_, props) = text(&out, "itemProps1.xml");
    assert!(props.contains(ns), "{props}");
    // Everything else is still translated and kept.
    assert!(holds(&out, ACTIVEX_BIN) && holds(&out, OLE) && holds(&out, PRINTER));
}

#[test]
fn a_part_with_strict_only_values_is_dropped_alone() {
    // A universal measure for a coordinate: Strict only, Transitional needs EMUs.
    let (wb, rep) = read_xlsx(&package(true, CUSTOM_XML_NS, "2.5in")).unwrap();
    assert!(rep.warnings.iter().any(|w| w.contains("controls were dropped") && w.contains("Strict Open XML values")), "{:?}", rep.warnings);
    let out = unpack(&write_xlsx(&wb).unwrap());
    let (_, sheet) = text(&out, "worksheets/sheet1.xml");
    assert!(!sheet.contains("<control ") && !holds(&out, ACTIVEX_BIN), "{sheet}");
    // The rest of the workbook's kept content survives.
    assert!(sheet.contains("<oleObject ") && holds(&out, OLE) && holds(&out, PRINTER), "{sheet}");
    let (_, props) = text(&out, "itemProps1.xml");
    assert!(props.contains(CUSTOM_XML_NS));
}
