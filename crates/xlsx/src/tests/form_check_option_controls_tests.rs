//! Form check boxes and option buttons: read from the properties part or the VML, with their
//! checked state, caption and linked cell, and written back as Excel writes them.
//!
//! The kinds are compared by name (`CheckBox`, `OptionButton`) so the tests compile before the
//! model has them.

use super::minimal;
use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";

fn kind(c: &gridcraft_model::FormControl) -> String {
    format!("{:?}", c.kind)
}

/// A sheet with two listed controls (properties parts) and one VML-only control.
fn book_bytes() -> Vec<u8> {
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="{REL}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/><Relationship Id="rIdA" Type="{REL}/ctrlProp" Target="../ctrlProps/ctrlProp1.xml"/><Relationship Id="rIdB" Type="{REL}/ctrlProp" Target="../ctrlProps/ctrlProp2.xml"/></Relationships>"#
    );
    let prop_a = format!(
        r#"<?xml version="1.0"?><formControlPr xmlns="{X14}" objectType="CheckBox" checked="Checked" fmlaLink="$B$2" lockText="1" noThreeD="1"/>"#
    );
    let prop_b = format!(r#"<?xml version="1.0"?><formControlPr xmlns="{X14}" objectType="Radio" checked="Checked" fmlaLink="$B$5" lockText="1"/>"#);
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout>
<v:shape id="_x0000_s1025" type="#_x0000_t201" stroked="f" filled="f"><v:textbox><div>Paid</div></v:textbox><x:ClientData ObjectType="Checkbox"><x:Anchor>2, 0, 1, 0, 4, 0, 2, 0</x:Anchor><x:FmlaLink>$B$2</x:FmlaLink><x:Checked>1</x:Checked><x:NoThreeD/></x:ClientData></v:shape>
<v:shape id="_x0000_s1026" type="#_x0000_t201" stroked="f" filled="f"><v:textbox><div>Small</div></v:textbox><x:ClientData ObjectType="Radio"><x:Anchor>2, 0, 4, 0, 4, 0, 5, 0</x:Anchor><x:FmlaLink>$B$5</x:FmlaLink><x:Checked>1</x:Checked></x:ClientData></v:shape>
<v:shape id="_x0000_s1027" type="#_x0000_t201" stroked="f" filled="f"><v:textbox><div>Maybe</div></v:textbox><x:ClientData ObjectType="Checkbox"><x:Anchor>2, 0, 7, 0, 4, 0, 8, 0</x:Anchor><x:FmlaLink>Other!$C$3</x:FmlaLink><x:Checked>2</x:Checked></x:ClientData></v:shape>
</xml>"##;
    let controls = r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" Requires="x14"><controls><mc:AlternateContent><mc:Choice Requires="x14"><control shapeId="1025" r:id="rIdA" name="Check Box 1"><controlPr defaultSize="0" autoFill="0" autoLine="0" autoPict="0"><anchor moveWithCells="1"><from><xdr:col xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">2</xdr:col><xdr:colOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:colOff><xdr:row xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">1</xdr:row><xdr:rowOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:rowOff></from><to><xdr:col xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">4</xdr:col><xdr:colOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:colOff><xdr:row xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">2</xdr:row><xdr:rowOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:rowOff></to></anchor></controlPr></control></mc:Choice></mc:AlternateContent><mc:AlternateContent><mc:Choice Requires="x14"><control shapeId="1026" r:id="rIdB" name="Option Button 2"><controlPr defaultSize="0" autoFill="0" autoLine="0" autoPict="0"><anchor moveWithCells="1"><from><xdr:col xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">2</xdr:col><xdr:colOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:colOff><xdr:row xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">4</xdr:row><xdr:rowOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:rowOff></from><to><xdr:col xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">4</xdr:col><xdr:colOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:colOff><xdr:row xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">5</xdr:row><xdr:rowOff xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing">0</xdr:rowOff></to></anchor></controlPr></control></mc:Choice></mc:AlternateContent></controls></mc:Choice></mc:AlternateContent>"#;
    minimal(
        &format!(r#"<sheetData/><legacyDrawing r:id="rId9"/>{controls}"#),
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", &rels),
            ("xl/drawings/vmlDrawing1.vml", vml),
            ("xl/ctrlProps/ctrlProp1.xml", &prop_a),
            ("xl/ctrlProps/ctrlProp2.xml", &prop_b),
        ],
        "",
        "",
    )
}

#[test]
fn check_boxes_and_option_buttons_are_read_with_state_caption_and_link() {
    let (wb, _) = read_xlsx(&book_bytes()).unwrap();
    let c = &wb.sheet(0).unwrap().controls;
    assert_eq!(c.len(), 3, "two listed controls and one VML-only check box: {c:?}");
    let by_text = |t: &str| c.iter().find(|x| x.text == t).unwrap_or_else(|| panic!("no control captioned {t}: {c:?}"));
    let paid = by_text("Paid");
    assert_eq!((kind(paid).as_str(), paid.value, paid.link.as_deref()), ("CheckBox", 1, Some("$B$2")));
    assert!(format!("{paid:?}").contains("three_d: false"), "noThreeD=\"1\" -> three_d false: {paid:?}");
    let small = by_text("Small");
    assert_eq!((kind(small).as_str(), small.value, small.link.as_deref()), ("OptionButton", 1, Some("$B$5")));
    assert!(format!("{small:?}").contains("three_d: true"), "{small:?}");
    let maybe = by_text("Maybe");
    assert_eq!((kind(maybe).as_str(), maybe.value, maybe.link.as_deref()), ("CheckBox", 2, Some("Other!$C$3")), "VML Checked 2 = mixed");
    assert_eq!(paid.anchor.cell, gridcraft_core::CellRef::parse("C2").unwrap());
}

#[test]
fn check_boxes_and_option_buttons_are_written_as_excel_writes_them() {
    let (wb, _) = read_xlsx(&book_bytes()).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&out).unwrap();
    let names = pkg.names();
    let mut props = String::new();
    for n in names.iter().filter(|n| n.starts_with("xl/ctrlProps/")) {
        props.push_str(&String::from_utf8(pkg.read(n).unwrap().unwrap()).unwrap());
        props.push('\n');
    }
    assert_eq!(props.matches("objectType=\"CheckBox\"").count(), 2, "{props}");
    assert_eq!(props.matches("objectType=\"Radio\"").count(), 1, "{props}");
    assert_eq!(props.matches("checked=\"Checked\"").count(), 2, "{props}");
    assert_eq!(props.matches("checked=\"Mixed\"").count(), 1, "{props}");
    assert_eq!(props.matches("noThreeD=\"1\"").count(), 1, "{props}");
    assert!(props.contains("fmlaLink=\"$B$2\"") && props.contains("fmlaLink=\"$B$5\""), "{props}");
    let vml_name = names.iter().find(|n| n.starts_with("xl/drawings/vmlDrawing")).expect("a VML drawing");
    let vml = String::from_utf8(pkg.read(vml_name).unwrap().unwrap()).unwrap();
    assert_eq!(vml.matches("ObjectType=\"Checkbox\"").count(), 2, "each check box once: {vml}");
    assert_eq!(vml.matches("ObjectType=\"Radio\"").count(), 1, "{vml}");
    assert!(vml.contains("<x:Checked>2</x:Checked>"), "{vml}");
    assert_eq!(vml.matches("<x:NoThreeD/>").count(), 1, "{vml}");
}

#[test]
fn a_saved_file_reads_back_the_same_controls() {
    let (wb, _) = read_xlsx(&book_bytes()).unwrap();
    let (back, report) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let key = |w: &gridcraft_model::Workbook| {
        let mut v: Vec<(String, String, u32, Option<String>)> =
            w.sheet(0).unwrap().controls.iter().map(|c| (kind(c), c.text.clone(), c.value, c.link.clone())).collect();
        v.sort();
        v
    };
    assert_eq!(key(&back), key(&wb));
    assert_eq!(back.sheet(0).unwrap().controls.len(), 3);
}

#[test]
fn an_unchecked_check_box_omits_checked() {
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="{REL}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#
    );
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shape id="_x0000_s1025" type="#_x0000_t201"><v:textbox><div>Off</div></v:textbox><x:ClientData ObjectType="CHECKBOX"><x:Anchor>1, 0, 1, 0, 3, 0, 2, 0</x:Anchor><x:Checked>7</x:Checked></x:ClientData></v:shape></xml>"##;
    let bytes = minimal(
        r#"<sheetData/><legacyDrawing r:id="rId9"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", &rels), ("xl/drawings/vmlDrawing1.vml", vml)],
        "",
        "",
    );
    let (wb, _) = read_xlsx(&bytes).unwrap();
    let c = &wb.sheet(0).unwrap().controls;
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!((kind(&c[0]).as_str(), c[0].value, c[0].link.as_deref()), ("CheckBox", 0, None), "an out-of-range state reads as unchecked");
    let out = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&out).unwrap();
    let prop = String::from_utf8(pkg.read("xl/ctrlProps/ctrlProp1.xml").unwrap().expect("ctrlProp1")).unwrap();
    assert!(prop.contains("objectType=\"CheckBox\"") && !prop.contains("checked="), "{prop}");
}
