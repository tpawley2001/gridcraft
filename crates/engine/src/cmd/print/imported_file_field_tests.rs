//! `&F` in a header or footer prints the name of the file the workbook was opened from. An
//! imported workbook (here SYLK) is titled `<stem> (imported).xlsx` so Save asks for a new
//! .xlsx, but its printed `&F` is the original file name, as in Excel; after Save As it is the
//! new file's name.

use serde_json::json;

use crate::Session;

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn pdf(s: &mut Session) -> Vec<u8> {
    let r = s.execute("file.exportPdf", json!({})).unwrap();
    crate::io::base64_decode(r["base64"].as_str().unwrap()).unwrap()
}

fn open_with_file_footer(name: &str, bytes: &[u8]) -> Session {
    let mut s = Session::new();
    s.execute("file.open", json!({"name": name, "base64": crate::io::base64_encode(bytes)})).unwrap();
    s.execute("pageLayout.headerFooter", json!({"footer": "&C&F"})).unwrap();
    s
}

const SYLK: &[u8] = b"ID;P\r\nC;Y1;X1;K7\r\nE\r\n";

#[test]
fn imported_workbook_prints_its_original_file_name() {
    let mut s = open_with_file_footer("report.slk", SYLK);
    let out = pdf(&mut s);
    assert!(contains(&out, "(report.slk) Tj"), "&F prints the opened file's name");
    assert!(!contains(&out, "(imported)"), "&F never prints the import title");
}

#[test]
fn imported_workbook_title_is_unchanged() {
    let s = open_with_file_footer("report.slk", SYLK);
    assert_eq!(s.doc().unwrap().display_title(), "report (imported).xlsx");
}

#[test]
fn after_save_as_the_new_name_prints() {
    let mut s = open_with_file_footer("report.slk", SYLK);
    let dir = std::env::temp_dir().join(format!("gc-imported-file-field-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("renamed.xlsx");
    s.execute("file.saveAs", json!({"path": path.to_string_lossy()})).unwrap();
    let out = pdf(&mut s);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(contains(&out, "(renamed.xlsx) Tj"));
    assert!(!contains(&out, "(report.slk) Tj"));
}

#[test]
fn xlsx_and_new_workbooks_keep_their_names() {
    let mut s = Session::new();
    s.new_workbook();
    s.execute("pageLayout.headerFooter", json!({"footer": "&C&F"})).unwrap();
    let title = s.doc().unwrap().display_title();
    let out = pdf(&mut s);
    assert!(contains(&out, &format!("({title}) Tj")));
}
