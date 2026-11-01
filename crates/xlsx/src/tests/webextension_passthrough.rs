//! Office add-ins (web extensions, [MS-OWEXML]): a content add-in's drawing anchor wrapped in
//! `mc:AlternateContent` with its `webextension` part and fallback picture, and the workbook's
//! task panes part, survive open + save. Packages are built in code; the parts are small
//! synthetic stand-ins.

use std::collections::{HashMap, HashSet};
use std::io::Read;

use crate::package::{normalize, resolve, split_dir};
use crate::tests::make_zip;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_WE: &str = "http://schemas.microsoft.com/office/webextensions/webextension/2010/11";
const REL_WE: &str = "http://schemas.microsoft.com/office/2011/relationships/webextension";
const REL_TASKPANES: &str = "http://schemas.microsoft.com/office/2011/relationships/webextensiontaskpanes";
const CT_WE: &str = "application/vnd.ms-office.webextension+xml";
const CT_TASKPANES: &str = "application/vnd.ms-office.webextensiontaskpanes+xml";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n fallback snapshot bytes";
const ORDINARY_PNG: &[u8] = b"\x89PNG\r\n\x1a\n ordinary picture bytes";
const WE_IMAGE: &[u8] = b"\x89PNG\r\n\x1a\n web extension image bytes";

const WEB_EXTENSION: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><we:webextension xmlns:we="http://schemas.microsoft.com/office/webextensions/webextension/2010/11" id="{11111111-2222-3333-4444-555555555555}"><we:reference id="WA104380862" version="1.0.0.0" store="en-US" storeType="OMEX"/><we:alternateReferences/><we:properties><we:property name="ADDIN-MARKER" value="1"/></we:properties><we:bindings><we:binding id="B1" type="cell" ref="Data!$A$1"/></we:bindings><we:snapshot xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><we:image r:id="rId1"/></we:snapshot></we:webextension>"#;
const WEB_EXTENSION_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image3.png"/></Relationships>"#;
const TASKPANES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><wetp:taskpanes xmlns:wetp="http://schemas.microsoft.com/office/webextensions/taskpanes/2010/11"><wetp:taskpane dockState="right" visibility="1" width="350" row="1"><wetp:webextensionref xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId1"/></wetp:taskpane></wetp:taskpanes>"#;
const TASKPANES_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.microsoft.com/office/2011/relationships/webextension" Target="webextension1.xml"/></Relationships>"#;

const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/webextensions/webextension1.xml" ContentType="application/vnd.ms-office.webextension+xml"/><Override PartName="/xl/webextensions/taskpanes.xml" ContentType="application/vnd.ms-office.webextensiontaskpanes+xml"/></Types>"#;

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        let ty = if ty.starts_with("http") { ty.to_string() } else { format!("{NS_R}/{ty}") };
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// The add-in's anchor as Excel writes it: the web extension frame in the choice, the snapshot
/// picture in the fallback. `we` is the web extension relationship id, `image` the fallback's.
fn addin_anchor(we: &str, image: &str) -> String {
    format!(
        r#"<xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>11</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><mc:AlternateContent xmlns:mc="{NS_MC}"><mc:Choice xmlns:we="{NS_WE}" Requires="we"><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="3" name="Add-in 1"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="{NS_WE}"><we:webextensionref xmlns:r="{NS_R}" r:id="{we}"/></a:graphicData></a:graphic></xdr:graphicFrame></mc:Choice><mc:Fallback><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="4" name="Add-in 1"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip xmlns:r="{NS_R}" r:embed="{image}"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic></mc:Fallback></mc:AlternateContent><xdr:clientData/></xdr:twoCellAnchor>"#
    )
}

const PICTURE_ANCHOR: &str = r#"<xdr:twoCellAnchor><xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>13</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>2</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>17</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1" descr="ordinary"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor>"#;

/// One sheet "Data" with an ordinary picture and an Office add-in, the add-in's web extension
/// part (with its own rels) and the workbook's task panes part. `webextension` writes the web
/// extension part and its rels; `taskpanes` the task panes part and its rels.
fn package(webextension: bool, taskpanes: bool) -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let mut wb_rels = vec![("rId1", "worksheet", "worksheets/sheet1.xml")];
    if taskpanes {
        wb_rels.push(("rId2", REL_TASKPANES, "webextensions/taskpanes.xml"));
    }
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#
    );
    let sheet_rels = rels(&[("rId1", "drawing", "../drawings/drawing1.xml")]);
    let drawing = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}">{PICTURE_ANCHOR}{}</xdr:wsDr>"#,
        addin_anchor("rId3", "rId2")
    );
    let drawing_rels = rels(&[
        ("rId1", "image", "../media/image1.png"),
        ("rId2", "image", "../media/image2.png"),
        ("rId3", REL_WE, "../webextensions/webextension1.xml"),
    ]);
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&wb_rels);
    let mut files: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels.as_bytes()),
        ("xl/media/image1.png", ORDINARY_PNG),
        ("xl/media/image2.png", PNG),
        ("xl/media/image3.png", WE_IMAGE),
    ];
    if webextension {
        files.push(("xl/webextensions/webextension1.xml", WEB_EXTENSION.as_bytes()));
        files.push(("xl/webextensions/_rels/webextension1.xml.rels", WEB_EXTENSION_RELS.as_bytes()));
    }
    if taskpanes {
        files.push(("xl/webextensions/taskpanes.xml", TASKPANES.as_bytes()));
        files.push(("xl/webextensions/_rels/taskpanes.xml.rels", TASKPANES_RELS.as_bytes()));
    }
    make_zip(&files)
}

struct Unpacked {
    parts: HashMap<String, Vec<u8>>,
}

impl Unpacked {
    fn new(bytes: &[u8]) -> Unpacked {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut parts = HashMap::new();
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let mut v = vec![];
            f.read_to_end(&mut v).unwrap();
            parts.insert(f.name().to_string(), v);
        }
        Unpacked { parts }
    }

    fn text(&self, name: &str) -> String {
        String::from_utf8(self.parts.get(name).unwrap_or_else(|| panic!("no part {name}")).clone()).unwrap()
    }

    /// (id, type, resolved target)
    fn rels(&self, part: &str) -> Vec<(String, String, String)> {
        let (dir, file) = split_dir(part);
        let name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let Some(b) = self.parts.get(&name) else { return vec![] };
        let root = xml::parse(b).unwrap();
        root.kids("Relationship")
            .map(|r| (r.attr("Id").unwrap().into(), r.attr("Type").unwrap().into(), resolve(dir, r.attr("Target").unwrap())))
            .collect()
    }

    fn target(&self, part: &str, id: &str) -> String {
        self.rels(part).into_iter().find(|r| r.0 == id).unwrap_or_else(|| panic!("{part}: no relationship {id}")).2
    }

    fn rel_of_type(&self, part: &str, kind: &str) -> Option<(String, String)> {
        self.rels(part).into_iter().find(|r| r.1.ends_with(&format!("/{kind}"))).map(|r| (r.0, r.2))
    }

    /// Every part has a content type, every internal relationship target exists and rel ids are
    /// unique per part: nothing kept is written without the parts it names, or vice versa.
    fn assert_no_dangling(&self) {
        let types = xml::parse(self.parts.get("[Content_Types].xml").unwrap()).unwrap();
        let defaults: HashSet<String> = types.kids("Default").filter_map(|d| d.attr("Extension")).map(str::to_ascii_lowercase).collect();
        let overrides: HashSet<String> = types.kids("Override").filter_map(|d| d.attr("PartName")).map(normalize).collect();
        for name in self.parts.keys().filter(|n| *n != "[Content_Types].xml") {
            let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
            assert!(overrides.contains(&normalize(name)) || defaults.contains(&ext), "{name} has no content type");
        }
        for name in self.parts.keys().filter(|n| n.ends_with(".rels")) {
            let source = name.replace("_rels/", "").trim_end_matches(".rels").to_string();
            let mut ids = HashSet::new();
            for (id, _, target) in self.rels(&source) {
                assert!(ids.insert(id.clone()), "{name}: duplicate id {id}");
                assert!(self.parts.keys().any(|p| normalize(p) == normalize(&target)), "{name}: {id} → missing {target}");
            }
        }
    }

    /// The drawing part of sheet "Data" (through `<drawing r:id>`), with its XML.
    fn drawing(&self) -> (String, String) {
        let wb = xml::parse(self.text("xl/workbook.xml").as_bytes()).unwrap();
        let id = wb.child("sheets").unwrap().kids("sheet").next().unwrap().attr("id").unwrap().to_string();
        let sheet = self.target("xl/workbook.xml", &id);
        let x = xml::parse(self.text(&sheet).as_bytes()).unwrap();
        let did = x.child("drawing").unwrap_or_else(|| panic!("{sheet} has no <drawing>")).attr("id").unwrap().to_string();
        let target = self.target(&sheet, &did);
        let text = self.text(&target);
        (target, text)
    }
}

/// The add-in anchor of a drawing: its `from`/`to` markers and the two relationship ids.
fn addin_anchor_of(drawing: &str) -> (String, String, String, String) {
    let d = xml::parse(drawing.as_bytes()).unwrap();
    let anchor = d
        .children
        .iter()
        .find(|c| c.name == "twoCellAnchor" && c.child("AlternateContent").is_some())
        .unwrap_or_else(|| panic!("add-in anchor lost: {drawing}"));
    let marker = |m: &El| -> String {
        let v = |n: &str| m.child(n).and_then(|e| e.text.trim().parse::<u32>().ok()).unwrap_or(u32::MAX);
        format!("{},{},{},{}", v("col"), v("colOff"), v("row"), v("rowOff"))
    };
    let from = marker(anchor.child("from").unwrap());
    let to = marker(anchor.child("to").unwrap());
    let ac = anchor.child("AlternateContent").unwrap();
    let we = ac
        .path(&["Choice", "graphicFrame", "graphic", "graphicData", "webextensionref"])
        .and_then(|e| e.attr("id"))
        .unwrap_or_else(|| panic!("no webextensionref: {drawing}"))
        .to_string();
    let image = ac
        .path(&["Fallback", "pic", "blipFill", "blip"])
        .and_then(|e| e.attr("embed"))
        .unwrap_or_else(|| panic!("no fallback picture: {drawing}"))
        .to_string();
    (from, to, we, image)
}

/// The add-in is complete in `out`: the anchor at its place, its web extension part (with its own
/// rels and their targets) and its fallback picture, each resolvable and with its content type.
fn assert_addin_kept(out: &[u8]) {
    let u = Unpacked::new(out);
    u.assert_no_dangling();
    let (drawing, dx) = u.drawing();
    let (from, to, we, image) = addin_anchor_of(&dx);
    assert_eq!(from, "2,0,1,0", "{dx}");
    assert_eq!(to, "7,0,11,0", "{dx}");
    let (_, ty, part) = u.rels(&drawing).into_iter().find(|r| r.0 == we).unwrap_or_else(|| panic!("{we} does not resolve from {drawing}"));
    assert_eq!(ty, REL_WE);
    assert_eq!(u.parts.get(&part).map(Vec::as_slice), Some(WEB_EXTENSION.as_bytes()), "{part} changed");
    let ct = u.text("[Content_Types].xml");
    assert!(ct.contains(&format!("PartName=\"/{part}\" ContentType=\"{CT_WE}\"")), "content type of {part}: {ct}");
    // The web extension's own rels and the image they name.
    let (_, _, image_part) = u
        .rels(&part)
        .into_iter()
        .find(|r| r.1.ends_with("/image"))
        .unwrap_or_else(|| panic!("{part} lost its rels: {:?}", u.text(&format!("{}/_rels/{}.rels", split_dir(&part).0, split_dir(&part).1))));
    assert_eq!(u.parts.get(&image_part).map(Vec::as_slice), Some(WE_IMAGE), "{image_part} changed");
    // The fallback picture.
    let (_, ty, fallback) = u.rels(&drawing).into_iter().find(|r| r.0 == image).unwrap_or_else(|| panic!("fallback {image} does not resolve"));
    assert!(ty.ends_with("/image"), "{ty}");
    assert_eq!(u.parts.get(&fallback).map(Vec::as_slice), Some(PNG), "{fallback} changed");
}

/// The task panes part, its workbook relationship and its web extension part are written.
fn assert_taskpanes_kept(out: &[u8]) {
    let u = Unpacked::new(out);
    let (_, part) = u.rel_of_type("xl/workbook.xml", "webextensiontaskpanes").expect("task panes relationship lost");
    assert_eq!(u.parts.get(&part).map(Vec::as_slice), Some(TASKPANES.as_bytes()), "{part} changed");
    let ct = u.text("[Content_Types].xml");
    assert!(ct.contains(&format!("PartName=\"/{part}\" ContentType=\"{CT_TASKPANES}\"")), "content type of {part}: {ct}");
    let (_, ty, we) = u.rels(&part).into_iter().find(|r| r.1 == REL_WE).unwrap_or_else(|| panic!("{part} lost its rels"));
    assert_eq!(ty, REL_WE);
    assert_eq!(u.parts.get(&we).map(Vec::as_slice), Some(WEB_EXTENSION.as_bytes()), "{we} changed");
}

#[test]
fn addin_survives_two_saves() {
    let (wb, rep) = read_xlsx(&package(true, true)).unwrap();
    assert!(rep.warnings.iter().any(|w| w.contains("Office add-in")), "expected a 'not shown, kept' warning: {:?}", rep.warnings);
    let out = write_xlsx(&wb).unwrap();
    assert_addin_kept(&out);
    assert_taskpanes_kept(&out);
    let (wb2, _) = read_xlsx(&out).unwrap();
    let out2 = write_xlsx(&wb2).unwrap();
    assert_addin_kept(&out2);
    assert_taskpanes_kept(&out2);
    // The second save writes the same parts, each once.
    assert_eq!(Unpacked::new(&out2).parts.len(), Unpacked::new(&out).parts.len(), "a save changed the part count");
}

#[test]
fn addin_beside_a_picture_keeps_both() {
    let (wb, _) = read_xlsx(&package(true, true)).unwrap();
    assert_eq!(wb.sheet(0).unwrap().images.len(), 1, "the ordinary picture is modelled");
    let out = write_xlsx(&wb).unwrap();
    let u = Unpacked::new(&out);
    u.assert_no_dangling();
    let (drawing, dx) = u.drawing();
    let d = xml::parse(dx.as_bytes()).unwrap();
    let names: Vec<&str> = d.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["twoCellAnchor", "twoCellAnchor"], "{dx}");
    let pic = d.children[0].path(&["pic", "blipFill", "blip"]).unwrap().attr("embed").unwrap();
    assert_eq!(u.parts.get(&u.target(&drawing, pic)).map(Vec::as_slice), Some(ORDINARY_PNG));
    assert_addin_kept(&out);
}

#[test]
fn taskpanes_share_the_addin_part() {
    let (wb, _) = read_xlsx(&package(true, true)).unwrap();
    let u = Unpacked::new(&write_xlsx(&wb).unwrap());
    u.assert_no_dangling();
    let (drawing, _) = u.drawing();
    let (_, from_drawing) = u.rel_of_type(&drawing, "webextension").expect("drawing web extension");
    let (_, taskpanes) = u.rel_of_type("xl/workbook.xml", "webextensiontaskpanes").expect("task panes");
    let (_, from_taskpanes) = u.rel_of_type(&taskpanes, "webextension").expect("task panes web extension");
    assert_eq!(from_drawing, from_taskpanes, "the web extension part is written once");
    let written = u.parts.keys().filter(|n| n.starts_with("xl/webextensions/webextension")).count();
    assert_eq!(written, 1, "duplicate web extension parts: {:?}", u.parts.keys().collect::<Vec<_>>());
    // Its own rels are written once too, and still name the image the part's XML refers to.
    assert_eq!(u.parts.keys().filter(|n| n.starts_with("xl/webextensions/_rels/")).count(), 2, "{:?}", u.parts.keys().collect::<Vec<_>>());
}

#[test]
fn deleted_sheet_drops_the_addin_but_keeps_taskpanes() {
    let (mut wb, _) = read_xlsx(&package(true, true)).unwrap();
    wb.sheets[0] = std::sync::Arc::new(gridcraft_model::Sheet::new("Data"));
    let out = write_xlsx(&wb).unwrap();
    let u = Unpacked::new(out.as_slice());
    u.assert_no_dangling();
    assert!(!u.parts.values().any(|v| v.as_slice() == PNG), "the add-in's fallback picture of a deleted sheet was written");
    assert!(
        !u.parts.contains_key("xl/drawings/drawing1.xml"),
        "the drawing of the replaced sheet was kept: {:?}",
        u.parts.keys().collect::<Vec<_>>()
    );
    assert_taskpanes_kept(&out);
}

#[test]
fn missing_webextension_part_leaves_nothing_dangling() {
    // The drawing names a web extension part the package doesn't hold: the anchor (choice and
    // fallback alike) is dropped, so nothing points at a part that isn't written.
    let (wb, rep) = read_xlsx(&package(false, true)).unwrap();
    assert!(rep.warnings.iter().any(|w| w.contains("Office add-in")), "expected a warning: {:?}", rep.warnings);
    let out = write_xlsx(&wb).unwrap();
    let u = Unpacked::new(&out);
    u.assert_no_dangling();
    let (drawing, dx) = u.drawing();
    assert!(!dx.contains("webextensionref"), "the anchor still names a missing part: {dx}");
    assert!(!u.rels(&drawing).iter().any(|r| r.1 == REL_WE), "a relationship to a missing part was written");
    assert!(!u.parts.keys().any(|n| n.starts_with("xl/webextensions/webextension")), "a missing part was invented");
    assert!(u.rel_of_type("xl/workbook.xml", "webextensiontaskpanes").is_none(), "a task panes part naming a missing part was written");
    // The ordinary picture is still there.
    let d = xml::parse(dx.as_bytes()).unwrap();
    let pic = d.children[0].path(&["pic", "blipFill", "blip"]).unwrap().attr("embed").unwrap();
    assert_eq!(u.parts.get(&u.target(&drawing, pic)).map(Vec::as_slice), Some(ORDINARY_PNG));
}

#[test]
fn missing_taskpanes_webextension_leaves_nothing_dangling() {
    // The task panes part names a web extension part the package doesn't hold.
    let (wb, _) = read_xlsx(&package(true, false)).unwrap();
    let out = write_xlsx(&wb).unwrap();
    let u = Unpacked::new(&out);
    u.assert_no_dangling();
    assert!(u.rel_of_type("xl/workbook.xml", "webextensiontaskpanes").is_none(), "a task panes part naming a missing part was written");
    assert_addin_kept(&out);
}

#[test]
fn addin_anchor_keeps_its_relationship_ids() {
    // The web extension part is written byte for byte and names its own rels by id.
    let (wb, _) = read_xlsx(&package(true, true)).unwrap();
    let u = Unpacked::new(&write_xlsx(&wb).unwrap());
    let (drawing, _) = u.drawing();
    let (_, part) = u.rel_of_type(&drawing, "webextension").unwrap();
    let (id, _, image) = u.rels(&part).into_iter().find(|r| r.1.ends_with("/image")).expect("the part's own rels");
    assert!(u.text(&part).contains(&format!("r:id=\"{id}\"")), "the part's own id changed");
    assert_eq!(u.parts.get(&image).map(Vec::as_slice), Some(WE_IMAGE));
}
