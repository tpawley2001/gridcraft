//! Synthetic DrawingML objects retain their individual visibility through save/reopen.
use super::minimal;
use crate::{read_xlsx, write_xlsx};
use std::io::Read;

#[test]
fn hidden_objects_roundtrip_across_anchor_and_object_types() {
    for flag in ["1", "true", "0", "false", ""] {
        let attr = if flag.is_empty() { String::new() } else { format!(r#" hidden="{flag}""#) };
        let shape = format!(r#"<xdr:sp><xdr:nvSpPr><xdr:cNvPr id="1" name="Shape"{attr}/></xdr:nvSpPr><xdr:spPr/></xdr:sp>"#);
        let connector = shape.replace("xdr:sp>", "xdr:cxnSp>").replace("nvSpPr", "nvCxnSpPr");
        let picture = format!(
            r#"<xdr:pic><xdr:nvPicPr><xdr:cNvPr id="3" name="Picture"{attr}/></xdr:nvPicPr><xdr:blipFill><a:blip r:embed="rId1"/></xdr:blipFill></xdr:pic>"#
        );
        let chart = format!(
            r#"<xdr:graphicFrame><xdr:nvGraphicFramePr><xdr:cNvPr id="4" name="Chart"{attr}/></xdr:nvGraphicFramePr><a:graphic><a:graphicData><c:chart r:id="rId2"/></a:graphicData></a:graphic></xdr:graphicFrame>"#
        );
        let mut anchors = String::new();
        for object in [&shape, &connector, &picture, &chart] {
            for kind in ["twoCellAnchor", "oneCellAnchor", "absoluteAnchor"] {
                anchors.push_str(&format!(r#"<xdr:{kind}><xdr:from><xdr:col>0</xdr:col><xdr:row>0</xdr:row></xdr:from><xdr:to><xdr:col>10</xdr:col><xdr:row>50</xdr:row></xdr:to><xdr:pos x="0" y="0"/><xdr:ext cx="952500" cy="952500"/>{object}</xdr:{kind}>"#));
            }
        }
        let drawing = format!(
            r#"<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">{anchors}</xdr:wsDr>"#
        );
        let bytes = minimal(
            r#"<sheetData/><drawing r:id="rId1"/>"#,
            &[
                (
                    "xl/worksheets/_rels/sheet1.xml.rels",
                    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/drawing" Target="../drawings/drawing1.xml"/></Relationships>"#,
                ),
                ("xl/drawings/drawing1.xml", &drawing),
                (
                    "xl/drawings/_rels/drawing1.xml.rels",
                    r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/p.png"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="../charts/c.xml"/></Relationships>"#,
                ),
                ("xl/media/p.png", "synthetic picture bytes"),
                (
                    "xl/charts/c.xml",
                    r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:chart><c:plotArea><c:barChart><c:barDir val="col"/></c:barChart></c:plotArea></c:chart></c:chartSpace>"#,
                ),
            ],
            "",
            "",
        );
        let hidden = flag == "1" || flag == "true";
        let mut wb = read_xlsx(&bytes).unwrap().0;
        for pass in 0..2 {
            let sh = wb.sheet(0).unwrap();
            assert_eq!((sh.shapes.len(), sh.images.len(), sh.charts.len()), (6, 3, 3), "pass {pass}");
            assert!(sh.shapes.iter().all(|s| s.hidden == hidden));
            assert!(sh.images.iter().all(|i| i.hidden == hidden));
            assert!(sh.charts.iter().all(|c| c.hidden == hidden));
            let saved = write_xlsx(&wb).unwrap();
            let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&saved)).unwrap();
            let mut xml = String::new();
            zip.by_name("xl/drawings/drawing1.xml").unwrap().read_to_string(&mut xml).unwrap();
            assert_eq!(xml.matches(r#" hidden="1""#).count(), if hidden { 12 } else { 0 });
            wb = read_xlsx(&saved).unwrap().0;
        }
    }
}
