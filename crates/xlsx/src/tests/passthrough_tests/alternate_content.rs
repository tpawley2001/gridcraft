//! Drawing anchors wrapped in `mc:AlternateContent` (form and ActiveX control shapes) survive
//! open + save. Packages are built in code.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::Arc;

use gridcraft_core::CellRef;
use gridcraft_model::{Anchor, Chart, ChartKind, LegendPos, Series, Workbook};

use crate::package::{normalize, resolve, split_dir};
use crate::tests::make_zip;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n picture bytes";
const FILL_PNG: &[u8] = b"\x89PNG\r\n\x1a\n control fill bytes";
const CTRL_PROP: &str = r#"<?xml version="1.0"?><formControlPr xmlns="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main" objectType="EditBox" fmlaLink="$C$1" lockText="1"/>"#;
const VML: &str = r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:x="urn:schemas-microsoft-com:office:excel"><o:shapelayout v:ext="edit"><o:idmap v:ext="edit" data="1"/></o:shapelayout><v:shape id="_x0000_s1025" type="#_x0000_t201" style="position:absolute"><x:ClientData ObjectType="EditBox"><x:Anchor>1, 0, 1, 0, 3, 0, 2, 0</x:Anchor><x:FmlaLink>$C$1</x:FmlaLink></x:ClientData></v:shape></xml>"##;
const CONTENT_TYPES: &str = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="vml" ContentType="application/vnd.openxmlformats-officedocument.vmlDrawing"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/><Override PartName="/xl/ctrlProps/ctrlProp1.xml" ContentType="application/vnd.ms-excel.controlproperties+xml"/></Types>"#;

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = String::from(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#);
    for (id, ty, target) in items {
        let ty = if ty.starts_with("http") { ty.to_string() } else { format!("{NS_R}/{ty}") };
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

/// An edit box's DrawingML anchor as Excel 2010+ writes it, with `cNvPr id` `id`; `fill` gives the
/// shape a picture fill through relationship `rId2`.
fn control_anchor(id: u32, fill: bool) -> String {
    let blip = if fill { r#"<a:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>"# } else { "" };
    format!(
        r#"<mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" Requires="a14"><xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>3</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>2</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="{id}" name="Drop Down 1" hidden="1"><a:extLst><a:ext uri="{{63B3BB69-23CF-44E3-9099-C40C66FF867C}}"><a14:compatExt spid="_x0000_s1025"/></a:ext></a:extLst></xdr:cNvPr><xdr:cNvSpPr/></xdr:nvSpPr><xdr:spPr bwMode="auto"><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{blip}</xdr:spPr></xdr:sp><xdr:clientData/></xdr:twoCellAnchor></mc:Choice><mc:Fallback/></mc:AlternateContent>"#
    )
}

const PICTURE_ANCHOR: &str = r#"<xdr:twoCellAnchor><xdr:from><xdr:col>5</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>5</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>9</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="3" name="Picture 2" descr="logo"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor>"#;

/// Sheet "Controls" with an edit box (its `<controls>` entry, ctrlProp part and VML shape) whose
/// DrawingML anchor is `control_anchor(control_id, picture)`, after a picture when `picture`;
/// sheet "Other" holds a value.
fn package(control_id: u32, picture: bool) -> Vec<u8> {
    let workbook = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Controls" sheetId="1" r:id="rId1"/><sheet name="Other" sheetId="2" r:id="rId2"/></sheets></workbook>"#
    );
    let sheet1 = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="C1" t="b"><v>1</v></c></row></sheetData><pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/><drawing r:id="rId1"/><legacyDrawing r:id="rId2"/><controls><control shapeId="1025" r:id="rId3" name="Drop Down 1"/></controls></worksheet>"#
    );
    let sheet2 =
        format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData></worksheet>"#);
    let pic = if picture { PICTURE_ANCHOR } else { "" };
    let drawing = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}">{pic}{}</xdr:wsDr>"#,
        control_anchor(control_id, picture)
    );
    let root_rels = rels(&[("rId1", "officeDocument", "xl/workbook.xml")]);
    let wb_rels = rels(&[("rId1", "worksheet", "worksheets/sheet1.xml"), ("rId2", "worksheet", "worksheets/sheet2.xml")]);
    let sheet_rels = rels(&[
        ("rId1", "drawing", "../drawings/drawing1.xml"),
        ("rId2", "vmlDrawing", "../drawings/vmlDrawing1.vml"),
        ("rId3", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/ctrlProp", "../ctrlProps/ctrlProp1.xml"),
    ]);
    let drawing_rels = rels(&[("rId1", "image", "../media/image1.png"), ("rId2", "image", "../media/image2.png")]);
    let files: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", CONTENT_TYPES.as_bytes()),
        ("_rels/.rels", root_rels.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet1.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet2.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", drawing_rels.as_bytes()),
        ("xl/drawings/vmlDrawing1.vml", VML.as_bytes()),
        ("xl/ctrlProps/ctrlProp1.xml", CTRL_PROP.as_bytes()),
        ("xl/media/image1.png", PNG),
        ("xl/media/image2.png", FILL_PNG),
    ];
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
    /// Every internal relationship target exists and rel ids are unique per part.
    fn assert_no_dangling(&self) {
        for name in self.parts.keys().filter(|n| n.ends_with(".rels")) {
            let source = name.replace("_rels/", "").trim_end_matches(".rels").to_string();
            let mut ids = HashSet::new();
            for (id, _, target) in self.rels(&source) {
                assert!(ids.insert(id.clone()), "{name}: duplicate id {id}");
                assert!(self.parts.keys().any(|p| normalize(p) == normalize(&target)), "{name}: {id} → missing {target}");
            }
        }
    }
    fn sheet_part(&self, sheet: &str) -> String {
        let wb = xml::parse(self.text("xl/workbook.xml").as_bytes()).unwrap();
        let id = wb.child("sheets").unwrap().kids("sheet").find(|s| s.attr("name") == Some(sheet)).unwrap().attr("id").unwrap().to_string();
        self.target("xl/workbook.xml", &id)
    }
    /// The drawing part of `sheet` (through `<drawing r:id>`), with its XML.
    fn drawing(&self, sheet: &str) -> (String, String) {
        let part = self.sheet_part(sheet);
        let x = xml::parse(self.text(&part).as_bytes()).unwrap();
        let id = x.child("drawing").unwrap_or_else(|| panic!("{part} has no <drawing>")).attr("id").unwrap().to_string();
        let target = self.target(&part, &id);
        let text = self.text(&target);
        (target, text)
    }
}

fn cnvpr_ids(e: &El, out: &mut Vec<String>) {
    if e.name == "cNvPr" {
        out.push(e.attr("id").unwrap_or("").to_string());
    }
    for c in &e.children {
        cnvpr_ids(c, out);
    }
}

/// The `cNvPr` ids of a drawing, asserting they are unique.
fn unique_ids(drawing: &str) -> Vec<String> {
    let mut ids = vec![];
    cnvpr_ids(&xml::parse(drawing.as_bytes()).unwrap(), &mut ids);
    let set: HashSet<&String> = ids.iter().collect();
    assert_eq!(set.len(), ids.len(), "duplicate cNvPr ids {ids:?} in {drawing}");
    ids
}

fn roundtrip(wb: &Workbook) -> Unpacked {
    let u = Unpacked::new(&write_xlsx(wb).unwrap());
    u.assert_no_dangling();
    u
}

#[test]
fn control_only_drawing_survives_round_trip() {
    let (wb, _) = read_xlsx(&package(1025, false)).unwrap();
    assert!(wb.sheets[0].images.is_empty() && wb.sheets[0].shapes.is_empty() && wb.sheets[0].charts.is_empty());
    let bytes = write_xlsx(&wb).unwrap();
    // Twice: the saved file is read back and saved again.
    let (again, _) = read_xlsx(&bytes).unwrap();
    for u in [Unpacked::new(&bytes), roundtrip(&again)] {
        u.assert_no_dangling();
        let (part, x) = u.drawing("Controls");
        assert!(part.starts_with("xl/drawings/drawing"), "{part}");
        assert!(x.contains(r#"compatExt spid="_x0000_s1025""#), "{x}");
        assert!(x.contains("Requires=\"a14\""), "{x}");
        assert_eq!(unique_ids(&x), vec!["1025".to_string()]);
        // Schema order: drawing, legacyDrawing, controls.
        let sx = u.text(&u.sheet_part("Controls"));
        let (d, l, c) = (sx.find("<drawing ").unwrap(), sx.find("<legacyDrawing ").unwrap(), sx.find("<controls").unwrap());
        assert!(d < l && l < c, "{sx}");
        assert!(u.parts.values().any(|v| v.as_slice() == CTRL_PROP.as_bytes()));
        // The other sheet has no drawing.
        assert!(!u.text(&u.sheet_part("Other")).contains("<drawing"));
    }
}

#[test]
fn picture_and_control_shape_both_kept() {
    // The control's id is the one GridCraft would give its first object.
    let (wb, _) = read_xlsx(&package(2, true)).unwrap();
    assert_eq!(wb.sheets[0].images.len(), 1);
    let u = roundtrip(&wb);
    let (part, x) = u.drawing("Controls");
    let d = xml::parse(x.as_bytes()).unwrap();
    // The picture first, the control second, as in the source.
    let names: Vec<&str> = d.children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["twoCellAnchor", "AlternateContent"], "{x}");
    let ids = unique_ids(&x);
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"2".to_string()));
    assert!(x.contains(r#"compatExt spid="_x0000_s1025""#));
    // Both pictures resolve through the written drawing's relationships.
    let pic = d.children[0].path(&["pic", "blipFill", "blip"]).unwrap().attr("embed").unwrap();
    assert_eq!(u.parts.get(&u.target(&part, pic)).unwrap().as_slice(), PNG);
    let fill = d.children[1].path(&["Choice", "twoCellAnchor", "sp", "spPr", "blipFill", "blip"]).unwrap().attr("embed").unwrap();
    assert_eq!(u.parts.get(&u.target(&part, fill)).unwrap().as_slice(), FILL_PNG);
    // And survive a second round trip.
    let (again, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(again.sheets[0].images.len(), 1);
    let (_, x) = roundtrip(&again).drawing("Controls");
    assert_eq!(unique_ids(&x).len(), 2);
    assert!(x.contains(r#"compatExt spid="_x0000_s1025""#));
}

#[test]
fn added_chart_keeps_control_anchor() {
    let (mut wb, _) = read_xlsx(&package(2, false)).unwrap();
    let sheet = Arc::make_mut(&mut wb.sheets[0]);
    sheet.charts.push(Chart {
        rounded_corners: false,
        lang: None,
        print_settings: None,
        of_pie: None,
        alt: String::new(),
        theme_override: None,
        hidden: false,
        no_print: false,
        id: 99,
        kind: ChartKind::Line,
        anchor: Anchor { cell: CellRef::new(4, 4), width: 480.0, height: 300.0, ..Anchor::default() },
        title: None,
        series: vec![Series {
            name: None,
            categories: None,
            values: "Controls!$C$1:$C$1".into(),
            bubble_sizes: None,
            color: None,
            secondary: false,
            kind: None,
            smooth: false,
            trendlines: Vec::new(),
            explosion: 0,
            point_explosions: Vec::new(),
            bar_shape: None,
            quartiles: Default::default(),
        }],
        legend: LegendPos::Bottom,
        data_labels: false,
        gridlines: true,
        style: 2,
        hole_size: 75,
        first_slice_angle: 0,
        x_title: None,
        y_title: None,
        source: None,
        by_rows: false,
        pivot: None,
        data_table: None,
        scale: Default::default(),
        view_3d: None,
    });
    let u = roundtrip(&wb);
    let (part, x) = u.drawing("Controls");
    assert!(x.contains(r#"compatExt spid="_x0000_s1025""#), "{x}");
    let ids = unique_ids(&x);
    assert_eq!(ids.len(), 2, "{x}");
    let d = xml::parse(x.as_bytes()).unwrap();
    let frame = d.children.iter().find_map(|c| c.child("graphicFrame")).unwrap();
    let chart_id = frame.path(&["nvGraphicFramePr", "cNvPr"]).unwrap().attr("id").unwrap();
    assert_ne!(chart_id, "2");
    let rid = frame.path(&["graphic", "graphicData", "chart"]).unwrap().attr("id").unwrap();
    assert!(u.target(&part, rid).starts_with("xl/charts/chart"));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.sheets[0].charts.len(), 1);
}

#[test]
fn deleted_sheet_drops_kept_drawing() {
    let (mut wb, _) = read_xlsx(&package(1025, true)).unwrap();
    wb.sheets.remove(0);
    let u = roundtrip(&wb);
    for (name, data) in &u.parts {
        assert!(!name.starts_with("xl/drawings/") && !name.starts_with("xl/media/") && !name.starts_with("xl/ctrlProps/"), "{name} was kept");
        let text = String::from_utf8_lossy(data);
        assert!(!text.contains("_x0000_s1025") && !text.contains("compatExt"), "{name} still refers to the control");
    }
    assert!(!u.text(&u.sheet_part("Other")).contains("<drawing"));
}
