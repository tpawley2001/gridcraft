//! The workbook's legacy colour palette (`colors/indexedColors`, ECMA-376 Part 1 §18.8.27) and its
//! Recent Colors (`colors/mruColors`, §18.8.28) survive open + save as Excel keeps them: a custom
//! palette as written (alpha byte kept, hex upper-case), a short one padded to 64 entries with the
//! built-in palette (alpha `00`), a palette equal to the built-in one dropped, and `mruColors` kept
//! in order. Expected values measured from Microsoft 365 Excel re-saves.

use super::{CT, ROOT_RELS, make_zip};
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use std::io::Read;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The built-in 64-entry palette (RGB), as Excel writes it for indexes a short palette leaves out.
const DEFAULT: [&str; 64] = [
    "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00", "FF00FF", "00FFFF", "000000", "FFFFFF", "FF0000", "00FF00", "0000FF", "FFFF00",
    "FF00FF", "00FFFF", "800000", "008000", "000080", "808000", "800080", "008080", "C0C0C0", "808080", "9999FF", "993366", "FFFFCC", "CCFFFF",
    "660066", "FF8080", "0066CC", "CCCCFF", "000080", "FF00FF", "FFFF00", "00FFFF", "800080", "800000", "008080", "0000FF", "00CCFF", "CCFFFF",
    "CCFFCC", "FFFF99", "99CCFF", "FF99CC", "CC99FF", "FFCC99", "3366FF", "33CCCC", "99CC00", "FFCC00", "FF9900", "FF6600", "666699", "969696",
    "003366", "339966", "003300", "333300", "993300", "993366", "333399", "333333",
];

/// A one-sheet package whose styles.xml ends with `colors` (may be empty: no `<colors>`), and
/// whose A1 uses a fill with `indexed="10"`.
fn book(colors: &str) -> Vec<u8> {
    let styles = format!(
        r#"<styleSheet xmlns="{MAIN}"><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor indexed="10"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="0" fillId="2" borderId="0" xfId="0" applyFill="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles><dxfs count="0"/><tableStyles count="0" defaultTableStyle="TableStyleMedium2" defaultPivotStyle="PivotStyleLight16"/>{colors}</styleSheet>"#
    );
    let sheet = format!(r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1" s="1"><v>1</v></c></row></sheetData></worksheet>"#);
    let workbook = format!(r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Data" sheetId="1" r:id="s1"/></sheets></workbook>"#);
    let rels = format!(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="st" Type="{REL}/styles" Target="styles.xml"/></Relationships>"#
    );
    make_zip(&[
        ("[Content_Types].xml", CT.as_bytes()),
        ("_rels/.rels", ROOT_RELS.as_bytes()),
        ("xl/workbook.xml", workbook.as_bytes()),
        ("xl/_rels/workbook.xml.rels", rels.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/styles.xml", styles.as_bytes()),
    ])
}

fn part(bytes: &[u8], name: &str) -> El {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut b = Vec::new();
    z.by_name(name).unwrap().read_to_end(&mut b).unwrap();
    parse(&b).unwrap()
}

/// The saved styles.xml after open + save.
fn saved_styles(colors: &str) -> El {
    let saved = write_xlsx(&read_xlsx(&book(colors)).unwrap().0).unwrap();
    part(&saved, "xl/styles.xml")
}

/// The `rgb` attributes of the saved `indexedColors`, or `None` when it wasn't written.
fn saved_palette(styles: &El) -> Option<Vec<String>> {
    let ix = styles.path(&["colors", "indexedColors"])?;
    Some(ix.kids("rgbColor").map(|c| c.attr("rgb").unwrap_or("").to_string()).collect())
}

fn saved_mru(styles: &El) -> Vec<String> {
    styles.path(&["colors", "mruColors"]).map(|m| m.kids("color").map(|c| c.attr("rgb").unwrap_or("").to_string()).collect()).unwrap_or_default()
}

fn palette_xml(entries: &[String]) -> String {
    let rows: String = entries.iter().map(|e| format!(r#"<rgbColor rgb="{e}"/>"#)).collect();
    format!("<indexedColors>{rows}</indexedColors>")
}

/// The built-in palette with alpha `aa` and entries `changes` (index, ARGB) replaced.
fn palette(aa: &str, changes: &[(usize, &str)]) -> Vec<String> {
    let mut p: Vec<String> = DEFAULT.iter().map(|c| format!("{aa}{c}")).collect();
    for (i, c) in changes {
        p[*i] = (*c).to_string();
    }
    p
}

#[test]
fn a_custom_palette_is_kept_entry_for_entry_with_its_alpha() {
    let custom = palette("00", &[(10, "00CC0000"), (17, "00FFC000")]);
    let styles = saved_styles(&format!("<colors>{}</colors>", palette_xml(&custom)));
    assert_eq!(saved_palette(&styles), Some(custom));
}

#[test]
fn hex_is_written_upper_case_and_ff_alpha_is_kept() {
    let lower: Vec<String> = palette("ff", &[(12, "ff1f497d")]);
    let styles = saved_styles(&format!("<colors>{}</colors>", palette_xml(&lower)));
    let upper: Vec<String> = lower.iter().map(|c| c.to_uppercase()).collect();
    assert_eq!(saved_palette(&styles), Some(upper));
}

#[test]
fn a_short_palette_is_padded_to_64_with_the_built_in_colours() {
    // 15 entries given (as in a corpus file); Excel writes 64, the rest from the built-in palette with alpha 00.
    let given: Vec<String> = palette("FF", &[(10, "FFC00000")]).into_iter().take(15).collect();
    let styles = saved_styles(&format!("<colors>{}</colors>", palette_xml(&given)));
    let got = saved_palette(&styles).expect("a custom palette is written");
    assert_eq!(got.len(), 64);
    assert_eq!(&got[..15], &given[..]);
    assert_eq!(got[15], "0000FFFF");
    assert_eq!(got[56], "00003366");
    assert_eq!(got[63], "00333333");
}

#[test]
fn a_palette_equal_to_the_built_in_one_is_dropped() {
    for aa in ["00", "FF"] {
        let styles = saved_styles(&format!("<colors>{}</colors>", palette_xml(&palette(aa, &[]))));
        assert_eq!(saved_palette(&styles), None, "alpha {aa}");
    }
}

#[test]
fn recent_colours_are_kept_in_order_without_a_palette() {
    let mru =
        r#"<colors><mruColors><color rgb="FF99CCFF"/><color rgb="FFC0C0C0"/><color rgb="FF666699"/><color rgb="FFFFFF99"/></mruColors></colors>"#;
    let styles = saved_styles(mru);
    assert_eq!(saved_mru(&styles), vec!["FF99CCFF", "FFC0C0C0", "FF666699", "FFFFFF99"]);
    assert_eq!(saved_palette(&styles), None);
}

#[test]
fn palette_then_recent_colours_after_table_styles() {
    let custom = palette("00", &[(10, "00CC0000")]);
    let colors = format!(r#"<colors>{}<mruColors><color rgb="FFE1D719"/><color rgb="FFFFFF99"/></mruColors></colors>"#, palette_xml(&custom));
    let styles = saved_styles(&colors);
    assert_eq!(saved_palette(&styles), Some(custom));
    assert_eq!(saved_mru(&styles), vec!["FFE1D719", "FFFFFF99"]);
    let order: Vec<&str> = styles.children.iter().map(|c| c.name.as_str()).collect();
    let at = |n: &str| order.iter().position(|x| *x == n);
    let colors_at = at("colors").expect("colors written");
    if let Some(t) = at("tableStyles") {
        assert!(t < colors_at, "{order:?}");
    }
    if let Some(d) = at("dxfs") {
        assert!(d < colors_at, "{order:?}");
    }
    if let Some(e) = at("extLst") {
        assert!(colors_at < e, "{order:?}");
    }
    let inner: Vec<&str> = styles.child("colors").unwrap().children.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(inner, vec!["indexedColors", "mruColors"]);
}

/// Guard: without `<colors>` nothing is added, and the custom palette still colours the cell's fill.
#[test]
fn no_colors_stays_no_colors_and_indexed_fills_still_resolve() {
    let styles = saved_styles("");
    assert!(styles.child("colors").is_none());
    let custom = palette("00", &[(10, "00CC0000")]);
    let (wb, _) = read_xlsx(&book(&format!("<colors>{}</colors>", palette_xml(&custom)))).unwrap();
    let saved = write_xlsx(&wb).unwrap();
    let text = {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(&saved[..])).unwrap();
        let mut s = String::new();
        z.by_name("xl/styles.xml").unwrap().read_to_string(&mut s).unwrap();
        s
    };
    assert!(text.to_uppercase().contains("CC0000"), "the fill keeps the custom palette's colour");
}
