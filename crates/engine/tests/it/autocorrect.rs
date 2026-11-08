//! AutoCorrect on typed text entries.

use gridcraft_engine::autocorrect::{AutoCorrect, default_entries};

fn on() -> AutoCorrect {
    AutoCorrect::default()
}

#[test]
fn defaults_turn_every_rule_on() {
    let a = on();
    assert!(a.two_initial_caps && a.day_names && a.caps_lock && a.replace_text);
    assert_eq!(a.entries, default_entries());
}

#[test]
fn two_initial_capitals_lower_the_second_letter() {
    let a = on();
    assert_eq!(a.correct("COlumn"), "Column");
    assert_eq!(a.correct("TWo INitial CApitals"), "Two Initial Capitals");
    for kept in ["ABC", "ID", "TO", "Hello", "MOnDAY", "iPhone"] {
        assert_eq!(a.correct(kept), kept, "{kept}");
    }
}

#[test]
fn names_of_days_are_capitalized() {
    let a = on();
    assert_eq!(a.correct("monday"), "Monday");
    assert_eq!(a.correct("see you sunday"), "see you Sunday");
    assert_eq!(a.correct("xmondayx"), "xmondayx");
}

#[test]
fn accidental_caps_lock_is_corrected() {
    let a = on();
    assert_eq!(a.correct("jOHN"), "John");
    assert_eq!(a.correct("cAPS lOCK"), "Caps Lock");
}

#[test]
fn replacement_list_matches_whole_tokens() {
    let a = on();
    assert_eq!(a.correct("copyright (c) 2026"), "copyright © 2026");
    assert_eq!(a.correct("(r)"), "®");
    assert_eq!(a.correct("Brand(tm)"), "Brand(tm)");
    assert_eq!(a.correct("(tm)"), "™");
    assert_eq!(a.correct("(e)"), "€");
    assert_eq!(a.correct("wait ..."), "wait …");
    assert_eq!(a.correct("teh cat adn dog"), "the cat and dog");
    assert_eq!(a.correct("recieve"), "receive");
}

#[test]
fn formulas_numbers_and_apostrophe_entries_are_left_alone() {
    let a = on();
    for kept in ["=teh", "+COlumn", "-monday", "'COlumn", "12.5", "TRUE", "#N/A", ""] {
        assert_eq!(a.correct(kept), kept, "{kept}");
    }
}

#[test]
fn each_flag_turns_off_only_its_rule() {
    let a = AutoCorrect { two_initial_caps: false, ..on() };
    assert_eq!(a.correct("COlumn monday"), "COlumn Monday");
    let a = AutoCorrect { day_names: false, ..on() };
    assert_eq!(a.correct("COlumn monday"), "Column monday");
    let a = AutoCorrect { caps_lock: false, ..on() };
    assert_eq!(a.correct("jOHN"), "jOHN");
    let a = AutoCorrect { replace_text: false, ..on() };
    assert_eq!(a.correct("teh (c)"), "teh (c)");
    let a = AutoCorrect { entries: Vec::new(), ..on() };
    assert_eq!(a.correct("teh"), "teh");
}
