//! Legacy `.xls` (BIFF8) page setup import: Setup, margins, headers/footers, print options,
//! page breaks and the sheet tab colour, built from the [MS-XLS] 2.4 record layouts.

use gridcraft_core::Value;
use gridcraft_model::{Color, Orientation, PrintSettings, Sheet};

use super::xls_tests::{cat, one_sheet_with, rec, u16s, v, xlstr};
use crate::{read_xls, read_xlsx, write_xlsx};

/// Setup (0x00A1): iPaperSize, iScale, iPageStart, iFitWidth, iFitHeight, flags, iRes, iVRes,
/// numHdr, numFtr, iCopies.
fn setup(paper: u16, scale: u16, fit: (u16, u16), flags: u16, hdr: f64, ftr: f64) -> Vec<u8> {
    rec(0x00A1, &cat(&[&u16s(&[paper, scale, 1, fit.0, fit.1, flags, 600, 600]), &hdr.to_le_bytes(), &ftr.to_le_bytes(), &u16s(&[1])]))
}

const PORTRAIT: u16 = 0x0002;
const NO_PLS: u16 = 0x0004;

fn margin(id: u16, inches: f64) -> Vec<u8> {
    rec(id, &inches.to_le_bytes())
}

fn wsbool(fit_to_page: bool) -> Vec<u8> {
    rec(0x0081, &[0xC1, if fit_to_page { 0x05 } else { 0x04 }])
}

/// A FrtHeader for `rt`.
fn frt(rt: u16) -> Vec<u8> {
    cat(&[&u16s(&[rt, 0]), &[0; 8]])
}

/// The first sheet of a one-sheet workbook holding `records`, with the import's warnings.
fn sheet(records: &[u8]) -> (Sheet, Vec<String>) {
    let (wb, report) = read_xls(&one_sheet_with(records)).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::Number(7.0));
    ((*wb.sheets[0]).clone(), report.warnings)
}

/// The print settings after saving the sheet as .xlsx and reading it back: what the .xlsx
/// reader makes of the same settings.
fn via_xlsx(records: &[u8]) -> (PrintSettings, Option<Color>) {
    let (wb, _) = read_xls(&one_sheet_with(records)).unwrap();
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    (back.sheets[0].print.clone(), back.sheets[0].tab_color)
}

#[test]
fn xls_setup_landscape_a4_scale_and_header_footer_margins() {
    let records = setup(9, 75, (1, 1), 0, 0.3, 0.3);
    let (sh, _) = sheet(&records);
    let p = &sh.print;
    assert_eq!(p.orientation, Orientation::Landscape);
    assert_eq!(p.paper, "A4");
    assert_eq!(p.scale, 75);
    assert_eq!((p.margins[4], p.margins[5]), (0.3, 0.3));
    // No WsBool fFitToPage: the fit counts don't apply.
    assert_eq!(p.fit_to, None);
    assert_eq!(via_xlsx(&records).0, *p);

    let (sh, _) = sheet(&setup(9, 75, (1, 1), PORTRAIT, 0.5, 0.4));
    assert_eq!(sh.print.orientation, Orientation::Portrait);
    assert_eq!((sh.print.margins[4], sh.print.margins[5]), (0.5, 0.4));
    // fNoOrient: the orientation is the default even with fPortrait clear.
    let (sh, _) = sheet(&setup(9, 75, (1, 1), 0x0040, 0.3, 0.3));
    assert_eq!(sh.print.orientation, Orientation::Portrait);
    // A zero scale is unstated, like `<pageSetup scale="0">`.
    let (sh, _) = sheet(&setup(9, 0, (1, 1), 0, 0.3, 0.3));
    assert_eq!(sh.print.scale, 100);
}

#[test]
fn xls_setup_no_printer_settings_keeps_paper_scale_and_orientation_defaults() {
    let (sh, _) = sheet(&setup(9, 75, (1, 1), NO_PLS, 0.2, 0.25));
    let d = PrintSettings::default();
    assert_eq!((sh.print.paper.as_str(), sh.print.scale, sh.print.orientation), (d.paper.as_str(), d.scale, d.orientation));
    // The header and footer margins are still defined.
    assert_eq!((sh.print.margins[4], sh.print.margins[5]), (0.2, 0.25));
}

#[test]
fn xls_fit_to_one_page_wide_and_automatic_tall() {
    for records in
        [cat(&[&wsbool(true), &setup(1, 100, (1, 0), PORTRAIT, 0.3, 0.3)]), cat(&[&setup(1, 100, (1, 0), PORTRAIT, 0.3, 0.3), &wsbool(true)])]
    {
        let (sh, _) = sheet(&records);
        assert_eq!(sh.print.fit_to, Some((1, 0)));
        assert_eq!(via_xlsx(&records).0.fit_to, Some((1, 0)));
    }
    let (sh, _) = sheet(&cat(&[&wsbool(false), &setup(1, 100, (1, 0), PORTRAIT, 0.3, 0.3)]));
    assert_eq!(sh.print.fit_to, None);
}

#[test]
fn xls_margins_and_print_options() {
    let records = cat(&[
        &margin(0x0026, 0.5),
        &margin(0x0027, 0.6),
        &margin(0x0028, 1.1),
        &margin(0x0029, 1.2),
        &rec(0x002B, &u16s(&[1])),
        &rec(0x002A, &u16s(&[1])),
        &rec(0x0083, &u16s(&[1])),
        &rec(0x0084, &u16s(&[0])),
    ]);
    let (sh, _) = sheet(&records);
    let p = &sh.print;
    assert_eq!(&p.margins[..4], &[0.5, 0.6, 1.1, 1.2]);
    // Header and footer margins stay at the defaults without a Setup record.
    assert_eq!(&p.margins[4..], &PrintSettings::default().margins[4..]);
    assert_eq!((p.gridlines, p.headings, p.center_h, p.center_v), (true, true, true, false));
    assert_eq!(via_xlsx(&records).0, *p);

    // Absent: the defaults.
    let (sh, _) = sheet(&[]);
    assert_eq!(sh.print, PrintSettings::default());
}

#[test]
fn xls_headers_footers_and_different_first_page() {
    let header = "&L&\"Arial,Bold\"Left&CPage &P of &N&RRight";
    let footer = "&CConfidential";
    let first = "&CTitle page";
    // HeaderFooter: FrtHeader, zero guidSView, fHFDiffFirst | fHFScaleWithDoc | fHFAlignMargins,
    // the even header/footer and first header/footer counts, then the first header only.
    let hf = rec(0x089C, &cat(&[&frt(0x089C), &[0; 16], &u16s(&[0x0002 | 0x0004 | 0x0008, 0, 0, first.len() as u16, 0]), &xlstr(first)]));
    let records = cat(&[&rec(0x0014, &xlstr(header)), &rec(0x0015, &xlstr(footer)), &hf]);
    let (sh, warnings) = sheet(&records);
    let p = &sh.print;
    assert_eq!((p.header.as_str(), p.footer.as_str()), (header, footer));
    assert!(p.different_first && !p.different_odd_even && p.hf_scale_with_doc && p.hf_align_with_margins);
    assert_eq!((p.first_header.as_str(), p.first_footer.as_str(), p.even_header.as_str()), (first, "", ""));
    assert!(!warnings.iter().any(|w| w.contains("page setup")), "{warnings:?}");
    assert_eq!(via_xlsx(&records).0, *p);

    // Empty Header/Footer records: none. UTF-16 text reads too.
    let wide = cat(&[&2u16.to_le_bytes(), &[1], &u16s(&['&' as u16, 0x00E9])]);
    let (sh, _) = sheet(&cat(&[&rec(0x0014, &[]), &rec(0x0015, &wide)]));
    assert_eq!((sh.print.header.as_str(), sh.print.footer.as_str()), ("", "&é"));

    // Scale with document and align with margins both off.
    let hf = rec(0x089C, &cat(&[&frt(0x089C), &[0; 16], &u16s(&[0x0001, 0, 0, 0, 0])]));
    let (sh, _) = sheet(&hf);
    assert!(sh.print.different_odd_even && !sh.print.hf_scale_with_doc && !sh.print.hf_align_with_margins);
}

#[test]
fn xls_manual_page_breaks() {
    let rows = rec(0x001B, &u16s(&[2, 10, 0, 255, 20, 0, 255]));
    let cols = rec(0x001A, &u16s(&[1, 5, 0, 65535]));
    let records = cat(&[&rows, &cols]);
    let (sh, _) = sheet(&records);
    assert_eq!(sh.print.row_breaks, [10, 20]);
    assert_eq!(sh.print.col_breaks, [5]);
    let (back, _) = via_xlsx(&records);
    assert_eq!((back.row_breaks, back.col_breaks), (sh.print.row_breaks.clone(), sh.print.col_breaks.clone()));
}

#[test]
fn xls_sheet_tab_colour() {
    let ext = |icv: u32, optional: &[u8]| {
        let cb: u32 = if optional.is_empty() { 0x14 } else { 0x28 };
        rec(0x0862, &cat(&[&frt(0x0862), &cb.to_le_bytes(), &icv.to_le_bytes(), optional]))
    };
    let records = ext(10, &[]);
    let (sh, _) = sheet(&records);
    assert_eq!(sh.tab_color, Some(Color::Rgb(0xFF0000)));
    assert_eq!(via_xlsx(&records).1, sh.tab_color);
    // 0x7F: no tab colour.
    assert_eq!(sheet(&ext(0x7F, &[])).0.tab_color, None);
    // SheetExtOptional with an RGB CFColor: the RGB wins over the palette index.
    let rgb = cat(&[&10u32.to_le_bytes(), &[0], &2u32.to_le_bytes(), &[0x12, 0x34, 0x56, 0xFF], &0f64.to_le_bytes(), &[0; 3]]);
    assert_eq!(sheet(&ext(10, &rgb)).0.tab_color, Some(Color::Rgb(0x123456)));
    // A theme CFColor is skipped: the palette index stands.
    let theme = cat(&[&10u32.to_le_bytes(), &[0], &3u32.to_le_bytes(), &4u32.to_le_bytes(), &0f64.to_le_bytes(), &[0; 3]]);
    assert_eq!(sheet(&ext(12, &theme)).0.tab_color, Some(Color::Rgb(0x0000FF)));
}

#[test]
fn xls_damaged_page_setup_records_are_skipped_with_a_warning() {
    let full = setup(9, 75, (1, 1), 0, 0.3, 0.3);
    // Every truncation of Setup: the sheet opens with default settings and one warning.
    // The fields read end with numFtr, 32 bytes in.
    for len in 0..32 {
        let (sh, warnings) = sheet(&rec(0x00A1, &full[4..4 + len]));
        assert_eq!(sh.print, PrintSettings::default(), "length {len}");
        assert_eq!(warnings.iter().filter(|w| w.contains("page setup")).count(), 1, "{warnings:?}");
    }
    // Every other kind, truncated: skipped, never a panic.
    for id in [0x0026u16, 0x0014, 0x0015, 0x089C, 0x002B, 0x0083, 0x001B, 0x001A, 0x0862] {
        for payload in [&[][..], &[3][..], &[3, 0, 1][..], &[0xFF; 20][..]] {
            let (sh, _) = sheet(&rec(id, payload));
            assert!(sh.print.header.len() <= 255);
        }
    }
    // Two damaged Setup records still give one warning.
    let (_, warnings) = sheet(&cat(&[&rec(0x00A1, &[1, 2]), &rec(0x00A1, &[1, 2, 3])]));
    assert_eq!(warnings.iter().filter(|w| w.contains("page setup")).count(), 1, "{warnings:?}");
}
