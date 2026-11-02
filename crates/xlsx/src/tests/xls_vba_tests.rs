//! Macros in legacy `.xls` (BIFF8) files: the `_VBA_PROJECT_CUR` storage ([MS-XLS] 2.1.7,
//! [MS-OVBA] 2.2.1) becomes the workbook's VBA project and saves into an `.xlsm`. Every file is
//! assembled here in code; the "VBA" streams are arbitrary bytes, never parsed or run.

use gridcraft_core::Value;

use super::xls_tests::{one_sheet_with, v};
use crate::cfb::{Compound, Node, write_compound};
use crate::{XlsxKind, read_xls, read_xlsx, write_xlsx_as};

const KEPT: &str = "macros (VBA project) were kept but not executed";
const DROPPED: &str = "macros (VBA project) could not be read and were dropped";

fn bytes(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| (i as u8).wrapping_mul(13).wrapping_add(seed)).collect()
}

fn stream(name: &str, data: Vec<u8>) -> Node {
    Node::Stream { name: name.into(), data }
}

/// The `_VBA_PROJECT_CUR` subtree: a `VBA` storage with three streams and a `PROJECT` stream.
fn project(module_len: usize) -> Vec<Node> {
    vec![
        Node::Storage {
            name: "VBA".into(),
            clsid: [0; 16],
            children: vec![stream("dir", bytes(120, 1)), stream("_VBA_PROJECT", bytes(7, 2)), stream("Module1", bytes(module_len, 3))],
        },
        stream("PROJECT", bytes(300, 4)),
    ]
}

/// The one-sheet test workbook (A1 = 7), repackaged with `extra` next to its Workbook stream.
fn book_with(extra: Vec<Node>) -> Vec<u8> {
    let plain = one_sheet_with(&[]);
    let workbook = Compound::open(&plain).unwrap().root_stream("Workbook").unwrap().unwrap();
    let mut root = vec![stream("Workbook", workbook)];
    root.extend(extra);
    write_compound(&root).unwrap()
}

fn sorted(mut nodes: Vec<Node>) -> Vec<Node> {
    nodes.sort_by(|a, b| a.name().cmp(b.name()));
    nodes
        .into_iter()
        .map(|n| match n {
            Node::Storage { name, clsid, children } => Node::Storage { name, clsid, children: sorted(children) },
            s => s,
        })
        .collect()
}

/// Every child of `bin`'s root, read back through `Compound`.
fn tree(bin: &[u8]) -> Vec<Node> {
    let cf = Compound::open(bin).unwrap();
    let vba = cf.root_storage("VBA").unwrap().unwrap();
    let project = cf.root_stream("PROJECT").unwrap().unwrap();
    sorted(vec![Node::Storage { name: "VBA".into(), clsid: [0; 16], children: vba }, stream("PROJECT", project)])
}

#[test]
fn the_vba_project_storage_becomes_the_vba_project() {
    let file = book_with(vec![
        Node::Storage { name: "_VBA_PROJECT_CUR".into(), clsid: [0; 16], children: project(5000) },
        stream("\u{5}DigitalSignature", bytes(64, 9)),
    ]);
    let (wb, report) = read_xls(&file).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::Number(7.0));
    assert!(report.warnings.iter().any(|w| w == KEPT), "{:?}", report.warnings);
    let bin = wb.vba_project.as_ref().expect("VBA project kept");
    assert_eq!(tree(bin), sorted(project(5000)));
    // The signature at the file's root stays behind.
    assert!(!Compound::open(bin).unwrap().has_root_stream("\u{5}DigitalSignature"));

    // Saved as .xlsm and read back, the project is the same bytes.
    let xlsm = write_xlsx_as(&wb, XlsxKind::MacroWorkbook).unwrap();
    let (back, _) = read_xlsx(&xlsm).unwrap();
    assert_eq!(back.vba_project.as_deref(), Some(&**bin));
}

#[test]
fn a_file_without_macros_has_no_vba_project() {
    let (wb, report) = read_xls(&one_sheet_with(&[])).unwrap();
    assert!(wb.vba_project.is_none());
    assert!(!report.warnings.iter().any(|w| w.contains("macros (VBA project)")), "{:?}", report.warnings);
}

#[test]
fn a_truncated_vba_storage_is_dropped_and_the_cells_still_load() {
    // Module1 is the only regular-sector stream, so it is last in the file: cut into it.
    let mut file = book_with(vec![Node::Storage { name: "_VBA_PROJECT_CUR".into(), clsid: [0; 16], children: project(9000) }]);
    file.truncate(file.len() - 2000);
    let (wb, report) = read_xls(&file).unwrap();
    assert_eq!(v(&wb, 0, "A1"), Value::Number(7.0));
    assert!(wb.vba_project.is_none());
    assert!(report.warnings.iter().any(|w| w.starts_with(DROPPED)), "{:?}", report.warnings);
}
