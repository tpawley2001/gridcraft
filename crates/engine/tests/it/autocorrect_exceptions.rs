//! AutoCorrect's INitial CAps exceptions: listed words skip the TWo INitial CApitals rule.

use gridcraft_engine::autocorrect::AutoCorrect;

fn with(words: &[&str]) -> AutoCorrect {
    AutoCorrect { two_caps_exceptions: words.iter().map(|w| w.to_string()).collect(), ..AutoCorrect::default() }
}

#[test]
fn the_default_list_is_empty() {
    let a = AutoCorrect::default();
    assert!(a.two_caps_exceptions.is_empty());
    assert_eq!(a.correct("IDs"), "Ids");
}

#[test]
fn a_listed_word_stays_as_typed() {
    let a = with(&["IDs"]);
    assert_eq!(a.correct("IDs"), "IDs");
    assert_eq!(a.correct("my IDs, CDs"), "my IDs, Cds");
    assert_eq!(a.correct("COlumn"), "Column");
}

#[test]
fn the_match_is_case_sensitive() {
    assert_eq!(with(&["ids"]).correct("IDs"), "Ids");
    assert_eq!(with(&["IDS"]).correct("IDs"), "Ids");
}

#[test]
fn other_rules_and_switches_are_unchanged() {
    let a = with(&["IDs"]);
    for kept in ["Ids", "IDS", "ID"] {
        assert_eq!(a.correct(kept), kept, "{kept}");
    }
    assert_eq!(a.correct("monday IDs"), "Monday IDs");
    assert_eq!(a.correct("(c) IDs"), "© IDs");
    let off = AutoCorrect { two_initial_caps: false, ..with(&["IDs"]) };
    assert_eq!(off.correct("COlumn IDs"), "COlumn IDs");
}
