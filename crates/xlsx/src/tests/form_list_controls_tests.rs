//! Form list boxes, drop-downs, group boxes and labels: read from the properties part or the VML
//! with their item range, selected item, selection type and drop-down lines, and written back as
//! Excel writes them, each control once.
//!
//! The kinds are compared by name and the new fields by their `Debug` text, so the tests compile
//! before the model has them.

use super::minimal;
use crate::package::Package;
use crate::{read_xlsx, write_xlsx};

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";

fn kind(c: &gridcraft_model::FormControl) -> String {
    format!("{:?}", c.kind)
}

/// One `<control>` with its anchor (zero-based columns and rows).
fn control(shape: u32, rid: &str, name: &str, (c1, r1, c2, r2): (u32, u32, u32, u32)) -> String {
    let m = |tag: &str, c: u32, r: u32| {
        format!(
            "<{tag}><xdr:col xmlns:xdr=\"{XDR}\">{c}</xdr:col><xdr:colOff xmlns:xdr=\"{XDR}\">0</xdr:colOff><xdr:row xmlns:xdr=\"{XDR}\">{r}</xdr:row><xdr:rowOff xmlns:xdr=\"{XDR}\">0</xdr:rowOff></{tag}>"
        )
    };
    format!(
        "<mc:AlternateContent><mc:Choice Requires=\"x14\"><control shapeId=\"{shape}\" r:id=\"{rid}\" name=\"{name}\"><controlPr defaultSize=\"0\" autoLine=\"0\"><anchor moveWithCells=\"1\">{}{}</anchor></controlPr></control></mc:Choice></mc:AlternateContent>",
        m("from", c1, r1),
        m("to", c2, r2)
    )
}

/// A sheet with a list box and a drop-down (properties parts) and a VML-only group box, label
/// and multi-selection list box.
fn book_bytes() -> Vec<u8> {
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="{REL}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/><Relationship Id="rIdA" Type="{REL}/ctrlProp" Target="../ctrlProps/ctrlProp1.xml"/><Relationship Id="rIdB" Type="{REL}/ctrlProp" Target="../ctrlProps/ctrlProp2.xml"/></Relationships>"#
    );
    let prop_a = format!(
        r#"<?xml version="1.0"?><formControlPr xmlns="{X14}" objectType="List" fmlaLink="$B$2" fmlaRange="$E$1:$E$4" sel="2" noThreeD="1"/>"#
    );
    let prop_b = format!(
        r#"<?xml version="1.0"?><formControlPr xmlns="{X14}" objectType="Drop" dropStyle="combo" dx="16" fmlaLink="$B$9" fmlaRange="Lists!$A$1:$A$3" sel="3" dropLines="5"/>"#
    );
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout>
<v:shape id="_x0000_s1025" type="#_x0000_t201" stroked="f" filled="f"><x:ClientData ObjectType="List"><x:Anchor>2, 0, 1, 0, 4, 0, 6, 0</x:Anchor><x:FmlaLink>$B$2</x:FmlaLink><x:FmlaRange>$E$1:$E$4</x:FmlaRange><x:Sel>2</x:Sel><x:NoThreeD/></x:ClientData></v:shape>
<v:shape id="_x0000_s1026" type="#_x0000_t201" stroked="f" filled="f"><x:ClientData ObjectType="Drop"><x:Anchor>2, 0, 8, 0, 4, 0, 9, 0</x:Anchor><x:FmlaLink>$B$9</x:FmlaLink><x:FmlaRange>Lists!$A$1:$A$3</x:FmlaRange><x:Sel>3</x:Sel><x:DropLines>5</x:DropLines><x:DropStyle>Combo</x:DropStyle></x:ClientData></v:shape>
<v:shape id="_x0000_s1027" type="#_x0000_t201" stroked="f" filled="f"><v:textbox><div>Size</div></v:textbox><x:ClientData ObjectType="GBox"><x:Anchor>6, 0, 1, 0, 9, 0, 6, 0</x:Anchor></x:ClientData></v:shape>
<v:shape id="_x0000_s1028" type="#_x0000_t201" stroked="f" filled="f"><v:textbox><div>Pick one</div></v:textbox><x:ClientData ObjectType="Label"><x:Anchor>6, 0, 8, 0, 8, 0, 9, 0</x:Anchor></x:ClientData></v:shape>
<v:shape id="_x0000_s1029" type="#_x0000_t201" stroked="f" filled="f"><x:ClientData ObjectType="List"><x:Anchor>10, 0, 1, 0, 12, 0, 6, 0</x:Anchor><x:FmlaRange>$F$1:$F$6</x:FmlaRange><x:SelType>Multi</x:SelType></x:ClientData></v:shape>
</xml>"##;
    let controls = format!(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:x14="{X14}" Requires="x14"><controls>{}{}</controls></mc:Choice></mc:AlternateContent>"#,
        control(1025, "rIdA", "List Box 1", (2, 1, 4, 6)),
        control(1026, "rIdB", "Drop Down 2", (2, 8, 4, 9))
    );
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

fn controls_of(bytes: &[u8]) -> Vec<gridcraft_model::FormControl> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    wb.sheet(0).unwrap().controls.clone()
}

fn find<'a>(c: &'a [gridcraft_model::FormControl], k: &str, nth: usize) -> &'a gridcraft_model::FormControl {
    c.iter().filter(|x| kind(x) == k).nth(nth).unwrap_or_else(|| panic!("no {k} #{nth}: {c:?}"))
}

#[test]
fn list_boxes_and_drop_downs_are_read_with_range_selection_and_link() {
    let c = controls_of(&book_bytes());
    assert_eq!(c.len(), 5, "two listed controls and three VML-only ones: {c:?}");
    let list = find(&c, "ListBox", 0);
    assert_eq!((list.value, list.link.as_deref()), (2, Some("$B$2")), "sel=2: {list:?}");
    let dbg = format!("{list:?}");
    assert!(dbg.contains("input_range: Some(\"$E$1:$E$4\")") && dbg.contains("selection: Single"), "{dbg}");
    assert_eq!(list.anchor.cell, gridcraft_core::CellRef::parse("C2").unwrap());
    let drop = find(&c, "DropDown", 0);
    assert_eq!((drop.value, drop.link.as_deref()), (3, Some("$B$9")), "{drop:?}");
    let dbg = format!("{drop:?}");
    assert!(dbg.contains("input_range: Some(\"Lists!$A$1:$A$3\")") && dbg.contains("drop_lines: 5"), "{dbg}");
}

#[test]
fn vml_only_group_boxes_labels_and_multi_select_lists_are_read() {
    let c = controls_of(&book_bytes());
    assert_eq!(find(&c, "GroupBox", 0).text, "Size");
    assert_eq!(find(&c, "Label", 0).text, "Pick one");
    let multi = find(&c, "ListBox", 1);
    let dbg = format!("{multi:?}");
    assert!(dbg.contains("input_range: Some(\"$F$1:$F$6\")") && dbg.contains("selection: Multi"), "{dbg}");
    assert_eq!((multi.value, multi.link.as_deref()), (0, None), "no Sel, no link: {multi:?}");
}

#[test]
fn a_new_control_shows_eight_drop_down_lines() {
    // Excel's default `dropLines` when the file has none.
    let fresh = gridcraft_model::FormControl::default();
    assert!(format!("{fresh:?}").contains("drop_lines: 8"), "{fresh:?}");
}

#[test]
fn list_controls_are_written_as_excel_writes_them_each_once() {
    let (wb, _) = read_xlsx(&book_bytes()).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&out).unwrap();
    let names = pkg.names();
    let mut props = String::new();
    for n in names.iter().filter(|n| n.starts_with("xl/ctrlProps/")) {
        props.push_str(&String::from_utf8(pkg.read(n).unwrap().unwrap()).unwrap());
        props.push('\n');
    }
    assert_eq!(props.matches("objectType=\"List\"").count(), 2, "{props}");
    assert_eq!(props.matches("objectType=\"Drop\"").count(), 1, "{props}");
    assert_eq!(props.matches("objectType=\"GBox\"").count(), 1, "{props}");
    assert_eq!(props.matches("objectType=\"Label\"").count(), 1, "{props}");
    assert!(props.contains("fmlaRange=\"$E$1:$E$4\"") && props.contains("fmlaRange=\"Lists!$A$1:$A$3\""), "{props}");
    assert!(props.contains("sel=\"2\"") && props.contains("sel=\"3\"") && props.contains("dropLines=\"5\""), "{props}");
    assert_eq!(props.matches("selType=\"multi\"").count(), 1, "{props}");
    assert_eq!(props.matches("dropStyle=\"combo\"").count(), 1, "{props}");
    let sheet = String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap();
    assert_eq!(sheet.matches("<control ").count(), 5, "each control once, none also kept by pass-through: {sheet}");
    let vml_name = names.iter().find(|n| n.starts_with("xl/drawings/vmlDrawing")).expect("a VML drawing");
    let vml = String::from_utf8(pkg.read(vml_name).unwrap().unwrap()).unwrap();
    assert_eq!(vml.matches("ObjectType=\"List\"").count(), 2, "{vml}");
    assert_eq!(vml.matches("ObjectType=\"Drop\"").count(), 1, "{vml}");
    assert_eq!(vml.matches("ObjectType=\"GBox\"").count(), 1, "{vml}");
    assert_eq!(vml.matches("ObjectType=\"Label\"").count(), 1, "{vml}");
    assert!(vml.contains("<x:FmlaRange>Lists!$A$1:$A$3</x:FmlaRange>"), "{vml}");
}

#[test]
fn a_saved_file_reads_back_the_same_list_controls() {
    let (wb, _) = read_xlsx(&book_bytes()).unwrap();
    let (back, report) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(report.warnings.is_empty(), "{:?}", report.warnings);
    let key = |w: &gridcraft_model::Workbook| {
        let mut v: Vec<String> =
            w.sheet(0).unwrap().controls.iter().map(|c| format!("{}|{}|{}|{:?}|{:?}", kind(c), c.text, c.value, c.link, c.anchor.cell)).collect();
        v.sort();
        v
    };
    assert_eq!(key(&back), key(&wb));
    assert_eq!(back.sheet(0).unwrap().controls.len(), 5);
}

#[test]
fn edit_boxes_stay_kept_by_pass_through() {
    // Guard: an EditBox (dialog sheets only in Excel) is still not a model control.
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="{REL}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#
    );
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shape id="_x0000_s1025" type="#_x0000_t201"><x:ClientData ObjectType="EditBox"><x:Anchor>2, 0, 1, 0, 4, 0, 2, 0</x:Anchor></x:ClientData></v:shape></xml>"##;
    let bytes = minimal(
        r#"<sheetData/><legacyDrawing r:id="rId9"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", &rels), ("xl/drawings/vmlDrawing1.vml", vml)],
        "",
        "",
    );
    let (wb, _) = read_xlsx(&bytes).unwrap();
    assert!(wb.sheet(0).unwrap().controls.is_empty());
    let out = write_xlsx(&wb).unwrap();
    let mut pkg = Package::open(&out).unwrap();
    for n in pkg.names().iter().filter(|n| n.starts_with("xl/ctrlProps/")) {
        let prop = String::from_utf8(pkg.read(n).unwrap().unwrap()).unwrap();
        assert!(!prop.contains("objectType=\"EditBox\""), "an edit box is not written as a model control: {prop}");
    }
}
