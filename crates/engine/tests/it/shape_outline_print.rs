//! Printed shape outlines and text boxes, like Excel: a colour's `a:shade` is applied in linear
//! light (ECMA-376 Part 1 §20.1.2.3.31), the outline is `a:ln w` EMU wide (§20.1.2.2.24) and the
//! text sits at the `a:bodyPr` insets (§21.1.2.1.1; defaults 91440 / 45720 EMU).
//!
//! Expected colours were measured in Excel's own PDF prints: `lt1` (FFFFFF) shade 50 % → BCBCBC,
//! FFFFFF shade 85 % → EDEDED, 4F81BD shade 50 % → 385D8A. A 9525 EMU outline prints 0.75 pt wide,
//! and a text box's text starts 7.2 pt (the default left inset) from its frame.

use std::io::Write;

use gridcraft_engine::Session;
use serde_json::json;

const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const NS_PKG_R: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "application/vnd.openxmlformats-officedocument";

fn rels(id: &str, kind: &str, target: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><Relationships xmlns="{NS_PKG_R}"><Relationship Id="{id}" Type="{NS_R}/{kind}" Target="{target}"/></Relationships>"#
    )
}

/// One rectangle text box anchored E9:G14 (no cells), with `sp_pr` inside `xdr:spPr` after the
/// geometry and `body_pr` as its `a:bodyPr` element.
fn package(sp_pr: &str, body_pr: &str) -> Vec<u8> {
    let drawing = format!(
        r#"<?xml version="1.0"?><xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><xdr:twoCellAnchor><xdr:from><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>7</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>14</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="TextBox 1"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="2438400" y="1524000"/><a:ext cx="1828800" cy="1143000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{sp_pr}</xdr:spPr><xdr:txBody>{body_pr}<a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"/><a:t>Some text</a:t></a:r></a:p></xdr:txBody></xdr:sp><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    let sheet = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheetData/><drawing r:id="rId1"/></worksheet>"#);
    let parts: Vec<(&str, String)> = vec![
        (
            "[Content_Types].xml",
            format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="{CT}.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="{CT}.spreadsheetml.worksheet+xml"/><Override PartName="/xl/drawings/drawing1.xml" ContentType="{CT}.drawing+xml"/></Types>"#
            ),
        ),
        ("_rels/.rels", rels("rId1", "officeDocument", "xl/workbook.xml")),
        (
            "xl/workbook.xml",
            format!(
                r#"<?xml version="1.0"?><workbook xmlns="{NS_MAIN}" xmlns:r="{NS_R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            ),
        ),
        ("xl/_rels/workbook.xml.rels", rels("rId1", "worksheet", "worksheets/sheet1.xml")),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/worksheets/_rels/sheet1.xml.rels", rels("rId1", "drawing", "../drawings/drawing1.xml")),
        ("xl/drawings/drawing1.xml", drawing),
    ];
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    for (name, body) in &parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

/// The PDF's uncompressed content, as text.
fn pdf_text(sp_pr: &str, body_pr: &str) -> String {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": "outline.xlsx", "base64": gridcraft_engine::io::base64_encode(&package(sp_pr, body_pr))})).unwrap();
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    let bytes = gridcraft_engine::io::base64_decode(r["base64"].as_str().unwrap()).unwrap();
    String::from_utf8_lossy(&bytes).into_owned()
}

fn nums(line: &str, n: usize) -> Option<Vec<f32>> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    if toks.len() < n + 1 {
        return None;
    }
    let start = toks.len() - 1 - n;
    toks[start..toks.len() - 1].iter().map(|t| t.parse().ok()).collect()
}

/// Every colour set by `op` (`RG` for strokes, `rg` for fills; `G`/`g` greys count too), 0..255.
fn colours(pdf: &str, op: &str) -> Vec<[u8; 3]> {
    let grey = op.to_lowercase().replace("rg", "g");
    let grey = if op == "RG" { "G".to_string() } else { grey };
    let to = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    let mut out = Vec::new();
    for l in pdf.lines().flat_map(|l| l.split('\n')) {
        let l = l.trim();
        if l.ends_with(&format!(" {op}")) {
            if let Some(v) = nums(l, 3) {
                out.push([to(v[0]), to(v[1]), to(v[2])]);
            }
        } else if l.ends_with(&format!(" {grey}")) && l.split_whitespace().count() == 2 {
            if let Some(v) = nums(l, 1) {
                out.push([to(v[0]); 3]);
            }
        }
    }
    out
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (*x as i16 - y as i16).abs() <= 1)
}

fn has_colour(pdf: &str, op: &str, want: [u8; 3]) -> bool {
    colours(pdf, op).into_iter().any(|c| near(c, want))
}

/// Every line width set with `w` (the first number of a line containing ` w`).
fn widths(pdf: &str) -> Vec<f32> {
    pdf.lines()
        .filter_map(|l| {
            let toks: Vec<&str> = l.split_whitespace().collect();
            let i = toks.iter().position(|t| *t == "w")?;
            if i == 0 { None } else { toks[i - 1].parse().ok() }
        })
        .collect()
}

/// The left edge of the shape's frame: the smallest x of the path ops (`m`, `l`, `re`) drawn
/// before the first text matrix, and the x of that text matrix (`1 0 0 1 x y Tm`).
fn frame_left_and_text_x(pdf: &str) -> (f32, f32) {
    let mut min_x = f32::MAX;
    for l in pdf.lines() {
        let l = l.trim();
        if l.ends_with(" Tm") {
            let v = nums(l, 6).expect("text matrix");
            assert!(min_x < f32::MAX, "no frame drawn before the text:\n{pdf}");
            return (min_x, v[4]);
        }
        // Path segments can share a line ("x y m x y l ... h B"): walk the tokens.
        let toks: Vec<&str> = l.split_whitespace().collect();
        for (i, t) in toks.iter().enumerate() {
            let x = match *t {
                "m" | "l" if i >= 2 => toks[i - 2].parse::<f32>().ok(),
                "re" if i >= 4 && !toks.get(i + 1).is_some_and(|n| *n == "W") => toks[i - 4].parse::<f32>().ok(),
                _ => None,
            };
            if let Some(x) = x {
                min_x = min_x.min(x);
            }
        }
    }
    panic!("no text in the PDF:\n{pdf}");
}

const BODY: &str = r#"<a:bodyPr wrap="square" rtlCol="0" anchor="t"/>"#;

#[test]
fn text_box_default_outline_lt1_shade_50_prints_bcbcbc() {
    let pdf = pdf_text(
        r#"<a:solidFill><a:schemeClr val="lt1"/></a:solidFill><a:ln w="9525" cmpd="sng"><a:solidFill><a:schemeClr val="lt1"><a:shade val="50000"/></a:schemeClr></a:solidFill></a:ln>"#,
        BODY,
    );
    assert!(has_colour(&pdf, "RG", [0xBC, 0xBC, 0xBC]), "outline should be BCBCBC, strokes {:?}", colours(&pdf, "RG"));
    assert!(!has_colour(&pdf, "RG", [0xFF, 0xFF, 0xFF]), "outline must not print white, strokes {:?}", colours(&pdf, "RG"));
}

#[test]
fn srgb_fill_shade_85_prints_ededed() {
    let pdf = pdf_text(r#"<a:solidFill><a:srgbClr val="FFFFFF"><a:shade val="85000"/></a:srgbClr></a:solidFill>"#, BODY);
    assert!(has_colour(&pdf, "rg", [0xED, 0xED, 0xED]), "fill should be EDEDED, fills {:?}", colours(&pdf, "rg"));
}

#[test]
fn srgb_outline_shade_50_is_linear_light_385d8a() {
    let pdf =
        pdf_text(r#"<a:noFill/><a:ln w="9525"><a:solidFill><a:srgbClr val="4F81BD"><a:shade val="50000"/></a:srgbClr></a:solidFill></a:ln>"#, BODY);
    assert!(has_colour(&pdf, "RG", [0x38, 0x5D, 0x8A]), "outline should be 385D8A, strokes {:?}", colours(&pdf, "RG"));
}

#[test]
fn outline_width_comes_from_ln_w_in_emu() {
    let thin = pdf_text(r#"<a:noFill/><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#, BODY);
    assert!(widths(&thin).iter().any(|w| (w - 0.75).abs() <= 0.02), "9525 EMU should stroke 0.75 pt, widths {:?}", widths(&thin));
    let thick = pdf_text(r#"<a:noFill/><a:ln w="28575"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#, BODY);
    assert!(widths(&thick).iter().any(|w| (w - 2.25).abs() <= 0.02), "28575 EMU should stroke 2.25 pt, widths {:?}", widths(&thick));
}

#[test]
fn text_starts_at_the_default_left_inset() {
    let pdf = pdf_text(
        r#"<a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#,
        BODY,
    );
    let (left, text_x) = frame_left_and_text_x(&pdf);
    assert!((text_x - left - 7.2).abs() <= 0.3, "text should start 7.2 pt inside the frame: frame {left}, text {text_x}");
}

#[test]
fn text_starts_at_an_explicit_left_inset() {
    let body = r#"<a:bodyPr wrap="square" lIns="0" tIns="0" rIns="0" bIns="0" anchor="t"/>"#;
    let pdf = pdf_text(
        r#"<a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:ln w="9525"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln>"#,
        body,
    );
    let (left, text_x) = frame_left_and_text_x(&pdf);
    assert!((text_x - left).abs() <= 0.3, "lIns=0 puts the text on the frame: frame {left}, text {text_x}");
}

/// Guard: an outline without a transform keeps its colour.
#[test]
fn plain_outline_colour_is_unchanged() {
    let pdf = pdf_text(r#"<a:noFill/><a:ln w="9525"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:ln>"#, BODY);
    assert!(has_colour(&pdf, "RG", [0xFF, 0, 0]), "red outline, strokes {:?}", colours(&pdf, "RG"));
}
