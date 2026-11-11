//! A chart's own print settings (`c:printSettings`, ECMA-376 Part 1 §21.2.2.149: `c:headerFooter`
//! §21.2.2.79, `c:pageMargins` §21.2.2.133, `c:pageSetup` §21.2.2.135) are kept when the chart is
//! opened and saved. Excel writes the element at the end of every chart part it saves.

use gridcraft_model::{Chart, Sheet, Workbook};
use std::sync::Arc;

use crate::chart::{read_chart, write_chart};
use crate::xml::parse;

/// A clustered column chart followed by `tail` (the `c:printSettings` under test, or nothing).
fn chart_xml(tail: &str) -> String {
    format!(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart><c:plotArea>\
         <c:barChart><c:barDir val=\"col\"/><c:grouping val=\"clustered\"/><c:varyColors val=\"0\"/>\
         <c:ser><c:idx val=\"0\"/><c:order val=\"0\"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f></c:strRef></c:tx>\
         <c:cat><c:numRef><c:f>Sheet1!$A$2:$A$6</c:f></c:numRef></c:cat>\
         <c:val><c:numRef><c:f>Sheet1!$B$2:$B$6</c:f></c:numRef></c:val></c:ser>\
         <c:axId val=\"10\"/><c:axId val=\"20\"/></c:barChart>\
         <c:catAx><c:axId val=\"10\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"b\"/><c:crossAx val=\"20\"/></c:catAx>\
         <c:valAx><c:axId val=\"20\"/><c:scaling><c:orientation val=\"minMax\"/></c:scaling><c:delete val=\"0\"/>\
         <c:axPos val=\"l\"/><c:crossAx val=\"10\"/></c:valAx>\
         </c:plotArea></c:chart>{tail}</c:chartSpace>"
    )
}

/// Excel's usual form (1,253 of 1,322 sampled Excel-saved charts).
const TYPICAL: &str = "<c:printSettings><c:headerFooter/><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" \
     header=\"0.3\" footer=\"0.3\"/><c:pageSetup/></c:printSettings>";

/// Every part set: header/footer flags and texts, odd margins as Excel writes them, page setup attributes.
const FULL: &str = "<c:printSettings><c:headerFooter alignWithMargins=\"0\" differentOddEven=\"1\" differentFirst=\"1\">\
     <c:oddHeader>&amp;C&amp;\"Arial,Bold\"Sales &amp; costs</c:oddHeader><c:oddFooter>Page &amp;P of &amp;N</c:oddFooter>\
     <c:evenHeader>&amp;LEven</c:evenHeader><c:evenFooter>&amp;R&amp;D</c:evenFooter>\
     <c:firstHeader>First &lt;page&gt;</c:firstHeader><c:firstFooter>&amp;F</c:firstFooter></c:headerFooter>\
     <c:pageMargins b=\"1\" l=\"0.75000000000000033\" r=\"0.75000000000000033\" t=\"1\" header=\"0.5\" footer=\"0.5\"/>\
     <c:pageSetup paperSize=\"9\" firstPageNumber=\"3\" orientation=\"portrait\" blackAndWhite=\"1\" draft=\"0\" \
     useFirstPageNumber=\"1\" horizontalDpi=\"1\" verticalDpi=\"-3\" copies=\"2\"/></c:printSettings>";

fn read(xml: &str) -> Chart {
    read_chart(&parse(xml.as_bytes()).unwrap()).expect("column chart must be read")
}

fn save(ch: &Chart) -> String {
    let wb = Workbook { sheets: vec![Arc::new(Sheet::new("Sheet1"))], ..Workbook::default() };
    write_chart(&wb, 0, ch, "Book1")
}

/// Open and save, then open and save again: the second save must equal the first (stable).
fn round_trip(xml: &str) -> String {
    let once = save(&read(xml));
    let twice = save(&read(&once));
    assert_eq!(once, twice, "second save differs from the first");
    once
}

/// The text of the `<c:printSettings>...</c:printSettings>` element.
fn print_settings(xml: &str) -> &str {
    let s = xml.find("<c:printSettings>").unwrap_or_else(|| panic!("no c:printSettings in {xml}"));
    let e = xml[s..].find("</c:printSettings>").expect("unclosed c:printSettings") + "</c:printSettings>".len();
    &xml[s..s + e]
}

fn pos(hay: &str, needle: &str) -> usize {
    hay.find(needle).unwrap_or_else(|| panic!("no {needle} in {hay}"))
}

#[test]
fn typical_print_settings_are_written_after_the_chart_and_stay_stable() {
    let out = round_trip(&chart_xml(TYPICAL));
    let ps = print_settings(&out);
    assert!(pos(&out, "</c:chart>") < pos(&out, "<c:printSettings>"), "printSettings follows c:chart: {out}");
    assert!(out.trim_end().ends_with("</c:printSettings></c:chartSpace>"), "printSettings is last: {out}");
    assert!(pos(ps, "<c:headerFooter") < pos(ps, "<c:pageMargins") && pos(ps, "<c:pageMargins") < pos(ps, "<c:pageSetup"), "{ps}");
    for attr in ["l=\"0.7\"", "r=\"0.7\"", "t=\"0.75\"", "b=\"0.75\"", "header=\"0.3\"", "footer=\"0.3\""] {
        assert!(ps.contains(attr), "{attr} in {ps}");
    }
    assert!(!ps.contains("orientation="), "no attribute the file did not have: {ps}");
    assert!(!ps.contains("<c:oddHeader") && !ps.contains("<c:oddFooter"), "no header/footer texts the file did not have: {ps}");
}

#[test]
fn every_print_setting_is_written_back() {
    let out = round_trip(&chart_xml(FULL));
    let ps = print_settings(&out);
    for frag in [
        "alignWithMargins=\"0\"",
        "differentOddEven=\"1\"",
        "differentFirst=\"1\"",
        "<c:oddHeader>&amp;C&amp;\"Arial,Bold\"Sales &amp; costs</c:oddHeader>",
        "<c:oddFooter>Page &amp;P of &amp;N</c:oddFooter>",
        "<c:firstHeader>First &lt;page&gt;</c:firstHeader>",
        "l=\"0.75000000000000033\"",
        "paperSize=\"9\"",
        "firstPageNumber=\"3\"",
        "orientation=\"portrait\"",
        "blackAndWhite=\"1\"",
        "draft=\"0\"",
        "useFirstPageNumber=\"1\"",
        "horizontalDpi=\"1\"",
        "verticalDpi=\"-3\"",
        "copies=\"2\"",
    ] {
        // The header text may be written with `"` escaped as `&quot;`; both are the same XML text.
        let alt = frag.replace("\"Arial,Bold\"", "&quot;Arial,Bold&quot;");
        assert!(ps.contains(frag) || ps.contains(&alt), "{frag} in {ps}");
    }
    let order = ["<c:oddHeader>", "<c:oddFooter>", "<c:evenHeader>", "<c:evenFooter>", "<c:firstHeader>", "<c:firstFooter>"];
    for w in order.windows(2) {
        assert!(pos(ps, w[0]) < pos(ps, w[1]), "{} before {}: {ps}", w[0], w[1]);
    }
}

#[test]
fn a_chart_without_print_settings_gets_none_on_save() {
    assert!(!round_trip(&chart_xml("")).contains("printSettings"), "no c:printSettings is invented");
}

#[test]
fn damaged_values_are_dropped_not_fatal() {
    // A margin attribute missing: margins unknown. An orientation outside ST_PageSetupOrientation: dropped.
    let out = round_trip(&chart_xml(
        "<c:printSettings><c:pageMargins b=\"0.75\" l=\"0.7\" r=\"0.7\" t=\"0.75\" header=\"0.3\"/>\
         <c:pageSetup orientation=\"sideways\" paperSize=\"x\" copies=\"2\"/></c:printSettings>",
    ));
    let ps = print_settings(&out);
    assert!(!ps.contains("sideways") && !ps.contains("paperSize") && !ps.contains("pageMargins"), "{ps}");
    assert!(ps.contains("<c:pageSetup") && ps.contains("copies=\"2\""), "the valid attribute is kept: {ps}");
    assert!(!ps.contains("headerFooter"), "no c:headerFooter the file did not have: {ps}");
}
