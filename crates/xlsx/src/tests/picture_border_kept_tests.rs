//! A picture's border (`xdr:pic/xdr:spPr/a:ln`, ECMA-376 Part 1 §20.1.2.2.24: Picture Border
//! colour, weight and dashes) survives open + save. Packages are built in code.

use std::io::Read;

use super::make_zip;
use crate::{read_xlsx, write_xlsx};

const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const PNG: &[u8] = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\xf8\xcf\xc0\x00\x00\x03\x01\x01\x00\xc9\xfe\x92\xef\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
const RED: &str = r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#;

/// One sheet whose drawing holds one picture of image1.png; `ln` is what follows `a:prstGeom`
/// inside its `xdr:spPr`.
fn package(ln: &str) -> Vec<u8> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>"#;
    let root = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#
    );
    let wb = format!(
        r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    let wb_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#
    );
    let sheet = format!(
        r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><drawing r:id="rId1"/></worksheet>"#
    );
    let sheet_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#
    );
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="{NS_R}"><xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>6</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1"/><xdr:cNvPicPr/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:xfrm><a:off x="609600" y="190500"/><a:ext cx="1828800" cy="952500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{ln}</xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    let d_rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{NS_R}/image" Target="../media/image1.png"/></Relationships>"#
    );
    make_zip(&[
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wb_rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/_rels/sheet1.xml.rels", sheet_rels.as_bytes()),
        ("xl/drawings/drawing1.xml", drawing.as_bytes()),
        ("xl/drawings/_rels/drawing1.xml.rels", d_rels.as_bytes()),
        ("xl/media/image1.png", PNG),
    ])
}

/// The picture's `xdr:spPr` (from `<xdr:spPr` up to `</xdr:spPr>`) in a saved workbook.
fn sp_pr_in(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut xml = String::new();
    z.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut xml).unwrap();
    let pic = xml.find("<xdr:pic>").unwrap_or_else(|| panic!("the picture was not written: {xml}"));
    let rest = &xml[pic..];
    let start = rest.find("<xdr:spPr").unwrap_or_else(|| panic!("the picture has no xdr:spPr: {rest}"));
    let end = rest[start..].find("</xdr:spPr>").unwrap_or_else(|| panic!("xdr:spPr is not closed: {rest}"));
    rest[start..start + end].to_string()
}

/// Open + save a picture whose `xdr:spPr` ends with `ln`; the written `xdr:spPr`.
fn saved(ln: &str) -> String {
    let (wb, _) = read_xlsx(&package(ln)).unwrap();
    assert_eq!(wb.sheets[0].images.len(), 1, "the picture opens");
    sp_pr_in(&write_xlsx(&wb).unwrap())
}

/// `needle` is in `hay`, after every occurrence of `before`.
fn after(hay: &str, needle: &str, before: &str) -> bool {
    match (hay.find(needle), hay.rfind(before)) {
        (Some(n), Some(b)) => n > b,
        _ => false,
    }
}

/// A red 3 pt border is written back with its colour and weight, after the geometry.
#[test]
fn border_colour_and_weight_are_kept() {
    let got = saved(&format!(r#"<a:ln w="38100">{RED}</a:ln>"#));
    assert_eq!(got.matches("<a:ln").count(), 1, "{got}");
    assert!(after(&got, r#"<a:ln w="38100">"#, "</a:prstGeom>"), "a:ln follows the geometry: {got}");
    assert!(after(&got, r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#, "<a:ln "), "{got}");
    assert!(got.ends_with("</a:ln>"), "nothing follows the border: {got}");
}

/// A theme colour stays a theme colour, with its adjustments.
#[test]
fn theme_coloured_border_is_kept_as_written() {
    let got = saved(r#"<a:ln w="6350"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></a:ln>"#);
    assert!(after(&got, r#"<a:solidFill><a:schemeClr val="tx1"/></a:solidFill>"#, r#"<a:ln w="6350">"#), "{got}");
    let got = saved(r#"<a:ln w="12700"><a:solidFill><a:schemeClr val="accent2"><a:lumMod val="75000"/></a:schemeClr></a:solidFill></a:ln>"#);
    assert!(got.contains(r#"<a:schemeClr val="accent2"><a:lumMod val="75000"/></a:schemeClr>"#), "{got}");
}

/// A dashed border keeps its dash after the colour; a solid one writes no dash pattern.
#[test]
fn border_dash_is_kept() {
    for dash in ["sysDash", "dash", "sysDot", "lgDashDotDot"] {
        let got = saved(&format!(r#"<a:ln w="19050">{RED}<a:prstDash val="{dash}"/></a:ln>"#));
        assert!(after(&got, &format!(r#"<a:prstDash val="{dash}"/>"#), "</a:solidFill>"), "{dash}: {got}");
        assert_eq!(got.matches("<a:prstDash").count(), 1, "{dash}: {got}");
    }
    for ln in
        [format!(r#"<a:ln w="19050">{RED}<a:prstDash val="solid"/></a:ln>"#), format!(r#"<a:ln w="19050">{RED}<a:prstDash val="zigzag"/></a:ln>"#)]
    {
        let got = saved(&ln).replace(r#"<a:prstDash val="solid"/>"#, "");
        assert!(!got.contains("prstDash"), "{got}");
        assert!(got.contains(r#"<a:ln w="19050">"#) && got.contains(r#"<a:srgbClr val="FF0000"/>"#), "the border itself is kept: {got}");
    }
}

/// A border without a stated weight is written without one; an out-of-range weight is dropped,
/// the border stays.
#[test]
fn weight_is_written_only_when_stated_and_valid() {
    let got = saved(&format!("<a:ln>{RED}</a:ln>"));
    assert!(got.contains(r#"<a:ln><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln>"#), "{got}");
    for w in ["-5", "20116801", "thick"] {
        let got = saved(&format!(r#"<a:ln w="{w}">{RED}</a:ln>"#));
        assert!(got.contains(r#"<a:ln><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln>"#), "w={w}: {got}");
    }
    let got = saved(&format!(r#"<a:ln w="20116800">{RED}</a:ln>"#));
    assert!(got.contains(r#"<a:ln w="20116800">"#), "{got}");
}

/// A picture without a border, or with its line turned off, gets no border.
#[test]
fn no_border_is_not_invented() {
    for ln in ["", "<a:ln><a:noFill/></a:ln>", r#"<a:ln w="9525"/>"#, "<a:noFill/>"] {
        let got = saved(ln);
        assert!(!got.contains("<a:solidFill"), "{ln}: {got}");
        assert!(!got.contains("prstDash"), "{ln}: {got}");
    }
    assert!(!saved("").contains("<a:ln"), "a plain picture is written as before");
}

/// The border survives a second open + save unchanged.
#[test]
fn border_survives_a_second_save() {
    let (wb, _) = read_xlsx(&package(&format!(r#"<a:ln w="25400">{RED}<a:prstDash val="dashDot"/></a:ln>"#))).unwrap();
    let first = write_xlsx(&wb).unwrap();
    let (again, _) = read_xlsx(&first).unwrap();
    let (a, b) = (sp_pr_in(&first), sp_pr_in(&write_xlsx(&again).unwrap()));
    assert_eq!(a, b);
    assert!(b.contains(r#"<a:ln w="25400">"#) && b.contains(r#"<a:prstDash val="dashDot"/>"#), "{b}");
}
