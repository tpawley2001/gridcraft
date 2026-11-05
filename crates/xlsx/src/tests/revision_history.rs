//! Shared workbook revision history (ECMA-376 Part 1 §18.11): kept on ordinary saves, dropped
//! whole when the sheets it names change, never kept with a header pointing at nothing.

use super::make_zip;
use crate::xml::{El, parse};
use crate::{read_xlsx, write_xlsx};
use std::io::Read;
use std::sync::Arc;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT_HEADERS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionHeaders+xml";
const CT_LOG: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.revisionLog+xml";

/// A workbook with sheets `Data` (sheetId 7) and `More` (sheetId 3) and, when `history`, a
/// revision headers part whose header names `log_target` through relationship `log`.
fn book(history: bool, log_target: &str, with_log: bool) -> Vec<u8> {
    let mut ct = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#,
    );
    if history {
        ct.push_str(&format!(r#"<Override PartName="/xl/revisions/revisionHeaders.xml" ContentType="{CT_HEADERS}"/>"#));
        ct.push_str(&format!(r#"<Override PartName="/xl/revisions/revisionLog1.xml" ContentType="{CT_LOG}"/>"#));
    }
    ct.push_str("</Types>");
    let root =
        format!(r#"<Relationships xmlns="{PKG_REL}"><Relationship Id="book" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#);
    let wb = format!(
        r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets><sheet name="Data" sheetId="7" r:id="s1"/><sheet name="More" sheetId="3" r:id="s2"/></sheets></workbook>"#
    );
    let extra = if history {
        format!(r#"<Relationship Id="hist" Type="{REL}/revisionHeaders" Target="revisions/revisionHeaders.xml"/>"#)
    } else {
        String::new()
    };
    let wr = format!(
        r#"<Relationships xmlns="{PKG_REL}"><Relationship Id="s1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="s2" Type="{REL}/worksheet" Target="worksheets/sheet2.xml"/>{extra}</Relationships>"#
    );
    let sheet = format!(r#"<worksheet xmlns="{MAIN}"><sheetData><row r="1"><c r="A1"><v>2</v></c></row></sheetData></worksheet>"#);
    let headers = format!(
        r#"<headers xmlns="{MAIN}" xmlns:r="{REL}" guid="{{11111111-1111-1111-1111-111111111111}}" lastGuid="{{22222222-2222-2222-2222-222222222222}}" shared="1"><header guid="{{22222222-2222-2222-2222-222222222222}}" dateTime="2026-10-01T00:00:00Z" userName="Ada" r:id="log" minRId="5" maxRId="5" maxSheetId="8"><sheetIdMap count="2"><sheetId val="7"/><sheetId val="3"/></sheetIdMap></header></headers>"#
    );
    let hr = format!(r#"<Relationships xmlns="{PKG_REL}"><Relationship Id="log" Type="{REL}/revisionLog" Target="{log_target}"/></Relationships>"#);
    let log = format!(r#"<revisions xmlns="{MAIN}"><rcc rId="5" sId="7"><oc r="A1"><v>1</v></oc><nc r="A1"><v>2</v></nc></rcc></revisions>"#);
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", ct.as_bytes()),
        ("_rels/.rels", root.as_bytes()),
        ("xl/workbook.xml", wb.as_bytes()),
        ("xl/_rels/workbook.xml.rels", wr.as_bytes()),
        ("xl/worksheets/sheet1.xml", sheet.as_bytes()),
        ("xl/worksheets/sheet2.xml", sheet.as_bytes()),
    ];
    if history {
        parts.push(("xl/revisions/revisionHeaders.xml", headers.as_bytes()));
        parts.push(("xl/revisions/_rels/revisionHeaders.xml.rels", hr.as_bytes()));
        if with_log {
            parts.push(("xl/revisions/revisionLog1.xml", log.as_bytes()));
        }
    }
    make_zip(&parts)
}

fn names(bytes: &[u8]) -> Vec<String> {
    let z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    z.file_names().map(str::to_string).collect()
}

fn part(bytes: &[u8], name: &str) -> El {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut b = Vec::new();
    z.by_name(name).unwrap().read_to_end(&mut b).unwrap();
    parse(&b).unwrap()
}

/// The part a relationship of `kind` from `owner` names, if any.
fn target(bytes: &[u8], owner: &str, kind: &str) -> Option<String> {
    let (dir, file) = owner.rsplit_once('/').unwrap();
    let rels = part(bytes, &format!("{dir}/_rels/{file}.rels"));
    let ty = format!("{REL}/{kind}");
    let t = rels.kids("Relationship").find(|r| r.attr("Type") == Some(ty.as_str()))?.attr("Target")?.to_string();
    Some(match t.strip_prefix('/') {
        Some(abs) => abs.to_string(),
        None => format!("{dir}/{t}"),
    })
}

fn history_kept(out: &[u8]) -> bool {
    let kept = target(out, "xl/workbook.xml", "revisionHeaders").is_some();
    let any_part = names(out).iter().any(|n| {
        let Ok(x) = parse(&{
            let mut z = zip::ZipArchive::new(std::io::Cursor::new(out)).unwrap();
            let mut b = Vec::new();
            z.by_name(n).unwrap().read_to_end(&mut b).unwrap();
            b
        }) else {
            return false;
        };
        x.name == "headers" || x.name == "revisions"
    });
    assert_eq!(kept, any_part, "a revision part without its relationship, or the reverse");
    kept
}

#[test]
fn sheet_ids_and_header_log_link_survive_with_original_sheet_ids() {
    let (wb, report) = read_xlsx(&book(true, "revisionLog1.xml", true)).unwrap();
    assert_eq!(report.warnings.iter().filter(|w| w.contains("revision history")).count(), 1, "{:?}", report.warnings);
    let out = write_xlsx(&wb).unwrap();
    assert!(history_kept(&out));
    let wbx = part(&out, "xl/workbook.xml");
    let ids: Vec<_> = wbx.child("sheets").unwrap().kids("sheet").map(|s| s.attr("sheetId").unwrap().to_string()).collect();
    assert_eq!(ids, ["7", "3"]);
    let h = target(&out, "xl/workbook.xml", "revisionHeaders").unwrap();
    let headers = part(&out, &h);
    let header = headers.child("header").unwrap();
    assert_eq!(header.attr("guid"), Some("{22222222-2222-2222-2222-222222222222}"));
    assert_eq!(header.attr_u32("maxRId"), Some(5));
    let map: Vec<_> = header.child("sheetIdMap").unwrap().kids("sheetId").map(|s| s.attr("val").unwrap().to_string()).collect();
    assert_eq!(map, ["7", "3"]);
    // The header's r:id resolves, through the headers part's own rels, to the log.
    let (dir, file) = h.rsplit_once('/').unwrap();
    let rels = part(&out, &format!("{dir}/_rels/{file}.rels"));
    let rel = rels.kids("Relationship").find(|r| r.attr("Id") == header.attr("id")).unwrap();
    assert_eq!(rel.attr("Type"), Some(format!("{REL}/revisionLog").as_str()));
    let log = target(&out, &h, "revisionLog").unwrap();
    let rcc = part(&out, &log);
    let rcc = rcc.child("rcc").unwrap();
    assert_eq!((rcc.attr("rId"), rcc.attr("sId")), (Some("5"), Some("7")));
    let ct = part(&out, "[Content_Types].xml");
    let ty = |n: &str| {
        ct.kids("Override").find(|o| o.attr("PartName") == Some(format!("/{n}").as_str())).and_then(|o| o.attr("ContentType")).map(str::to_string)
    };
    assert_eq!(ty(&h).as_deref(), Some(CT_HEADERS));
    assert_eq!(ty(&log).as_deref(), Some(CT_LOG));
}

#[test]
fn missing_revision_log_drops_the_history_without_dangling_rels() {
    let (wb, report) = read_xlsx(&book(true, "revisionLog1.xml", false)).unwrap();
    assert!(report.warnings.iter().any(|w| w.contains("revisionHeaders") && w.contains("dropped")), "{:?}", report.warnings);
    assert!(!history_kept(&write_xlsx(&wb).unwrap()));
}

#[test]
fn header_naming_no_log_relationship_drops_the_history() {
    // The headers part's only relationship has another id than the header's r:id.
    let bytes = book(true, "revisionLog1.xml", true);
    let bytes = {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.as_slice())).unwrap();
        let mut parts = vec![];
        for i in 0..z.len() {
            let mut f = z.by_index(i).unwrap();
            let mut b = Vec::new();
            f.read_to_end(&mut b).unwrap();
            let name = f.name().to_string();
            if name.ends_with("revisionHeaders.xml.rels") {
                b = String::from_utf8(b).unwrap().replace("Id=\"log\"", "Id=\"other\"").into_bytes();
            }
            parts.push((name, b));
        }
        let refs: Vec<(&str, &[u8])> = parts.iter().map(|(n, b)| (n.as_str(), b.as_slice())).collect();
        make_zip(&refs)
    };
    let (wb, _) = read_xlsx(&bytes).unwrap();
    assert!(!history_kept(&write_xlsx(&wb).unwrap()));
}

#[test]
fn renaming_adding_deleting_or_moving_sheets_drops_the_whole_history() {
    let edits: [fn(&mut gridcraft_model::Workbook); 4] = [
        |wb| wb.sheet_mut(1).unwrap().name = "Renamed".into(),
        |wb| wb.sheets.push(Arc::new(gridcraft_model::Sheet::new("New"))),
        |wb| {
            wb.sheets.remove(1);
        },
        |wb| wb.sheets.swap(0, 1),
    ];
    for edit in edits {
        let (mut wb, _) = read_xlsx(&book(true, "revisionLog1.xml", true)).unwrap();
        edit(&mut wb);
        let out = write_xlsx(&wb).unwrap();
        assert!(!history_kept(&out));
        // Saved again, it stays gone.
        let (wb, _) = read_xlsx(&out).unwrap();
        assert!(!history_kept(&write_xlsx(&wb).unwrap()));
    }
}

#[test]
fn workbook_without_history_gets_none_and_no_warning() {
    let (wb, report) = read_xlsx(&book(false, "", false)).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("revision")), "{:?}", report.warnings);
    assert!(!history_kept(&write_xlsx(&wb).unwrap()));
}
