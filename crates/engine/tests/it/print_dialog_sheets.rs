//! Visible Excel 5 dialog sheets print in tab order. Excel's frozen DialogSheet
//! reference includes the form before its two worksheets; all fixtures here are synthetic.
use std::io::Write as _;

use gridcraft_engine::Session;
use serde_json::{Value as Json, json};

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

fn package(state: &str) -> Vec<u8> {
    let hidden = if state.is_empty() { String::new() } else { format!(r#" state="{state}""#) };
    let wb = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><bookViews><workbookView activeTab="0"/></bookViews><sheets><sheet name="Before" sheetId="1" r:id="r1"/><sheet name="Form" sheetId="2" r:id="r2"{hidden}/><sheet name="After" sheetId="3" r:id="r3"/></sheets></workbook>"#
    );
    let rels = format!(
        r#"<Relationships xmlns="{PKG}"><Relationship Id="r1" Type="{REL}/worksheet" Target="worksheets/a.xml"/><Relationship Id="r2" Type="{REL}/dialogsheet" Target="dialogsheets/form.xml"/><Relationship Id="r3" Type="{REL}/worksheet" Target="worksheets/b.xml"/></Relationships>"#
    );
    let root =
        format!(r#"<Relationships xmlns="{PKG}"><Relationship Id="office" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#);
    let worksheet = format!(r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><v>7</v></c></row></sheetData></worksheet>"#);
    let dialog = format!(
        r#"<dialogsheet xmlns="{MAIN}" xmlns:r="{REL}"><sheetViews><sheetView workbookViewId="0"/></sheetViews><sheetFormatPr defaultColWidth="1" defaultRowHeight="5.25" customHeight="1"/><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><pageSetup paperSize="1" orientation="portrait"/><legacyDrawing r:id="vml"/></dialogsheet>"#
    );
    let dialog_rels =
        format!(r#"<Relationships xmlns="{PKG}"><Relationship Id="vml" Type="{REL}/vmlDrawing" Target="../drawings/form.vml"/></Relationships>"#);
    let vml = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shapetype id="_x0000_t201" coordsize="21600,21600"/><v:shape id="_x0000_s1100" type="#_x0000_t201" style="position:absolute;margin-left:20pt;margin-top:20pt;width:240pt;height:100pt"><v:textbox><div>Watch form</div></v:textbox><x:ClientData ObjectType="Dialog"><x:Anchor>4,0,4,0,48,0,23,0</x:Anchor></x:ClientData></v:shape><v:shape id="_x0000_s1101" type="#_x0000_t201" style="position:absolute;margin-left:120pt;margin-top:40pt;width:60pt;height:20pt"><v:textbox><div>Take watch</div></v:textbox><x:ClientData ObjectType="Button"><x:Anchor>24,0,8,0,36,0,12,0</x:Anchor><x:PrintObject>True</x:PrintObject></x:ClientData></v:shape><v:shape id="_x0000_s1102" type="#_x0000_t201" style="position:absolute;margin-left:120pt;margin-top:70pt;width:60pt;height:20pt"><v:textbox><div>Leave watch</div></v:textbox><x:ClientData ObjectType="Button"><x:Anchor>24,0,14,0,36,0,18,0</x:Anchor><x:PrintObject>True</x:PrintObject></x:ClientData></v:shape></xml>"##;
    let types = r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/a.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/b.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/dialogsheets/form.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.dialogsheet+xml"/></Types>"#;
    let parts = [
        ("[Content_Types].xml", types),
        ("_rels/.rels", root.as_str()),
        ("xl/workbook.xml", wb.as_str()),
        ("xl/_rels/workbook.xml.rels", rels.as_str()),
        ("xl/worksheets/a.xml", worksheet.as_str()),
        ("xl/worksheets/b.xml", worksheet.as_str()),
        ("xl/dialogsheets/form.xml", dialog.as_str()),
        ("xl/dialogsheets/_rels/form.xml.rels", dialog_rels.as_str()),
        ("xl/drawings/form.vml", vml),
    ];
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in parts {
        out.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(body.as_bytes()).unwrap();
    }
    out.finish().unwrap().into_inner()
}

fn open(bytes: &[u8]) -> Session {
    // The real SpreadsheetML parser reads every fixture; no raw XML string checks.
    let (wb, _) = gridcraft_xlsx::read_xlsx(bytes).unwrap();
    assert_eq!(wb.sheets.len(), 2, "dialog sheets stay out of editable worksheets");
    let mut s = Session::new();
    s.execute("file.open", json!({"name":"form.xlsx", "base64": gridcraft_engine::io::base64_encode(bytes)})).unwrap();
    s
}

fn names(s: &mut Session) -> Vec<String> {
    let result = s.execute("file.printPreview", json!({"sheets":"all"})).unwrap();
    result["pageList"].as_array().unwrap().iter().map(|p| p["sheet"].as_str().unwrap().to_owned()).collect()
}

#[test]
fn visible_dialog_prints_between_its_neighbor_worksheets() {
    assert_eq!(names(&mut open(&package(""))), ["Before", "Form", "After"]);
}

#[test]
fn dialog_controls_and_frame_caption_are_drawn_on_the_extra_pdf_page() {
    let mut s = open(&package(""));
    let result = s.execute("file.exportPdf", json!({"sheets":"all"})).unwrap();
    assert_eq!(result["pages"], 3);
    let bytes = gridcraft_engine::io::base64_decode(result["base64"].as_str().unwrap()).unwrap();
    let pdf = String::from_utf8_lossy(&bytes);
    for caption in ["Watch form", "Take watch", "Leave watch"] {
        assert!(pdf.contains(&format!("({caption}) Tj")), "missing printable caption {caption}");
    }
}

#[test]
fn hidden_and_very_hidden_forms_do_not_print() {
    for state in ["hidden", "veryHidden"] {
        assert_eq!(names(&mut open(&package(state))), ["Before", "After"]);
    }
}

#[test]
fn active_worksheet_print_remains_one_page() {
    let mut s = open(&package(""));
    let result = s.execute("file.printPreview", Json::Object(Default::default())).unwrap();
    assert_eq!(result["pageList"].as_array().unwrap().len(), 1);
    assert_eq!(result["pageList"][0]["sheet"], "Before");
}

#[test]
fn save_reopen_keeps_the_printable_form_and_its_tab_position() {
    let bytes = package("");
    let (wb, _) = gridcraft_xlsx::read_xlsx(&bytes).unwrap();
    let saved = gridcraft_xlsx::write_xlsx(&wb).unwrap();
    assert_eq!(names(&mut open(&saved)), ["Before", "Form", "After"]);
}
