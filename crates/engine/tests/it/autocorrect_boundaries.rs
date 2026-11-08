//! Text boundaries, Unicode casing and replacement outputs.
use gridcraft_engine::autocorrect::AutoCorrect;

#[test]
fn separators_and_unicode_letters_are_preserved() {
    let a = AutoCorrect::default();
    assert_eq!(a.correct("\tCOlumn,monday!\n  jOHN—sunday"), "\tColumn,Monday!\n  John—Sunday");
    assert_eq!(a.correct("ÉCole öSTERREICH"), "École Österreich");
    assert_eq!(a.correct("xMondayx"), "xMondayx");
}

#[test]
fn custom_replacements_are_case_sensitive_final_and_whitespace_delimited() {
    let a = AutoCorrect { entries: vec![("typo".into(), "COlumn monday".into())], ..Default::default() };
    assert_eq!(a.correct("typo\tTypo typo!\n typo"), "COlumn monday\tTypo typo!\n COlumn monday");
}

#[test]
fn parser_values_are_not_rewritten() {
    let a = AutoCorrect { entries: vec![("true".into(), "changed".into()), ("4-Mar-2026".into(), "changed".into())], ..Default::default() };
    for value in ["true", "4-Mar-2026", "10:30 PM", "1,000", "50%", "FALSE", "#VALUE!"] {
        assert_eq!(a.correct(value), value);
    }
}
