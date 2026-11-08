//! Pictures keep their locks (`a:picLocks`, ECMA-376 Part 1 §20.1.2.2.31) through open and save: a picture
//! whose aspect ratio is unlocked stays unlocked, the other locks are kept, and a picture without a
//! `cNvPicPr` (or made in the app) is still written with Excel's default `noChangeAspect="1"`.
use super::minimal;
use crate::{read_xlsx, write_xlsx};
use std::io::Read;

/// An .xlsx with one picture whose `xdr:nvPicPr` holds `cnv_pic_pr` after its `cNvPr`.
fn with_picture(cnv_pic_pr: &str) -> Vec<u8> {
    let drawing = format!(
        r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><xdr:twoCellAnchor><xdr:from><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>8</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to><xdr:pic><xdr:nvPicPr><xdr:cNvPr id="2" name="Picture 1"/>{cnv_pic_pr}</xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/><a:stretch><a:fillRect/></a:stretch></xdr:blipFill><xdr:spPr><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr></xdr:pic><xdr:clientData/></xdr:twoCellAnchor></xdr:wsDr>"#
    );
    minimal(
        r#"<sheetData/><drawing r:id="rId1"/>"#,
        &[
            (
                "xl/worksheets/_rels/sheet1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#,
            ),
            ("xl/drawings/drawing1.xml", &drawing),
            (
                "xl/drawings/_rels/drawing1.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/p.png"/></Relationships>"#,
            ),
            ("xl/media/p.png", "synthetic picture bytes"),
        ],
        "",
        "",
    )
}

/// The saved `xdr:cNvPicPr` element of the only picture, verbatim, after `saves` open + save rounds.
fn saved_cnv_pic_pr(bytes: &[u8], saves: usize) -> String {
    let mut bytes = bytes.to_vec();
    for _ in 0..saves {
        let (wb, _) = read_xlsx(&bytes).unwrap();
        assert_eq!(wb.sheet(0).unwrap().images.len(), 1);
        bytes = write_xlsx(&wb).unwrap();
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).unwrap();
    let name = zip.file_names().find(|n| n.starts_with("xl/drawings/drawing") && n.ends_with(".xml")).unwrap().to_string();
    let mut xml = String::new();
    zip.by_name(&name).unwrap().read_to_string(&mut xml).unwrap();
    let start = xml.find("<xdr:cNvPicPr").expect("a cNvPicPr");
    let rest = &xml[start..];
    if rest.starts_with("<xdr:cNvPicPr/>") {
        return "<xdr:cNvPicPr/>".to_string();
    }
    rest[..rest.find("</xdr:cNvPicPr>").expect("a closed cNvPicPr") + "</xdr:cNvPicPr>".len()].to_string()
}

#[test]
fn unlocked_aspect_ratio_stays_unlocked() {
    // Format Picture > Size > Lock aspect ratio turned off: Excel writes an empty cNvPicPr.
    assert_eq!(saved_cnv_pic_pr(&with_picture("<xdr:cNvPicPr/>"), 2), "<xdr:cNvPicPr/>");
    // Written out at its default, the same thing.
    assert_eq!(saved_cnv_pic_pr(&with_picture(r#"<xdr:cNvPicPr><a:picLocks noChangeAspect="0"/></xdr:cNvPicPr>"#), 2), "<xdr:cNvPicPr/>");
}

#[test]
fn locked_aspect_ratio_stays_locked() {
    let x = saved_cnv_pic_pr(&with_picture(r#"<xdr:cNvPicPr><a:picLocks noChangeAspect="1"/></xdr:cNvPicPr>"#), 2);
    assert_eq!(x, r#"<xdr:cNvPicPr><a:picLocks noChangeAspect="1"/></xdr:cNvPicPr>"#);
}

#[test]
fn every_lock_is_kept_in_schema_order() {
    // Given out of order and with `true`; written in CT_PictureLocking order with "1".
    let given = r#"<xdr:cNvPicPr><a:picLocks noCrop="true" noMove="1" noGrp="1" noSelect="1" noRot="1" noChangeAspect="1" noResize="1" noEditPoints="1" noAdjustHandles="1" noChangeArrowheads="1" noChangeShapeType="1"/></xdr:cNvPicPr>"#;
    let want = r#"<xdr:cNvPicPr><a:picLocks noGrp="1" noSelect="1" noRot="1" noChangeAspect="1" noMove="1" noResize="1" noEditPoints="1" noAdjustHandles="1" noChangeArrowheads="1" noChangeShapeType="1" noCrop="1"/></xdr:cNvPicPr>"#;
    assert_eq!(saved_cnv_pic_pr(&with_picture(given), 2), want);
}

#[test]
fn some_locks_without_the_aspect_lock() {
    let x = saved_cnv_pic_pr(&with_picture(r#"<xdr:cNvPicPr><a:picLocks noCrop="1" noRot="1"/></xdr:cNvPicPr>"#), 2);
    assert_eq!(x, r#"<xdr:cNvPicPr><a:picLocks noRot="1" noCrop="1"/></xdr:cNvPicPr>"#);
}

#[test]
fn a_picture_without_cnv_pic_pr_gets_excels_default_lock() {
    // Guard: unchanged behaviour for files without the element (and for pictures made in the app).
    let x = saved_cnv_pic_pr(&with_picture(""), 1);
    assert_eq!(x, r#"<xdr:cNvPicPr><a:picLocks noChangeAspect="1"/></xdr:cNvPicPr>"#);
}
