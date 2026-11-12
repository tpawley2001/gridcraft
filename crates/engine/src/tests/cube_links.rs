//! A cell's link to its cube (MDX) record, kept from a file in `Sheet::cell_mdx`, follows the cell
//! through edits that move it, survives recalculation, and is dropped when the content is replaced.

use gridcraft_core::CellRef;
use serde_json::json;

use super::s;
use crate::Session;

fn at(a: &str) -> CellRef {
    CellRef::parse(a).unwrap()
}

fn links(s: &Session) -> Vec<(String, u32)> {
    s.doc().unwrap().wb.active().unwrap().cell_mdx.iter().map(|(c, n)| (c.a1(), *n)).collect()
}

fn link(s: &mut Session, cell: &str, record: u32) {
    let sh = std::sync::Arc::make_mut(&mut s.doc_mut().unwrap().wb).sheet_mut(0).unwrap();
    sh.cell_mdx.insert(at(cell), record);
}

#[test]
fn recalculation_keeps_the_link_and_an_edit_drops_it() {
    let mut s = s();
    s.execute("cell.set", json!({"cell": "A1", "input": "2"})).unwrap();
    s.execute("cell.set", json!({"cell": "B1", "input": "=A1*2"})).unwrap();
    link(&mut s, "B1", 0);
    s.execute("cell.set", json!({"cell": "A1", "input": "3"})).unwrap();
    assert_eq!(links(&s), [("B1".to_string(), 0)]);
    s.execute("cell.set", json!({"cell": "B1", "input": "7"})).unwrap();
    assert!(links(&s).is_empty());
}

#[test]
fn links_follow_inserted_and_deleted_rows_and_sorts() {
    let mut s = s();
    s.execute("range.setValues", json!({"range": "A1", "values": [[3], [1], [2]]})).unwrap();
    link(&mut s, "A1", 5);
    s.execute("home.insertRows", json!({"rows": "1:1"})).unwrap();
    assert_eq!(links(&s), [("A2".to_string(), 5)]);
    s.execute("data.sortAscending", json!({"range": "A2:A4", "column": "A"})).unwrap();
    assert_eq!(links(&s), [("A4".to_string(), 5)]);
    s.execute("home.deleteRows", json!({"rows": "4:4"})).unwrap();
    assert!(links(&s).is_empty());
}
