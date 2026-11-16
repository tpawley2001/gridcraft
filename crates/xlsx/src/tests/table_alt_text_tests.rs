//! A table's Alt Text ([MS-XLSX] 2.6.35: `x14:table` `altText` = title, `altTextSummary` =
//! description, in the table part's ext `{504A1905-F514-4f6f-8877-14C23A59335A}`) survives save,
//! as Excel's own re-save keeps it.

use std::io::Read;

use super::minimal;
use crate::{read_xlsx, write_xlsx};

const URI: &str = "{504A1905-F514-4f6f-8877-14C23A59335A}";

fn book(ext_attrs: Option<&str>) -> Vec<u8> {
    let data = r#"<sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Item</t></is></c><c r="B1" t="inlineStr"><is><t>Amount</t></is></c></row><row r="2"><c r="A2" t="inlineStr"><is><t>Rent</t></is></c><c r="B2"><v>800</v></c></row></sheetData><tableParts count="1"><tablePart r:id="rId1"/></tableParts>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/table" Target="../tables/table1.xml"/></Relationships>"#;
    let ext = ext_attrs
        .map(|a| format!(r#"<extLst><ext uri="{URI}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:table {a}/></ext></extLst>"#))
        .unwrap_or_default();
    let table = format!(
        r#"<?xml version="1.0"?><table xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" id="1" name="Income" displayName="Income" ref="A1:B2" totalsRowShown="0"><autoFilter ref="A1:B2"/><tableColumns count="2"><tableColumn id="1" name="Item"/><tableColumn id="2" name="Amount"/></tableColumns><tableStyleInfo name="TableStyleMedium2" showFirstColumn="0" showLastColumn="0" showRowStripes="1" showColumnStripes="0"/>{ext}</table>"#
    );
    minimal(data, &[("xl/worksheets/_rels/sheet1.xml.rels", rels), ("xl/tables/table1.xml", &table)], "", "")
}

/// The saved table part.
fn saved_table(bytes: &[u8]) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let names: Vec<String> = z.file_names().map(str::to_string).collect();
    let name = names.iter().find(|n| n.starts_with("xl/tables/") && n.ends_with(".xml")).expect("a table part is saved").clone();
    let mut s = String::new();
    z.by_name(&name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn round_trip(ext_attrs: Option<&str>) -> String {
    let (wb, _) = read_xlsx(&book(ext_attrs)).unwrap();
    let once = write_xlsx(&wb).unwrap();
    let (wb2, _) = read_xlsx(&once).unwrap();
    let twice = saved_table(&write_xlsx(&wb2).unwrap());
    assert_eq!(saved_table(&once), twice, "a second save writes the same table part");
    twice
}

fn unescape(v: &str) -> String {
    v.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// The attributes of the saved `x14:table` element (any prefix bound to the x14 namespace is fine,
/// but Excel's own files use `x14`).
fn alt(table: &str) -> Option<(Option<String>, Option<String>)> {
    let ext_at = table.to_ascii_lowercase().find(&URI.to_ascii_lowercase())?;
    let rest = &table[ext_at..];
    let el = rest.find(":table ").map(|i| &rest[i..])?;
    let el = &el[..el.find("/>").or_else(|| el.find('>')).unwrap()];
    let get = |name: &str| el.split(&format!(" {name}=\"")).nth(1).and_then(|s| s.split('"').next()).map(unescape);
    Some((get("altText"), get("altTextSummary")))
}

#[test]
fn title_and_description_survive_save() {
    let t = round_trip(Some(r#"altText="Alt title" altTextSummary="Alt text""#));
    assert_eq!(alt(&t), Some((Some("Alt title".into()), Some("Alt text".into()))), "{t}");
    // extLst is the table's last child (ECMA-376 18.5.1.2), after tableStyleInfo.
    assert!(t.find("<tableStyleInfo").unwrap() < t.find("<extLst").unwrap(), "{t}");
    assert!(t.trim_end().ends_with("</extLst></table>"), "{t}");
}

#[test]
fn a_description_alone_survives_save() {
    let t = round_trip(Some(r#"altTextSummary="Invoice list with item #, description, quantity""#));
    assert_eq!(alt(&t), Some((None, Some("Invoice list with item #, description, quantity".into()))), "{t}");
}

#[test]
fn special_characters_round_trip() {
    let t = round_trip(Some(r#"altText="A &amp; B &lt;x&gt; &quot;q&quot;""#));
    assert_eq!(alt(&t), Some((Some("A & B <x> \"q\"".into()), None)), "{t}");
}

#[test]
fn no_alt_text_writes_no_ext() {
    let t = round_trip(None);
    assert!(!t.contains("extLst"), "{t}");
}
