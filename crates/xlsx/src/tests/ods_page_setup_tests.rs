//! ODS page styles -> sheet print settings: each table's master page gives its header, footer,
//! orientation, margins, scale, centering and page order, as Excel applies them when it opens
//! an `.ods`. Packages assembled in Rust, not files from another spreadsheet application.

use gridcraft_model::{Orientation, PageOrder, PrintSettings, Workbook};

use super::make_zip;
use crate::read_ods;

const MIME: &[u8] = b"application/vnd.oasis.opendocument.spreadsheet";
const MANIFEST: &[u8] = br#"<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/><manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/></manifest:manifest>"#;
const NS: &str = r#"xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0""#;

/// `tables`: `(sheet name, master page name or None for no table style)`.
fn content(tables: &[(&str, Option<&str>)]) -> String {
    let styles: String = tables
        .iter()
        .enumerate()
        .filter_map(|(i, (_, mp))| mp.map(|mp| format!(r#"<style:style style:name="ta{i}" style:family="table" style:master-page-name="{mp}"><style:table-properties table:display="true"/></style:style>"#)))
        .collect();
    let body: String = tables
        .iter()
        .enumerate()
        .map(|(i, (name, mp))| {
            let style = if mp.is_some() { format!(r#" table:style-name="ta{i}""#) } else { String::new() };
            format!(r#"<table:table table:name="{name}"{style}><table:table-row><table:table-cell office:value-type="float" office:value="1"/></table:table-row></table:table>"#)
        })
        .collect();
    format!(
        r#"<?xml version="1.0"?><office:document-content {NS}><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:spreadsheet>{body}</office:spreadsheet></office:body></office:document-content>"#
    )
}

fn styles(layouts: &str, masters: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><office:document-styles {NS}><office:automatic-styles>{layouts}</office:automatic-styles><office:master-styles>{masters}</office:master-styles></office:document-styles>"#
    )
}

/// A page layout with the given page properties and header/footer heights and spacing.
fn layout(name: &str, props: &str, header: &str, footer: &str) -> String {
    format!(
        r#"<style:page-layout style:name="{name}"><style:page-layout-properties {props}/><style:header-style><style:header-footer-properties {header}/></style:header-style><style:footer-style><style:header-footer-properties {footer}/></style:footer-style></style:page-layout>"#
    )
}

fn read(content: &str, styles: Option<&str>) -> Workbook {
    let mut parts: Vec<(&str, &[u8])> = vec![("mimetype", MIME), ("META-INF/manifest.xml", MANIFEST), ("content.xml", content.as_bytes())];
    if let Some(s) = styles {
        parts.push(("styles.xml", s.as_bytes()));
    }
    read_ods(&make_zip(&parts)).unwrap().0
}

fn assert_margins(p: &PrintSettings, want: [f32; 6], what: &str) {
    for (i, (got, want)) in p.margins.iter().zip(want).enumerate() {
        assert!((got - want).abs() < 1e-3, "{what}: margin {i} is {got}, want {want} (all: {:?})", p.margins);
    }
}

/// LibreOffice's default page style: sheet-name header, "Page N" footer, no page margins.
/// Excel prints "Sheet1" centered at the top and "Page 1" centered at the bottom, and saves them
/// back as the sheet-name and page-number fields.
#[test]
fn default_master_page_header_and_footer_become_sheet_name_and_page_number_fields() {
    let s = styles(
        &layout(
            "Mpm1",
            r#"style:writing-mode="lr-tb""#,
            r#"fo:min-height="0.2953in" fo:margin-bottom="0.0984in""#,
            r#"fo:min-height="0.2953in" fo:margin-top="0.0984in""#,
        ),
        r#"<style:master-page style:name="Default" style:page-layout-name="Mpm1"><style:header><text:p><text:sheet-name>???</text:sheet-name></text:p></style:header><style:header-left style:display="false"/><style:footer><text:p>Page <text:page-number>1</text:page-number></text:p></style:footer><style:footer-left style:display="false"/></style:master-page>"#,
    );
    // One table without a style (falls back to "Default"), one whose style names it.
    let wb = read(&content(&[("Sheet1", None), ("Sheet2", Some("Default"))]), Some(&s));
    for sheet in &wb.sheets {
        let p = &sheet.print;
        assert_eq!(p.header, "&C&A", "{}", sheet.name);
        assert_eq!(p.footer, "&CPage &P", "{}", sheet.name);
        assert_eq!(p.orientation, Orientation::Portrait);
        // No fo:margin-* -> 0; top = 0 + 0.2953 + 0.0984; header margin = 0.
        assert_margins(p, [0.0, 0.0, 0.3937, 0.3937, 0.0, 0.0], &sheet.name);
    }
}

/// Left/center/right regions and every mapped field; literal ampersands are doubled, `text:s`
/// expands to spaces, and a field's cached display text is not copied.
#[test]
fn header_regions_and_fields_map_to_excel_codes() {
    let s = styles(
        &layout("pm1", "", "", ""),
        r#"<style:master-page style:name="mp1" style:page-layout-name="pm1"><style:header><style:region-left><text:p><text:file-name>x.ods</text:file-name></text:p></style:region-left><style:region-right><text:p><text:date text:date-value="2025-02-13">00/00/0000</text:date>, <text:time>00:00:00</text:time></text:p></style:region-right></style:header><style:footer><style:region-center><text:p>R&amp;D<text:s text:c="2"/>Page <text:page-number>1</text:page-number><text:s/>/ <text:page-count>99</text:page-count></text:p></style:region-center></style:footer></style:master-page>"#,
    );
    let wb = read(&content(&[("Data", Some("mp1"))]), Some(&s));
    let p = &wb.sheets[0].print;
    assert_eq!(p.header, "&L&F&R&D, &T");
    assert_eq!(p.footer, "&CR&&D  Page &P / &N");
}

/// Margins, header/footer heights and units (in, cm, mm, pt): the body starts below the header
/// (fo:margin-top + min-height + spacing); a header with display="false" takes no room; an
/// empty `<style:header/>` still does.
#[test]
fn margins_add_the_header_and_footer_heights_and_accept_units() {
    let layouts = [
        layout(
            "pm1",
            r#"fo:margin-top="0.3in" fo:margin-bottom="0.2in" fo:margin-left="0.45in" fo:margin-right="0.8in" style:print-orientation="landscape""#,
            r#"fo:min-height="0.4in" fo:margin-bottom="0in""#,
            r#"fo:min-height="0.4in" fo:margin-top="0in""#,
        ),
        layout(
            "pm2",
            r#"fo:margin-top="7.62mm" fo:margin-bottom="2.54cm" fo:margin-left="36pt" fo:margin-right="0.5in""#,
            r#"fo:min-height="0.5in" fo:margin-bottom="0.1in""#,
            r#"fo:min-height="0.5in" fo:margin-top="0.1in""#,
        ),
        layout(
            "pm3",
            r#"fo:margin-top="0.315in" fo:margin-bottom="0.315in" fo:margin-left="0.5in" fo:margin-right="0.5in""#,
            r#"fo:min-height="0.63in" fo:margin-bottom="0in""#,
            r#"fo:min-height="0.433in" fo:margin-top="0in""#,
        ),
    ]
    .concat();
    let masters = r#"<style:master-page style:name="mp1" style:page-layout-name="pm1"><style:header><text:p>Top</text:p></style:header><style:footer><text:p>Bottom</text:p></style:footer></style:master-page><style:master-page style:name="mp2" style:page-layout-name="pm2"><style:header style:display="false"><text:p>hidden</text:p></style:header><style:footer><text:p><text:page-number>1</text:page-number></text:p></style:footer></style:master-page><style:master-page style:name="mp3" style:page-layout-name="pm3"><style:header/><style:footer/></style:master-page>"#;
    let wb = read(&content(&[("A", Some("mp1")), ("B", Some("mp2")), ("C", Some("mp3"))]), Some(&styles(&layouts, masters)));
    let [a, b, c] = [&wb.sheets[0].print, &wb.sheets[1].print, &wb.sheets[2].print];
    assert_eq!(a.orientation, Orientation::Landscape);
    assert_eq!((a.header.as_str(), a.footer.as_str()), ("&CTop", "&CBottom"));
    assert_margins(a, [0.45, 0.8, 0.7, 0.6, 0.3, 0.2], "A");
    assert_eq!((b.header.as_str(), b.footer.as_str()), ("", "&C&P"));
    // Header hidden: top = header margin = 0.3in; footer: 1in + 0.5 + 0.1.
    assert_margins(b, [0.5, 0.5, 0.3, 1.6, 0.3, 1.0], "B");
    assert_eq!((c.header.as_str(), c.footer.as_str()), ("", ""));
    assert_margins(c, [0.5, 0.5, 0.945, 0.748, 0.315, 0.315], "C");
}

/// Scale, fit to pages, centering, page order, gridlines/headings and first page number.
#[test]
fn scale_fit_centering_order_and_print_flags() {
    let layouts = [
        layout("pm1", r#"style:scale-to="75%" style:table-centering="horizontal" style:print-page-order="ttb" style:print="objects charts drawings" style:first-page-number="continue""#, "", ""),
        layout("pm2", r#"style:scale-to-X="2" style:scale-to-Y="3" style:table-centering="both" style:print-page-order="ltr" style:print="grid headers objects" style:first-page-number="5""#, "", ""),
        layout("pm3", r#"style:scale-to-X="1" style:table-centering="vertical""#, "", ""),
        layout("pm4", r#"style:scale-to-pages="1""#, "", ""),
    ]
    .concat();
    let masters: String = (1..=4).map(|i| format!(r#"<style:master-page style:name="mp{i}" style:page-layout-name="pm{i}"/>"#)).collect();
    let wb = read(&content(&[("A", Some("mp1")), ("B", Some("mp2")), ("C", Some("mp3")), ("D", Some("mp4"))]), Some(&styles(&layouts, &masters)));
    let p: Vec<&PrintSettings> = wb.sheets.iter().map(|s| &s.print).collect();

    assert_eq!((p[0].scale, p[0].fit_to), (75, None));
    assert_eq!((p[0].center_h, p[0].center_v), (true, false));
    assert_eq!(p[0].page_order, PageOrder::DownThenOver);
    assert_eq!((p[0].gridlines, p[0].headings), (false, false));
    assert_eq!(p[0].output.first_page_number, None);

    assert_eq!(p[1].fit_to, Some((2, 3)));
    assert_eq!((p[1].center_h, p[1].center_v), (true, true));
    assert_eq!(p[1].page_order, PageOrder::OverThenDown);
    assert_eq!((p[1].gridlines, p[1].headings), (true, true));
    assert_eq!(p[1].output.first_page_number, Some(5));
    assert_eq!(p[1].output.use_first_page_number, Some(true));

    assert_eq!(p[2].fit_to, Some((1, 0)));
    assert_eq!((p[2].center_h, p[2].center_v), (false, true));

    assert_eq!(p[3].fit_to, Some((1, 1)));
}

/// Page size picks the matching paper name in either orientation; an unknown size keeps the default.
#[test]
fn page_size_selects_the_paper_name() {
    let layouts = [
        layout("pm1", r#"fo:page-width="210mm" fo:page-height="297mm""#, "", ""),
        layout("pm2", r#"fo:page-width="14in" fo:page-height="8.5in" style:print-orientation="landscape""#, "", ""),
        layout("pm3", r#"fo:page-width="5in" fo:page-height="5in""#, "", ""),
    ]
    .concat();
    let masters: String = (1..=3).map(|i| format!(r#"<style:master-page style:name="mp{i}" style:page-layout-name="pm{i}"/>"#)).collect();
    let wb = read(&content(&[("A", Some("mp1")), ("B", Some("mp2")), ("C", Some("mp3"))]), Some(&styles(&layouts, &masters)));
    assert_eq!(wb.sheets[0].print.paper, "A4");
    assert_eq!(wb.sheets[1].print.paper, "Legal");
    assert_eq!(wb.sheets[1].print.orientation, Orientation::Landscape);
    assert_eq!(wb.sheets[2].print.paper, PrintSettings::default().paper);
}

/// Guard: a package without styles.xml, or a table naming an unknown master page with no
/// "Default", opens with today's default print settings and its cells.
#[test]
fn missing_styles_or_master_page_keeps_default_print_settings() {
    let wb = read(&content(&[("Data", None)]), None);
    assert_eq!(wb.sheets[0].print, PrintSettings::default());
    let s = styles(&layout("pm1", r#"style:scale-to="50%""#, "", ""), r#"<style:master-page style:name="Other" style:page-layout-name="pm1"/>"#);
    let wb = read(&content(&[("Data", Some("Nope"))]), Some(&s));
    assert_eq!(wb.sheets[0].print, PrintSettings::default());
    assert_eq!(wb.sheets[0].value(gridcraft_core::CellRef::new(0, 0)), gridcraft_core::Value::Number(1.0));
}
