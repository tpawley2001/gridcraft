//! Synthetic XLSX acceptance tests; no vendor-produced fixtures.
use super::minimal;
use crate::package::Package;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};

fn save(bytes: &[u8]) -> Vec<u8> {
    let (wb, _) = read_xlsx(bytes).unwrap();
    write_xlsx(&wb).unwrap()
}

fn note(shape_attrs: &str, flags: &str) -> Vec<u8> {
    let v = format!(
        r##"<xml xmlns:v="urn:schemas-microsoft-com:vml" xmlns:x="urn:schemas-microsoft-com:office:excel"><v:shape id="_x0000_s1025" type="#_x0000_t202" {shape_attrs}><v:textbox><div>note</div></v:textbox><x:ClientData ObjectType="Note">{flags}<x:Anchor>1,0,1,0,3,0,4,0</x:Anchor><x:Row>1</x:Row><x:Column>1</x:Column></x:ClientData></v:shape></xml>"##
    );
    let rels = r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="comments" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments" Target="../comments1.xml"/><Relationship Id="vml" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/vmlDrawing" Target="../drawings/vmlDrawing1.vml"/></Relationships>"#;
    let c = r#"<comments xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><authors><author>Example</author></authors><commentList><comment ref="B2" authorId="0"><text><t>note</t></text></comment></commentList></comments>"#;
    minimal(
        r#"<sheetData/><legacyDrawing r:id="vml"/>"#,
        &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/comments1.xml", c), ("xl/drawings/vmlDrawing1.vml", &v)],
        "",
        "",
    )
}
fn shape(attrs: &str, flags: &str) -> El {
    let out = save(&save(&note(attrs, flags)));
    let mut pkg = Package::open(&out).unwrap();
    // Resolve the part through its relationship rather than assuming the writer's VML numbering.
    let rels = pkg.rels("xl/worksheets/sheet1.xml").unwrap();
    let rel = rels.iter().find(|r| r.kind == "vmlDrawing").unwrap();
    let x = parse(&pkg.read(&rel.target).unwrap().unwrap()).unwrap();
    x.kids("shape").find(|s| s.child("ClientData").is_some_and(|c| c.attr("ObjectType") == Some("Note"))).unwrap().clone()
}
#[test]
fn note_colors_and_weight_survive_twice() {
    let s = shape(r##"fillcolor="#036 [56]" strokecolor="#669 [54]" strokeweight="3pt""##, "<x:MoveWithCells/>");
    assert_eq!(s.attr("fillcolor").unwrap().to_ascii_lowercase(), "#003366");
    assert_eq!(s.attr("strokecolor").unwrap().to_ascii_lowercase(), "#666699");
    assert_eq!(s.attr("strokeweight"), Some("3pt"));
}
#[test]
fn no_fill_and_no_stroke_are_retained() {
    let s = shape(r#"filled="f" stroked="false""#, "");
    for k in ["filled", "stroked"] {
        assert!(matches!(s.attr(k), Some("f" | "false" | "0")), "{k}");
    }
}
#[test]
fn placement_flags_are_independent_and_absence_stays_absent() {
    for (flags, mov, size) in [
        ("", false, false),
        ("<x:MoveWithCells/>", true, false),
        ("<x:SizeWithCells/>", false, true),
        ("<x:MoveWithCells>False</x:MoveWithCells><x:SizeWithCells>True</x:SizeWithCells>", false, true),
    ] {
        let s = shape("", flags);
        let c = s.child("ClientData").unwrap();
        for (k, on) in [("MoveWithCells", mov), ("SizeWithCells", size)] {
            let actual = c.child(k).is_some_and(|e| e.text.is_empty() || e.text.eq_ignore_ascii_case("true") || e.text == "1");
            assert_eq!(actual, on, "{k}: {flags}");
        }
    }
}
