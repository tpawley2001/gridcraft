//! Flash Fill learns parts of a word: characters by position, letter/digit runs inside a word, and
//! prefers the source's text over literal text copied from the example. Expected values are what
//! Microsoft Excel's Flash Fill gives for the same columns.

use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use serde_json::json;

/// Puts `sources` in A1.., `examples` in B1.., runs Flash Fill over the rest of column B and
/// returns the text of the filled B rows.
fn flash(sources: &[&str], examples: &[&str]) -> Vec<String> {
    let mut s = Session::new();
    s.new_workbook();
    for (r, v) in sources.iter().enumerate() {
        s.execute("cell.set", json!({"cell": format!("A{}", r + 1), "input": format!("'{v}")})).unwrap();
    }
    for (r, v) in examples.iter().enumerate() {
        s.execute("cell.set", json!({"cell": format!("B{}", r + 1), "input": format!("'{v}")})).unwrap();
    }
    let range = format!("B{}:B{}", examples.len() + 1, sources.len());
    s.execute("edit.flashFill", json!({"range": range})).unwrap();
    let sh = s.doc().unwrap().wb.sheet(0).unwrap();
    (examples.len()..sources.len())
        .map(|r| match sh.value(CellRef::new(r as u32, 1)) {
            Value::Text(t) => t.to_string(),
            Value::Empty => String::new(),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn phone_numbers_take_their_digits_by_position() {
    assert_eq!(flash(&["4255550123", "2069871234", "3604442222"], &["(425) 555-0123"]), ["(206) 987-1234", "(360) 444-2222"]);
}

#[test]
fn digits_after_letters_are_a_run_of_their_own() {
    assert_eq!(flash(&["ABC123", "XY9876", "Q55", "LMNOP7"], &["123", "9876"]), ["55", "7"]);
}

#[test]
fn last_characters_of_a_word_count_from_its_end() {
    assert_eq!(flash(&["AB12345678", "X123456", "QQQ99998888", "Z4321"], &["5678", "3456"]), ["8888", "4321"]);
}

#[test]
fn a_whole_word_and_part_of_another_with_case() {
    assert_eq!(flash(&["ab-7781", "cd-0042", "ef-9310"], &["AB/81"]), ["CD/42", "EF/10"]);
}

#[test]
fn whole_words_still_swap_and_change_case() {
    assert_eq!(flash(&["Smith, John", "Fuller, Andrew", "Leverling, Janet"], &["John Smith"]), ["Andrew Fuller", "Janet Leverling"]);
    assert_eq!(flash(&["john smith", "andrew fuller"], &["John Smith"]), ["Andrew Fuller"]);
}
