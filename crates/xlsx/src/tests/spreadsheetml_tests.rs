//! XML Spreadsheet 2003 import: values, R1C1 formulas, dates, styles, layout, names and hostile input.
use crate::{is_spreadsheetml, read_spreadsheetml};
use gridcraft_core::{CellError, CellRef, DateSystem, RangeRef, Value};
use gridcraft_model::{BorderStyle, Color, PatternType, Visibility, Workbook};

const HEAD: &str = r##"<?xml version="1.0"?>
<?mso-application progid="Excel.Sheet"?>
<Workbook xmlns="urn:schemas-microsoft-com:office:spreadsheet" xmlns:o="urn:schemas-microsoft-com:office:office"
 xmlns:x="urn:schemas-microsoft-com:office:excel" xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet"
 xmlns:html="http://www.w3.org/TR/REC-html40">
"##;

fn book(body: &str) -> String {
    format!("{HEAD}{body}</Workbook>")
}

fn read(body: &str) -> (Workbook, Vec<String>) {
    let (wb, report) = read_spreadsheetml(book(body).as_bytes()).unwrap();
    (wb, report.warnings)
}

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).unwrap()
}

fn value(wb: &Workbook, sheet: usize, a1: &str) -> Value {
    wb.sheets[sheet].cell(at(a1)).map(|c| c.value.clone()).unwrap_or(Value::Empty)
}

fn formula(wb: &Workbook, a1: &str) -> Option<String> {
    wb.sheets[0].cell(at(a1)).and_then(|c| c.formula.as_ref()).map(|f| f.text.clone())
}

#[test]
fn values_indexes_and_merges() {
    let (wb, warnings) = read(
        r##"<Worksheet ss:Name="Data"><Table>
  <Row><Cell><Data ss:Type="Number">1.5</Data></Cell><Cell><Data ss:Type="String">text</Data></Cell>
   <Cell ss:Index="5"><Data ss:Type="Boolean">1</Data></Cell><Cell><Data ss:Type="Error">#DIV/0!</Data></Cell></Row>
  <Row ss:Index="4"><Cell ss:MergeAcross="2"><Data ss:Type="String">wide</Data></Cell><Cell><Data ss:Type="Number">7</Data></Cell></Row>
  <Row><Cell ss:MergeDown="1"><Data ss:Type="String">tall</Data></Cell><Cell><Data ss:Type="Number">abc</Data></Cell>
   <Cell><Data ss:Type="Error">#OOPS</Data></Cell><Cell><Data ss:Type="Boolean">0</Data></Cell></Row>
 </Table></Worksheet>
 <Worksheet ss:Name="Second"><Table><Row ss:Index="2"><Cell ss:Index="3"><ss:Data ss:Type="String"
  xmlns="http://www.w3.org/TR/REC-html40"><B>Bold</B><Font> plain</Font></ss:Data></Cell></Row></Table></Worksheet>"##,
    );
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(wb.sheets.len(), 2);
    assert_eq!(wb.sheets[1].name, "Second");
    assert_eq!(value(&wb, 0, "A1"), Value::Number(1.5));
    assert_eq!(value(&wb, 0, "B1"), Value::Text("text".into()));
    assert_eq!(value(&wb, 0, "E1"), Value::Bool(true));
    assert_eq!(value(&wb, 0, "F1"), Value::Error(CellError::Div0));
    assert_eq!(value(&wb, 0, "A4"), Value::Text("wide".into()));
    // The index-less cell after a three-column merge lands after the merge.
    assert_eq!(value(&wb, 0, "D4"), Value::Number(7.0));
    assert_eq!(value(&wb, 0, "A5"), Value::Text("tall".into()));
    assert_eq!(value(&wb, 0, "B5"), Value::Text("abc".into()));
    assert_eq!(value(&wb, 0, "C5"), Value::Text("#OOPS".into()));
    assert_eq!(value(&wb, 0, "D5"), Value::Bool(false));
    assert_eq!(wb.sheets[0].merges, vec![RangeRef::parse("A4:C4").unwrap(), RangeRef::parse("A5:A6").unwrap()]);
    assert_eq!(value(&wb, 1, "C2"), Value::Text("Bold plain".into()));
}

#[test]
fn backward_indexes_are_skipped_with_one_warning() {
    let (wb, warnings) = read(
        r##"<Worksheet ss:Name="S"><Table>
  <Row ss:Index="3"><Cell ss:Index="2"><Data ss:Type="Number">1</Data></Cell><Cell ss:Index="2"><Data ss:Type="Number">2</Data></Cell></Row>
  <Row ss:Index="2"><Cell><Data ss:Type="Number">3</Data></Cell></Row>
  <Row ss:Index="1048577"><Cell><Data ss:Type="Number">4</Data></Cell></Row>
 </Table></Worksheet>"##,
    );
    assert_eq!(value(&wb, 0, "B3"), Value::Number(1.0));
    assert_eq!(wb.sheets[0].cells.iter().count(), 1);
    assert_eq!(warnings.iter().filter(|w| w.contains("ss:Index")).count(), 1);
    assert_eq!(warnings.iter().filter(|w| w.contains("beyond")).count(), 1);
}

#[test]
fn r1c1_formulas_convert_or_keep_their_values() {
    let (wb, warnings) = read(
        r##"<Worksheet ss:Name="F"><Table>
  <Row><Cell><Data ss:Type="Number">21</Data></Cell><Cell ss:Formula="=RC[-1]*2"><Data ss:Type="Number">42</Data></Cell>
   <Cell ss:Formula="=SUM(R1C1:R2C1)"><Data ss:Type="Number">21</Data></Cell>
   <Cell ss:Formula="=RC[-9]+1"><Data ss:Type="Number">5</Data></Cell>
   <Cell ss:Formula="=SUM(("><Data ss:Type="Number">6</Data></Cell></Row>
 </Table></Worksheet>"##,
    );
    assert_eq!(formula(&wb, "B1").as_deref(), Some("A1*2"));
    assert_eq!(value(&wb, 0, "B1"), Value::Number(42.0));
    assert_eq!(formula(&wb, "C1").as_deref(), Some("SUM($A$1:$A$2)"));
    assert_eq!(formula(&wb, "D1"), None);
    assert_eq!(value(&wb, 0, "D1"), Value::Number(5.0));
    assert_eq!(formula(&wb, "E1"), None);
    assert_eq!(value(&wb, 0, "E1"), Value::Number(6.0));
    assert!(warnings.contains(&"2 formula(s) couldn't be read and were kept as their saved values.".to_string()), "{warnings:?}");
}

const DATES: &str = r##"<Worksheet ss:Name="D"><Table>
  <Row><Cell><Data ss:Type="DateTime">2026-01-15T00:00:00.000</Data></Cell></Row>
  <Row><Cell><Data ss:Type="DateTime">2026-01-15T18:00:00.000</Data></Cell></Row>
  <Row><Cell><Data ss:Type="DateTime">1900-03-01T00:00:00.000</Data></Cell></Row>
  <Row><Cell><Data ss:Type="DateTime">1899-12-31T12:00:00.000</Data></Cell></Row>
  <Row><Cell><Data ss:Type="DateTime">yesterday</Data></Cell></Row>
 </Table></Worksheet>"##;

#[test]
fn datetimes_in_both_date_systems() {
    let (wb, _) = read(DATES);
    assert_eq!(wb.date_system, DateSystem::D1900);
    assert_eq!(value(&wb, 0, "A1"), Value::Number(46037.0));
    assert_eq!(value(&wb, 0, "A2"), Value::Number(46037.75));
    assert_eq!(value(&wb, 0, "A3"), Value::Number(61.0));
    // Excel writes a time of day without a date on 1899-12-31: serial 0.5 is 12:00.
    assert_eq!(value(&wb, 0, "A4"), Value::Number(0.5));
    assert_eq!(value(&wb, 0, "A5"), Value::Text("yesterday".into()));
    // No number-format style: the serial shows as a number.
    assert_eq!(wb.sheets[0].cell(at("A1")).unwrap().style, gridcraft_model::StyleId::DEFAULT);

    let (wb, _) = read(&format!("<x:ExcelWorkbook><x:Date1904/></x:ExcelWorkbook>{DATES}"));
    assert_eq!(wb.date_system, DateSystem::D1904);
    assert_eq!(value(&wb, 0, "A1"), Value::Number(44575.0));
    assert_eq!(value(&wb, 0, "A2"), Value::Number(44575.75));
}

#[test]
fn styles_map_to_cell_formats() {
    let (wb, _) = read(
        r##"<Styles>
  <Style ss:ID="Default" ss:Name="Normal"><Font ss:FontName="Arial" ss:Size="10"/></Style>
  <Style ss:ID="s1"><Font ss:Bold="1" ss:Color="#FF0000" ss:Underline="Single"/><Interior ss:Color="#FFFF00" ss:Pattern="Solid"/>
   <Borders><Border ss:Position="Bottom" ss:LineStyle="Continuous" ss:Weight="2" ss:Color="#0000FF"/>
    <Border ss:Position="Top" ss:LineStyle="Dot"/></Borders>
   <Alignment ss:Horizontal="Center" ss:Vertical="Top" ss:WrapText="1" ss:Rotate="45"/><Unknown/></Style>
  <Style ss:ID="s2" ss:Parent="s1"><NumberFormat ss:Format="Percent"/><Protection ss:Protected="0" ss:HideFormula="1"/></Style>
  <Style ss:ID="s3"><NumberFormat ss:Format="0.000"/><Interior ss:Color="#00FF00" ss:Pattern="Gray50"/></Style>
 </Styles>
 <Worksheet ss:Name="S"><Table><Row>
  <Cell><Data ss:Type="Number">1</Data></Cell><Cell ss:StyleID="s1"><Data ss:Type="Number">2</Data></Cell>
  <Cell ss:StyleID="s2"><Data ss:Type="Number">0.5</Data></Cell><Cell ss:StyleID="s3"/><Cell ss:StyleID="nope"><Data ss:Type="Number">3</Data></Cell>
 </Row></Table></Worksheet>"##,
    );
    let style = |a1: &str| wb.styles.get(wb.sheets[0].style_id(at(a1))).clone();
    let normal = style("A1");
    assert_eq!(normal.font.name, "Arial");
    assert_eq!(normal.font.size, 10.0);
    assert_eq!(style("E1"), normal);
    let s1 = style("B1");
    assert!(s1.font.bold);
    assert_eq!(s1.font.name, "Arial");
    assert_eq!(s1.font.color, Color::Rgb(0xFF0000));
    assert_eq!(s1.font.underline, gridcraft_model::Underline::Single);
    assert_eq!(s1.fill.pattern, PatternType::Solid);
    assert_eq!(s1.fill.fg, Color::Rgb(0xFFFF00));
    assert_eq!(s1.borders.bottom.style, BorderStyle::Medium);
    assert_eq!(s1.borders.bottom.color, Color::Rgb(0x0000FF));
    assert_eq!(s1.borders.top.style, BorderStyle::Dotted);
    assert_eq!(s1.align.h, gridcraft_model::HAlign::Center);
    assert_eq!(s1.align.v, gridcraft_model::VAlign::Top);
    assert!(s1.align.wrap);
    assert_eq!(s1.align.rotation, 45);
    let s2 = style("C1");
    assert_eq!(s2.num_fmt.as_str(), "0.00%");
    assert!(s2.font.bold, "inherits from its parent");
    assert_eq!(s2.borders.bottom.style, BorderStyle::Medium);
    assert!(!s2.protection.locked);
    assert!(s2.protection.hidden);
    let s3 = style("D1");
    assert_eq!(s3.num_fmt.as_str(), "0.000");
    assert_eq!(s3.fill.pattern, PatternType::Solid);
    assert_eq!(s3.fill.fg, Color::Rgb(0x00FF00));
}

#[test]
fn layout_view_names_notes_and_links() {
    let (wb, warnings) = read(
        r##"<Names><NamedRange ss:Name="Sales" ss:RefersTo="=Sheet1!R1C1:R5C2"/>
  <NamedRange ss:Name="_FilterDatabase" ss:RefersTo="=Sheet1!R1C1:R5C2" ss:Hidden="1"/>
  <NamedRange ss:Name="Broken" ss:RefersTo="=SUM(("/></Names>
 <Worksheet ss:Name="Sheet1"><Names><NamedRange ss:Name="Local" ss:RefersTo="=Sheet1!R2C3"/></Names>
  <Table ss:DefaultColumnWidth="60" ss:DefaultRowHeight="15">
  <Column ss:Width="30"/><Column ss:Index="3" ss:Span="2" ss:Width="75" ss:Hidden="1"/><Column ss:Width="15"/>
  <Row ss:Height="30"><Cell ss:HRef="https://example.com/" ss:HRefScreenTip="tip"><Data ss:Type="String">link</Data>
   <Comment ss:Author="Ann"><ss:Data xmlns="http://www.w3.org/TR/REC-html40"><B>Note</B> text</ss:Data></Comment></Cell>
   <Cell ss:HRef="#Sheet1!A1"><Data ss:Type="String">here</Data></Cell></Row>
  <Row ss:Hidden="1"/>
 </Table>
 <x:WorksheetOptions><x:FreezePanes/><x:FrozenNoSplit/><x:SplitHorizontal>1</x:SplitHorizontal><x:SplitVertical>2</x:SplitVertical></x:WorksheetOptions>
 </Worksheet>
 <Worksheet ss:Name="Hidden"><x:WorksheetOptions><x:Visible>SheetHidden</x:Visible><x:Selected/></x:WorksheetOptions></Worksheet>
 <Worksheet ss:Name="Secret"><x:WorksheetOptions><x:Visible>SheetVeryHidden</x:Visible></x:WorksheetOptions></Worksheet>"##,
    );
    let s = &wb.sheets[0];
    assert_eq!(s.default_col_width, 80.0);
    assert_eq!(s.default_row_height, 20.0);
    assert_eq!(s.cols[&0].size, Some(40.0));
    for c in 2..=4 {
        assert_eq!(s.cols[&c].size, Some(100.0));
        assert!(s.cols[&c].hidden);
    }
    assert_eq!(s.cols[&5].size, Some(20.0));
    assert!(!s.cols.contains_key(&1));
    assert_eq!(s.rows[&0].size, Some(40.0));
    assert!(s.rows[&1].hidden);
    assert_eq!(s.freeze, Some((1, 2)));
    let link = &s.hyperlinks[&at("A1")];
    assert_eq!((link.target.as_str(), link.tooltip.as_deref(), link.external), ("https://example.com/", Some("tip"), true));
    let local = &s.hyperlinks[&at("B1")];
    assert_eq!((local.target.as_str(), local.external), ("Sheet1!A1", false));
    let note = &s.comments[&at("A1")];
    assert_eq!((note.author.as_str(), note.text.as_str()), ("Ann", "Note text"));
    assert_eq!(wb.sheets[1].visibility, Visibility::Hidden);
    assert_eq!(wb.sheets[2].visibility, Visibility::VeryHidden);
    assert_eq!(wb.active_sheet.get(), 1);
    let names: Vec<_> = wb.names.iter().map(|n| (n.name.as_str(), n.scope, n.formula.as_str())).collect();
    assert_eq!(names, vec![("Sales", None, "Sheet1!$A$1:$B$5"), ("Local", Some(0), "Sheet1!$C$2")]);
    assert_eq!(warnings, vec!["Some defined names couldn't be read and were skipped.".to_string()]);
}

#[test]
fn sheet_names_are_sanitised() {
    let long = "A very long sheet name that goes past the limit";
    let (wb, warnings) = read(&format!(r##"<Worksheet ss:Name="{long}"/><Worksheet ss:Name="{long}"/><Worksheet ss:Name="a/b"/>"##));
    assert!(wb.sheets.iter().all(|s| s.name.chars().count() <= 31));
    assert_ne!(wb.sheets[0].name, wb.sheets[1].name);
    assert_eq!(wb.sheets[2].name, "ab");
    assert_eq!(warnings.len(), 1);
}

#[test]
fn prefixed_and_default_namespaces_read_the_same() {
    let plain = book(r##"<Worksheet ss:Name="S"><Table><Row><Cell ss:StyleID="b"><Data ss:Type="Number">1</Data></Cell></Row></Table></Worksheet>"##);
    let prefixed = r##"<?xml version="1.0" encoding="UTF-8"?>
<!-- exported -->
<ss:Workbook xmlns:ss="urn:schemas-microsoft-com:office:spreadsheet">
 <ss:Worksheet ss:Name="S"><ss:Table><ss:Row><ss:Cell ss:StyleID="b"><ss:Data ss:Type="Number">1</ss:Data></ss:Cell></ss:Row></ss:Table></ss:Worksheet>
</ss:Workbook>"##;
    assert!(is_spreadsheetml(plain.as_bytes()));
    assert!(is_spreadsheetml(prefixed.as_bytes()));
    assert!(is_spreadsheetml(format!("\u{FEFF}  {plain}").as_bytes()));
    let a = read_spreadsheetml(plain.as_bytes()).unwrap().0;
    let b = read_spreadsheetml(prefixed.as_bytes()).unwrap().0;
    assert_eq!(a.sheets[0].cells, b.sheets[0].cells);
    assert_eq!(a.sheets[0].name, b.sheets[0].name);

    for other in [
        r##"<root xmlns="urn:x"/>"##,
        r##"<?xml version="1.0"?><Workbook/>"##,
        r##"<Workbook xmlns="urn:schemas-microsoft-com:office:excel"/>"##,
        r##"<x:Workbook xmlns="urn:schemas-microsoft-com:office:spreadsheet" xmlns:x="urn:other"/>"##,
        "Name,Amount\n",
        "",
    ] {
        assert!(!is_spreadsheetml(other.as_bytes()), "{other}");
        assert!(read_spreadsheetml(other.as_bytes()).is_err());
    }
}

#[test]
fn hostile_input_never_panics() {
    let valid = book(&format!(
        r##"<Styles><Style ss:ID="a" ss:Parent="b"><Font ss:Size="1e400"/></Style><Style ss:ID="b" ss:Parent="a"/></Styles>
 <Names><NamedRange ss:Name="N" ss:RefersTo="=R99999999C1"/></Names>
 <Worksheet ss:Name="S"><Table ss:DefaultColumnWidth="-5">
  <Column ss:Index="16384" ss:Span="4294967295" ss:Width="1e300"/><Row ss:Span="4294967295" ss:Height="NaN"/>
  <Row ss:Index="18446744073709551616"><Cell ss:StyleID="a" ss:MergeAcross="4294967295" ss:MergeDown="4294967295"
   ss:Formula="=R[-1]C[-1]"><Data ss:Type="DateTime">9999-12-31T23:59:59.999</Data></Cell></Row>
 </Table>{DATES}</Worksheet>"##
    ));
    let (wb, _) = read_spreadsheetml(valid.as_bytes()).unwrap();
    assert!(wb.sheets[0].cols.len() <= 1);
    let bytes = valid.as_bytes();
    for end in (0..bytes.len()).step_by(7) {
        let _ = read_spreadsheetml(&bytes[..end]);
    }
    let mut seed = 0x2545F4914F6CDD1Du64;
    for _ in 0..200 {
        let mut copy = bytes.to_vec();
        for _ in 0..8 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let i = (seed as usize) % copy.len();
            copy[i] = (seed >> 32) as u8;
        }
        let _ = read_spreadsheetml(&copy);
    }
}
