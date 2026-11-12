//! `<hyperlink display="...">` (ECMA-376 Part 1 §18.3.1.47), the link's stored display string,
//! survives open + save for links to a web page as well as links inside the workbook. Packages
//! are built in code.

use std::io::Read;

use super::minimal;
use crate::xml::{self, El};
use crate::{read_xlsx, write_xlsx};

const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Open + save a sheet with five links; the saved `<hyperlink>` elements.
fn saved_links() -> Vec<El> {
    let rels = format!(
        r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/hyperlink" Target="http://www.perl.com/" TargetMode="External"/><Relationship Id="rId2" Type="{REL}/hyperlink" Target="https://example.com/tom" TargetMode="External"/><Relationship Id="rId3" Type="{REL}/hyperlink" Target="https://example.com/plain" TargetMode="External"/></Relationships>"#
    );
    let cell = |r: u32, t: &str| format!(r#"<row r="{r}"><c r="A{r}" t="inlineStr"><is><t>{t}</t></is></c></row>"#);
    let body = format!(
        r#"<sheetData>{}{}{}{}{}</sheetData><hyperlinks><hyperlink ref="A1" r:id="rId1" display="http://www.perl.com/"/><hyperlink ref="A2" r:id="rId2" tooltip="Go" display="Tom &amp; Co"/><hyperlink ref="A3" r:id="rId3"/><hyperlink ref="A4" location="Data!B5" display="Jump to totals"/><hyperlink ref="A5" location="Data!B6" display="Data!B6"/></hyperlinks>"#,
        cell(1, "Perl"),
        cell(2, "Tom"),
        cell(3, "Plain"),
        cell(4, "Totals"),
        cell(5, "Next")
    );
    let bytes = minimal(&body, &[("xl/worksheets/_rels/sheet1.xml.rels", &rels)], "", "");
    let out = write_xlsx(&read_xlsx(&bytes).unwrap().0).unwrap();
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
    let name = z.file_names().find(|n| n.starts_with("xl/worksheets/sheet")).unwrap().to_string();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    let sheet = xml::parse(s.as_bytes()).unwrap();
    sheet.child("hyperlinks").expect("hyperlinks").kids("hyperlink").cloned().collect()
}

/// The saved link on `cell`.
fn link<'a>(links: &'a [El], cell: &str) -> &'a El {
    links.iter().find(|l| l.attr("ref") == Some(cell)).unwrap_or_else(|| panic!("no hyperlink on {cell}"))
}

#[test]
fn hyperlink_display_kept_on_web_links() {
    let links = saved_links();
    assert_eq!(links.len(), 5);
    let perl = link(&links, "A1");
    assert!(perl.attr("id").is_some(), "still an external link");
    assert_eq!(perl.attr("display"), Some("http://www.perl.com/"));
    let tom = link(&links, "A2");
    assert_eq!((tom.attr("display"), tom.attr("tooltip")), (Some("Tom & Co"), Some("Go")));
}

#[test]
fn hyperlink_display_kept_absent_stays_absent_on_web_links() {
    let links = saved_links();
    let plain = link(&links, "A3");
    assert!(plain.attr("id").is_some(), "still an external link");
    assert_eq!(plain.attr("display"), None, "no display string invented");
}

#[test]
fn hyperlink_display_kept_on_links_inside_the_workbook() {
    let links = saved_links();
    let totals = link(&links, "A4");
    assert_eq!((totals.attr("location"), totals.attr("display")), (Some("Data!B5"), Some("Jump to totals")));
    let next = link(&links, "A5");
    assert_eq!((next.attr("location"), next.attr("display")), (Some("Data!B6"), Some("Data!B6")));
}
