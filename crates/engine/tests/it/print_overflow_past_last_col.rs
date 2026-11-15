//! Text that runs past the last used column widens the printed range, as Excel prints it.
//! Letter portrait, 0.5" margins, header "&A p&P", Calibri 11, default widths; row 120 is on
//! the 3rd page down. Replace these stubs with the cases in the task spec.

#[test]
#[ignore = "stub: base (1 in A1, L1, A120) prints 4 pages"]
fn base_sheet_prints_four_pages() {}

#[test]
#[ignore = "stub: base + K120 = 200 chars left-aligned prints 9 pages"]
fn long_text_in_last_band_adds_its_bands() {}

#[test]
#[ignore = "stub: base + A120 = 60 chars (ends inside band 1) prints 4 pages"]
fn short_overflow_adds_nothing() {}

#[test]
#[ignore = "stub: base + K120 = \"x\" + 150 spaces prints 6 pages"]
fn trailing_spaces_overflow() {}

#[test]
#[ignore = "stub: 1 in A1, L1, W1; K120 = 40 chars prints 7 pages"]
fn overflow_inside_used_bands() {}

#[test]
#[ignore = "stub: 1 in A1 only; A120 = 400 chars prints 12 pages (blank pages of the widened range included)"]
fn widened_range_prints_blank_pages() {}

#[test]
#[ignore = "stub: base + K120 = 200 chars centred prints 6 pages"]
fn centred_overflow_spreads_both_ways() {}

#[test]
#[ignore = "stub: long text in a hidden row / zero-height row / hidden column widens nothing"]
fn hidden_text_does_not_widen() {}

#[test]
#[ignore = "stub: SimSun 11 CJK text ending inside the band adds no band"]
fn cjk_text_uses_drawn_width() {}

#[test]
#[ignore = "stub: one 2,000-char cell in H adds only the bands its drawn text reaches"]
fn one_cell_adds_only_reached_bands() {}

#[test]
#[ignore = "stub: wrapped cells, cells with a non-empty right neighbour and merged cells never widen"]
fn non_overflowing_cells_do_not_widen() {}
