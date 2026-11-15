//! Independent dialog fixtures exercise inert print views and kept-part ownership.
use super::make_zip;
use crate::{read_xlsx, write_xlsx};
use gridcraft_model::ControlKind;
use std::io::Read as _;

const DIALOG: &str = r#"<dialogsheet xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheetFormatPr defaultColWidth="1" defaultRowHeight="6"/><pageSetup orientation="landscape"/><legacyDrawing r:id="v"/></dialogsheet>"#;
const VML: &str = r#"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shape id="_x0000_s1200"><v:textbox><div>Inspection form</div></v:textbox><x:ClientData ObjectType="Dialog"><x:Anchor>1,0,1,0,40,0,20,0</x:Anchor></x:ClientData></v:shape><v:shape id="_x0000_s1201"><v:textbox><div>Accept entry</div></v:textbox><x:ClientData ObjectType="Button"><x:Anchor>3,0,4,0,18,0,8,0</x:Anchor><x:FmlaMacro>[0]!DoSomething</x:FmlaMacro></x:ClientData></v:shape></xml>"#;

fn package(vml: Option<&str>, kind: &str) -> Vec<u8> {
    let rels = format!(
        r#"<Relationships><Relationship Id="a" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/a.xml"/><Relationship Id="b" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="dialogsheets/b.xml"/></Relationships>"#
    );
    let mut parts = vec![
        ("[Content_Types].xml", br#"<Types><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/dialogsheets/b.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml"/></Types>"#.as_slice()),
        ("_rels/.rels", br#"<Relationships><Relationship Id="w" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#),
        ("xl/workbook.xml", br#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="a"/><sheet name="Inspection" sheetId="2" r:id="b"/></sheets></workbook>"#),
        ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ("xl/worksheets/a.xml", br#"<worksheet><sheetData/></worksheet>"#),
        ("xl/dialogsheets/b.xml", DIALOG.as_bytes()),
        ("xl/dialogsheets/_rels/b.xml.rels", br#"<Relationships><Relationship Id="v" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/b.vml"/></Relationships>"#),
    ];
    if let Some(vml) = vml {
        parts.push(("xl/drawings/b.vml", vml.as_bytes()));
    }
    make_zip(&parts)
}

#[test]
fn detached_controls_are_inert_and_original_vml_is_preserved() {
    let (wb, _) = read_xlsx(&package(Some(VML), "dialogsheet")).unwrap();
    let view = &wb.dialog_sheets[0].sheet;
    assert_eq!(view.controls.len(), 2);
    assert_eq!(view.controls[0].kind, ControlKind::GroupBox);
    assert_eq!(view.controls[0].text, "Inspection form");
    assert_eq!(view.controls[1].text, "Accept entry");
    assert!(view.controls.iter().all(|c| c.action.is_none()));
    assert_eq!(view.print.orientation, gridcraft_model::Orientation::Landscape);
    assert_eq!(view.default_row_height, 8.0);
    let saved = write_xlsx(&wb).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&saved)).unwrap();
    let mut bytes = Vec::new();
    zip.by_name("xl/drawings/b.vml").unwrap().read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, VML.as_bytes());
    let (reopened, _) = read_xlsx(&saved).unwrap();
    assert_eq!(reopened.sheets.len(), 1);
    assert_eq!(reopened.dialog_sheets[0].sheet.controls, view.controls);
}

#[test]
fn absent_empty_or_unreadable_vml_has_no_form_content() {
    for vml in [None, Some("<xml/>"), Some("<invalid")] {
        let (wb, _) = read_xlsx(&package(vml, "dialogsheet")).unwrap();
        assert!(wb.dialog_sheets[0].sheet.controls.is_empty());
    }
}

#[test]
fn worksheet_dialog_frames_still_belong_to_pass_through() {
    let (wb, _) = read_xlsx(&package(Some(VML), "worksheet")).unwrap();
    assert!(wb.dialog_sheets.is_empty());
    assert_eq!(wb.sheets[1].controls.len(), 1);
    assert_eq!(wb.sheets[1].controls[0].kind, ControlKind::Button);
    assert!(wb.sheets[1].controls[0].action.is_some());
}
