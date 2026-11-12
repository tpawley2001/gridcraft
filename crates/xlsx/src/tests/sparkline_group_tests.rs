//! Sparkline groups keep their settings through open + save: the point highlights (high, low,
//! first, last, negative, markers) with their colours, the axis and line settings, the date-axis
//! range and the grouping itself ([MS-XLSX] `CT_SparklineGroup`). The input spellings follow what
//! Microsoft 365 Excel writes for a group.

use crate::package::Package;
use crate::tests::minimal;
use crate::{read_xlsx, write_xlsx};

const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const XM: &str = "http://schemas.microsoft.com/office/excel/2006/main";

/// A one-sheet package whose worksheet carries `groups` (the `<x14:sparklineGroup>` elements).
fn package(groups: &str) -> Vec<u8> {
    let body = format!(
        r#"<sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1"><v>-2</v></c><c r="C1"><v>3</v></c><c r="D1"><v>4</v></c></row><row r="2"><c r="A2"><v>5</v></c><c r="B2"><v>6</v></c><c r="C2"><v>-7</v></c><c r="D2"><v>8</v></c></row></sheetData><extLst><ext uri="{{05C60535-1F16-4fd2-B633-F4F36F0B64E0}}" xmlns:x14="{X14}"><x14:sparklineGroups xmlns:xm="{XM}">{groups}</x14:sparklineGroups></ext></extLst>"#
    );
    minimal(&body, &[], "", "")
}

fn sparkline(source: &str, cell: &str) -> String {
    format!("<x14:sparkline><xm:f>{source}</xm:f><xm:sqref>{cell}</xm:sqref></x14:sparkline>")
}

/// Opens `bytes`, saves the workbook and returns the saved worksheet XML.
fn saved_sheet(bytes: &[u8]) -> String {
    let (wb, _) = read_xlsx(bytes).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    // What was saved reads back as the same sparklines.
    let (again, _) = read_xlsx(&saved).unwrap();
    assert_eq!(again.sheets[0].sparklines, wb.sheets[0].sparklines, "sparklines differ after save + reopen");
    let mut pkg = Package::open(&saved).unwrap();
    String::from_utf8(pkg.read("xl/worksheets/sheet1.xml").unwrap().unwrap()).unwrap()
}

/// Every saved `<x14:sparklineGroup>`: its start tag and its content up to the end tag.
fn groups(xml: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<x14:sparklineGroup") {
        let tail = &rest[at + "<x14:sparklineGroup".len()..];
        if tail.starts_with('s') {
            // `<x14:sparklineGroups>`, the container.
            rest = tail;
            continue;
        }
        let tag_end = tail.find('>').expect("group start tag closes");
        let end = tail.find("</x14:sparklineGroup>").expect("group end tag");
        out.push((tail[..tag_end].to_string(), tail[tag_end + 1..end].to_string()));
        rest = &tail[end..];
    }
    out
}

/// The value of attribute `name` in a start tag's attribute text.
fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    let len = tag[at..].find('"')?;
    Some(tag[at..at + len].to_string())
}

fn is_on(tag: &str, name: &str) -> bool {
    matches!(attr(tag, name).as_deref(), Some("1" | "true"))
}

#[test]
fn sparkline_group_highlight_flags_and_colours_survive_save() {
    let group = format!(
        r#"<x14:sparklineGroup type="column" displayEmptyCellsAs="gap" high="1" low="1" first="1" last="1" negative="1"><x14:colorSeries rgb="FF00B050"/><x14:colorNegative theme="5"/><x14:colorAxis rgb="FF000000"/><x14:colorMarkers rgb="FF0070C0"/><x14:colorFirst rgb="FFFFC000"/><x14:colorLast rgb="FFFFC001"/><x14:colorHigh rgb="FF00B051"/><x14:colorLow rgb="FFFF0000"/><x14:sparklines>{}{}</x14:sparklines></x14:sparklineGroup>"#,
        sparkline("Data!A1:D1", "F1"),
        sparkline("Data!A2:D2", "F2")
    );
    let xml = saved_sheet(&package(&group));
    let groups = groups(&xml);
    assert_eq!(groups.len(), 1, "two sparklines of one group stay one group: {xml}");
    let (tag, body) = &groups[0];
    assert_eq!(attr(tag, "type").as_deref(), Some("column"), "{tag}");
    assert_eq!(attr(tag, "displayEmptyCellsAs").as_deref(), Some("gap"), "{tag}");
    for flag in ["high", "low", "first", "last", "negative"] {
        assert!(is_on(tag, flag), "{flag} is kept: {tag}");
    }
    assert!(!is_on(tag, "markers"), "markers was off: {tag}");
    // Each colour as written, in schema order, before the sparklines.
    let elements = [
        r#"<x14:colorSeries rgb="FF00B050"/>"#,
        r#"<x14:colorNegative theme="5"/>"#,
        r#"<x14:colorAxis rgb="FF000000"/>"#,
        r#"<x14:colorMarkers rgb="FF0070C0"/>"#,
        r#"<x14:colorFirst rgb="FFFFC000"/>"#,
        r#"<x14:colorLast rgb="FFFFC001"/>"#,
        r#"<x14:colorHigh rgb="FF00B051"/>"#,
        r#"<x14:colorLow rgb="FFFF0000"/>"#,
        "<x14:sparklines>",
    ];
    let mut from = 0;
    for element in elements {
        let at = body[from..].find(element).unwrap_or_else(|| panic!("{element} missing or out of order in {body}"));
        from += at + element.len();
    }
    // Both sparklines, in their order.
    let first = body.find("<xm:sqref>F1</xm:sqref>").expect("F1 kept");
    let second = body.find("<xm:sqref>F2</xm:sqref>").expect("F2 kept");
    assert!(first < second, "{body}");
    assert!(body.contains("<xm:f>Data!A1:D1</xm:f>") && body.contains("<xm:f>Data!A2:D2</xm:f>"), "{body}");
}

#[test]
fn sparkline_group_axis_and_line_settings_survive_save() {
    let group = format!(
        r#"<x14:sparklineGroup manualMax="10.5" manualMin="-2" lineWeight="2.25" displayEmptyCellsAs="span" markers="1" displayXAxis="1" displayHidden="1" minAxisType="custom" maxAxisType="custom" rightToLeft="1"><x14:colorSeries rgb="FF376092"/><x14:sparklines>{}</x14:sparklines></x14:sparklineGroup>"#,
        sparkline("Data!A1:D1", "F1")
    );
    let xml = saved_sheet(&package(&group));
    let groups = groups(&xml);
    assert_eq!(groups.len(), 1, "{xml}");
    let tag = &groups[0].0;
    let number = |name: &str| attr(tag, name).and_then(|v| v.parse::<f64>().ok());
    assert_eq!(number("manualMax"), Some(10.5), "{tag}");
    assert_eq!(number("manualMin"), Some(-2.0), "{tag}");
    assert_eq!(number("lineWeight"), Some(2.25), "{tag}");
    assert_eq!(attr(tag, "displayEmptyCellsAs").as_deref(), Some("span"), "{tag}");
    assert_eq!(attr(tag, "minAxisType").as_deref(), Some("custom"), "{tag}");
    assert_eq!(attr(tag, "maxAxisType").as_deref(), Some("custom"), "{tag}");
    for flag in ["markers", "displayXAxis", "displayHidden", "rightToLeft"] {
        assert!(is_on(tag, flag), "{flag} is kept: {tag}");
    }
    // A line group: no type attribute, or the default spelled out.
    assert!(matches!(attr(tag, "type").as_deref(), None | Some("line")), "{tag}");
}

#[test]
fn sparkline_group_shared_axis_groups_stay_separate_groups() {
    let shared = format!(
        r#"<x14:sparklineGroup displayEmptyCellsAs="gap" minAxisType="group" maxAxisType="group"><x14:colorSeries rgb="FF376092"/><x14:sparklines>{}{}</x14:sparklines></x14:sparklineGroup>"#,
        sparkline("Data!A1:D1", "F1"),
        sparkline("Data!A2:D2", "F2")
    );
    // Same settings as the group above except the axis types: still its own group.
    let own = format!(
        r#"<x14:sparklineGroup displayEmptyCellsAs="gap"><x14:colorSeries rgb="FF376092"/><x14:sparklines>{}</x14:sparklines></x14:sparklineGroup>"#,
        sparkline("Data!A1:D1", "G1")
    );
    let xml = saved_sheet(&package(&format!("{shared}{own}")));
    let groups = groups(&xml);
    assert_eq!(groups.len(), 2, "{xml}");
    let (tag, body) = &groups[0];
    assert_eq!(attr(tag, "minAxisType").as_deref(), Some("group"), "{tag}");
    assert_eq!(attr(tag, "maxAxisType").as_deref(), Some("group"), "{tag}");
    assert_eq!(body.matches("<x14:sparkline>").count(), 2, "{body}");
    let (tag, body) = &groups[1];
    assert!(matches!(attr(tag, "minAxisType").as_deref(), None | Some("individual")), "{tag}");
    assert!(matches!(attr(tag, "maxAxisType").as_deref(), None | Some("individual")), "{tag}");
    assert_eq!(body.matches("<x14:sparkline>").count(), 1, "{body}");
    assert!(body.contains("<xm:sqref>G1</xm:sqref>"), "{body}");
}

#[test]
fn sparkline_group_defaults_are_not_replaced_on_save() {
    // No displayEmptyCellsAs: the format's default is `zero` (not the `gap` of a new sparkline).
    let group = format!(
        r#"<x14:sparklineGroup type="stacked"><x14:colorSeries rgb="FF376092"/><x14:sparklines>{}</x14:sparklines></x14:sparklineGroup>"#,
        sparkline("Data!A1:D1", "F1")
    );
    let xml = saved_sheet(&package(&group));
    let groups = groups(&xml);
    assert_eq!(groups.len(), 1, "{xml}");
    let (tag, body) = &groups[0];
    assert_eq!(attr(tag, "type").as_deref(), Some("stacked"), "{tag}");
    assert!(matches!(attr(tag, "displayEmptyCellsAs").as_deref(), None | Some("zero")), "{tag}");
    for flag in ["markers", "high", "low", "first", "last", "negative", "displayXAxis", "displayHidden", "rightToLeft", "dateAxis"] {
        assert!(!is_on(tag, flag), "{flag} was off: {tag}");
    }
    assert!(attr(tag, "manualMin").is_none() && attr(tag, "manualMax").is_none(), "{tag}");
    // Colours that were absent stay absent.
    for element in ["colorNegative", "colorAxis", "colorMarkers", "colorFirst", "colorLast", "colorHigh", "colorLow"] {
        assert!(!body.contains(&format!("<x14:{element}")), "{element} was not in the file: {body}");
    }
}

#[test]
fn sparkline_group_date_axis_range_survives_save() {
    let group = format!(
        r#"<x14:sparklineGroup displayEmptyCellsAs="gap" dateAxis="1"><x14:colorSeries rgb="FF376092"/><xm:f>Data!A2:D2</xm:f><x14:sparklines>{}</x14:sparklines></x14:sparklineGroup>"#,
        sparkline("Data!A1:D1", "F1")
    );
    let xml = saved_sheet(&package(&group));
    let groups = groups(&xml);
    assert_eq!(groups.len(), 1, "{xml}");
    let (tag, body) = &groups[0];
    assert!(is_on(tag, "dateAxis"), "{tag}");
    let range = body.find("<xm:f>Data!A2:D2</xm:f>").expect("the date-axis range is kept");
    let list = body.find("<x14:sparklines>").expect("sparklines");
    assert!(range < list, "the group's date range comes before its sparklines: {body}");
    assert!(body[list..].contains("<xm:f>Data!A1:D1</xm:f>"), "{body}");
}
