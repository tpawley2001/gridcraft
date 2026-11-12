//! A form control's `<controlPr>` flags `autoFill`, `autoLine` (default true) and `altText`
//! survive open + save. Excel writes `autoFill="0" autoLine="0"` for check boxes and option
//! buttons; dropping them reopens the control with automatic fill and line. Packages are built
//! in code.

use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const XDR: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";

/// One `<control>` as Excel lists it, anchored over C{row+1}:E{row+2}.
fn control(shape: u32, rid: &str, name: &str, pr_attrs: &str, row: u32) -> String {
    let marker = |tag: &str, col: u32, row: u32| {
        format!(
            r#"<{tag}><xdr:col xmlns:xdr="{XDR}">{col}</xdr:col><xdr:colOff xmlns:xdr="{XDR}">0</xdr:colOff><xdr:row xmlns:xdr="{XDR}">{row}</xdr:row><xdr:rowOff xmlns:xdr="{XDR}">0</xdr:rowOff></{tag}>"#
        )
    };
    format!(
        r#"<mc:AlternateContent><mc:Choice Requires="x14"><control shapeId="{shape}" r:id="{rid}" name="{name}"><controlPr {pr_attrs}><anchor moveWithCells="1">{}{}</anchor></controlPr></control></mc:Choice></mc:AlternateContent>"#,
        marker("from", 2, row),
        marker("to", 4, row + 1)
    )
}

/// Three listed controls whose `controlPr` differ in the attributes under test.
fn book_bytes() -> Vec<u8> {
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId9" Type="{REL}/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/><Relationship Id="rIdA" Type="{REL}/ctrlProp" Target="../ctrlProps/ctrlProp1.xml"/><Relationship Id="rIdB" Type="{REL}/ctrlProp" Target="../ctrlProps/ctrlProp2.xml"/><Relationship Id="rIdC" Type="{REL}/ctrlProp" Target="../ctrlProps/ctrlProp3.xml"/></Relationships>"#
    );
    let check = format!(r#"<?xml version="1.0"?><formControlPr xmlns="{X14}" objectType="CheckBox" fmlaLink="$B$2" lockText="1" noThreeD="1"/>"#);
    let radio = format!(r#"<?xml version="1.0"?><formControlPr xmlns="{X14}" objectType="Radio" checked="Checked" fmlaLink="$B$5" lockText="1"/>"#);
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout>
<v:shape id="_x0000_s1025" type="#_x0000_t201" stroked="f" filled="f"><v:textbox><div>Paid</div></v:textbox><x:ClientData ObjectType="Checkbox"><x:Anchor>2, 0, 1, 0, 4, 0, 2, 0</x:Anchor><x:FmlaLink>$B$2</x:FmlaLink><x:NoThreeD/></x:ClientData></v:shape>
<v:shape id="_x0000_s1026" type="#_x0000_t201" stroked="f" filled="f"><v:textbox><div>Small</div></v:textbox><x:ClientData ObjectType="Radio"><x:Anchor>2, 0, 4, 0, 4, 0, 5, 0</x:Anchor><x:FmlaLink>$B$5</x:FmlaLink><x:Checked>1</x:Checked></x:ClientData></v:shape>
<v:shape id="_x0000_s1027" type="#_x0000_t201"><v:textbox><div>Plain</div></v:textbox><x:ClientData ObjectType="Checkbox"><x:Anchor>2, 0, 7, 0, 4, 0, 8, 0</x:Anchor><x:FmlaLink>$B$8</x:FmlaLink><x:NoThreeD/></x:ClientData></v:shape>
</xml>"##;
    let controls = format!(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:x14="{X14}" Requires="x14"><controls>{}{}{}</controls></mc:Choice></mc:AlternateContent>"#,
        control(1025, "rIdA", "Check Box 1", r#"defaultSize="0" autoFill="0" autoLine="0" autoPict="0" altText="Add SMS &amp; Bar""#, 1),
        control(1026, "rIdB", "Option Button 2", r#"defaultSize="0" print="0" autoLine="0" autoPict="0""#, 4),
        control(1027, "rIdC", "Check Box 3", r#"defaultSize="0" autoPict="0""#, 7),
    );
    minimal(
        &format!(r#"<sheetData/><legacyDrawing r:id="rId9"/>{controls}"#),
        &[
            ("xl/worksheets/_rels/sheet1.xml.rels", &rels),
            ("xl/drawings/vmlDrawing1.vml", vml),
            ("xl/ctrlProps/ctrlProp1.xml", &check),
            ("xl/ctrlProps/ctrlProp2.xml", &radio),
            ("xl/ctrlProps/ctrlProp3.xml", &check),
        ],
        "",
        "",
    )
}

/// The saved worksheet XML after open + save.
fn saved_sheet() -> String {
    let out = write_xlsx(&read_xlsx(&book_bytes()).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    s
}

/// The `<controlPr ...>` start tag of the `<control>` named `name`.
fn control_pr<'a>(sheet: &'a str, name: &str) -> &'a str {
    let at = sheet.find(&format!("name=\"{name}\"")).unwrap_or_else(|| panic!("no control named {name} in {sheet}"));
    let rest = &sheet[at..];
    let start = rest.find("<controlPr").unwrap_or_else(|| panic!("no controlPr after {name}"));
    let end = rest[start..].find('>').unwrap();
    &rest[start..start + end + 1]
}

#[test]
fn control_pr_kept_auto_fill_and_auto_line_off() {
    let sheet = saved_sheet();
    let check = control_pr(&sheet, "Check Box 1");
    assert!(check.contains(r#"autoFill="0""#), "autoFill=\"0\" kept: {check}");
    assert!(check.contains(r#"autoLine="0""#), "autoLine=\"0\" kept: {check}");
    let radio = control_pr(&sheet, "Option Button 2");
    assert!(radio.contains(r#"autoLine="0""#), "autoLine=\"0\" kept: {radio}");
    assert!(!radio.contains(r#"autoFill="0""#), "automatic fill stays automatic: {radio}");
    assert!(radio.contains(r#"print="0""#), "print=\"0\" still written: {radio}");
}

#[test]
fn control_pr_kept_alt_text() {
    let sheet = saved_sheet();
    let check = control_pr(&sheet, "Check Box 1");
    assert!(check.contains(r#"altText="Add SMS &amp; Bar""#), "altText kept and escaped: {check}");
    assert!(!control_pr(&sheet, "Option Button 2").contains("altText"), "no alt text invented");
}

#[test]
fn control_pr_kept_defaults_write_nothing_new() {
    let sheet = saved_sheet();
    let plain = control_pr(&sheet, "Check Box 3");
    for attr in ["autoFill", "autoLine", "altText"] {
        assert!(!plain.contains(attr), "{attr} is not written for a control that did not have it: {plain}");
    }
    assert!(plain.contains(r#"defaultSize="0""#) && plain.contains(r#"autoPict="0""#), "{plain}");
}

#[test]
fn control_pr_kept_in_the_model() {
    let (wb, _) = read_xlsx(&book_bytes()).unwrap();
    let c = &wb.sheet(0).unwrap().controls;
    assert_eq!(c.len(), 3, "{c:?}");
    let by_text = |t: &str| format!("{:?}", c.iter().find(|x| x.text == t).unwrap_or_else(|| panic!("no control captioned {t}: {c:?}")));
    let paid = by_text("Paid");
    assert!(paid.contains("auto_fill: false") && paid.contains("auto_line: false"), "{paid}");
    assert!(paid.contains(r#"alt_text: Some("Add SMS & Bar")"#), "{paid}");
    let small = by_text("Small");
    assert!(small.contains("auto_fill: true") && small.contains("auto_line: false") && small.contains("alt_text: None"), "{small}");
    let plain = by_text("Plain");
    assert!(plain.contains("auto_fill: true") && plain.contains("auto_line: true") && plain.contains("alt_text: None"), "{plain}");
}
