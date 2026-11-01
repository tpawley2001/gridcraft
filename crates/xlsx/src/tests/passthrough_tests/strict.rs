//! Pass-through for Strict Open XML (ISO/IEC 29500 Strict) packages: the unmodelled content is
//! kept and saved with Transitional relationship types and namespaces, like the rest of the
//! package. The Strict input is [`rich_package`] with every Transitional URI swapped for its
//! Strict spelling.

use super::*;

/// Transitional -> Strict, most specific first (ISO/IEC 29500-1 Annex A and the relationship
/// type tables; three relationship kinds are spelled differently in Strict).
const TO_STRICT: &[(&str, &str)] = &[
    (
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/custom-properties",
        "http://purl.oclc.org/ooxml/officeDocument/relationships/customProperties",
    ),
    (
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/customXmlProps",
        "http://purl.oclc.org/ooxml/officeDocument/relationships/customXmlProperties",
    ),
    ("http://schemas.openxmlformats.org/officeDocument/2006/relationships", "http://purl.oclc.org/ooxml/officeDocument/relationships"),
    ("http://schemas.openxmlformats.org/officeDocument/2006/custom-properties", "http://purl.oclc.org/ooxml/officeDocument/customProperties"),
    ("http://schemas.openxmlformats.org/officeDocument/2006/customXml", "http://purl.oclc.org/ooxml/officeDocument/customXml"),
    ("http://schemas.openxmlformats.org/spreadsheetml/2006/main", "http://purl.oclc.org/ooxml/spreadsheetml/main"),
    ("http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing", "http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing"),
    ("http://schemas.openxmlformats.org/drawingml/2006/main", "http://purl.oclc.org/ooxml/drawingml/main"),
];

fn to_strict(transitional: &[u8]) -> Vec<u8> {
    let u = Unpacked::new(transitional);
    let mut names: Vec<&String> = u.parts.keys().collect();
    names.sort();
    let parts: Vec<(&str, Vec<u8>)> = names
        .into_iter()
        .map(|name| {
            let data = &u.parts[name];
            let is_xml = name.ends_with(".xml") || name.ends_with(".rels") || name.ends_with(".vml");
            if !is_xml || name == "[Content_Types].xml" {
                return (name.as_str(), data.clone());
            }
            let mut s = String::from_utf8(data.clone()).unwrap();
            for (t, strict) in TO_STRICT {
                s = s.replace(t, strict);
            }
            (name.as_str(), s.into_bytes())
        })
        .collect();
    let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    make_zip(&refs)
}

#[test]
fn the_strict_fixture_is_strict() {
    let strict = to_strict(&rich_package());
    let u = Unpacked::new(&strict);
    assert!(u.rels("").iter().any(|r| r.1 == "http://purl.oclc.org/ooxml/officeDocument/relationships/officeDocument"));
    assert!(u.rels("").iter().any(|r| r.1.ends_with("/customProperties")));
    // The modelled content still reads.
    let (wb, _) = read_xlsx(&strict).unwrap();
    assert_eq!(wb.sheet(0).unwrap().comments.len(), 1);
}

#[test]
fn unmodelled_parts_of_a_strict_workbook_survive_a_round_trip_as_transitional() {
    let (wb, rep) = read_xlsx(&to_strict(&rich_package())).unwrap();
    assert!(wb.preserved.is_some(), "{:?}", rep.warnings);
    assert!(wb.sheet(0).unwrap().preserved.is_some(), "{:?}", rep.warnings);
    assert!(!rep.warnings.iter().any(|w| w.contains("dropped") || w.contains("not kept")), "{:?}", rep.warnings);

    let out = write_xlsx(&wb).unwrap();
    let u = Unpacked::new(&out);
    u.assert_valid();
    // Nothing Strict is written: every relationship type and namespace is Transitional.
    for (name, data) in &u.parts {
        let text = String::from_utf8_lossy(data);
        assert!(!text.contains("purl.oclc.org"), "{name} still has a Strict URI: {text}");
    }
    // Binary and namespace-free parts are kept byte for byte.
    for d in [PRINTER, OLE, EMF, CUSTOM_XML.as_bytes()] {
        assert_eq!(u.find(d).len(), 1, "kept part missing or duplicated: {}", String::from_utf8_lossy(d));
    }
    // Package root: custom properties, under the Transitional relationship name.
    let props = u.rels("").into_iter().find(|r| r.1.ends_with("/custom-properties")).expect("custom properties relationship");
    assert_eq!(props.1, format!("{NS_R}/custom-properties"));
    let props = u.text(&props.2);
    assert!(props.contains("http://schemas.openxmlformats.org/officeDocument/2006/custom-properties"), "{props}");
    // Workbook: the custom XML item keeps its properties part, translated.
    let item = u.find(CUSTOM_XML.as_bytes())[0].clone();
    let ip = u.rels(&item).into_iter().find(|r| r.1 == format!("{NS_R}/customXmlProps")).expect("customXmlProps relationship");
    assert!(u.text(&ip.2).contains("http://schemas.openxmlformats.org/officeDocument/2006/customXml"));
    // Workbook: connections kept.
    assert!(u.rels("xl/workbook.xml").iter().any(|r| r.1 == format!("{NS_R}/connections") && u.parts.contains_key(&r.2)));
    // Sheet: printer settings, OLE object with its preview, form control, query table.
    let sheet = sheet_part(&u, "Data");
    let sx = u.text(&sheet);
    let (ty, target) = u.target(&sheet, &attr_of(&sx, "pageSetup", "r:id"));
    assert_eq!(ty, format!("{NS_R}/printerSettings"));
    assert_eq!(u.parts.get(&target).unwrap(), PRINTER);
    assert_eq!(u.parts.get(&u.target(&sheet, &attr_of(&sx, "oleObject", "r:id")).1).unwrap(), OLE);
    assert_eq!(u.parts.get(&u.target(&sheet, &attr_of(&sx, "objectPr", "r:id")).1).unwrap(), EMF);
    let (ty, ctrl) = u.target(&sheet, &attr_of(&sx, "control", "r:id"));
    assert_eq!(ty, format!("{NS_R}/ctrlProp"));
    assert!(u.parts.contains_key(&ctrl));
    assert!(u.rels(&sheet).iter().any(|r| r.1 == format!("{NS_R}/queryTable") && u.parts.contains_key(&r.2)));
    // Schema order: pageSetup < legacyDrawing < oleObjects < controls.
    let pos = |t: &str| sx.find(t).unwrap_or_else(|| panic!("no {t} in {sx}"));
    assert!(pos("<pageSetup") < pos("<legacyDrawing") && pos("<legacyDrawing") < pos("<oleObjects") && pos("<oleObjects") < pos("<controls"));
}

#[test]
fn a_strict_round_trip_reads_back_the_same_kept_content() {
    let (wb, _) = read_xlsx(&to_strict(&rich_package())).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let (wb2, rep2) = read_xlsx(&out).unwrap();
    assert!(!rep2.warnings.iter().any(|w| w.contains("dropped")), "{:?}", rep2.warnings);
    let a = wb.sheet(0).unwrap().preserved.as_deref().expect("kept sheet content from the Strict file");
    let b = wb2.sheet(0).unwrap().preserved.as_deref().expect("kept sheet content after the round trip");
    assert_eq!(a.parts.len(), b.parts.len());
    assert_eq!(a.elements.len(), b.elements.len());
    assert_eq!(wb.preserved.as_deref().unwrap().parts.len(), wb2.preserved.as_deref().unwrap().parts.len());
}
