//! XLSB future-record (BrtFRTBegin) and alternate-content (BrtACBegin) blocks: the "skipped"
//! warning appears only when a record inside a block is actually dropped, and at most once.
use super::xlsb_sparkline_tests::{file, line_group, sheet};
use super::xlsb_tests::record;
use crate::read_xlsb;

const WARNING: &str = "future-record";

/// The sparkline group collection block, read in full.
fn sparkline_block() -> Vec<u8> {
    let mut out = record(35, &[0x0e, 0, 0, 0]);
    out.extend(record(1058, &[]));
    out.extend(line_group());
    out.extend(record(1059, &[]));
    out.extend(record(36, &[]));
    out
}

/// A future-record block holding one record the reader does not know.
fn unknown_block() -> Vec<u8> {
    let mut out = record(35, &[0x0e, 0, 0, 0]);
    out.extend(record(2000, &[]));
    out.extend(record(36, &[]));
    out
}

/// Reads a workbook whose first sheet ends with `tail`: (sparkline count, skip warnings).
fn read(tail: &[u8]) -> (usize, usize) {
    let (wb, report) = read_xlsb(&file(&sheet(tail))).unwrap();
    (wb.sheets[0].sparklines.len(), report.warnings.iter().filter(|w| w.contains(WARNING)).count())
}

#[test]
fn sparkline_block_does_not_warn() {
    assert_eq!(read(&sparkline_block()), (3, 0));
}

#[test]
fn unknown_record_in_a_block_warns_once() {
    assert_eq!(read(&unknown_block()), (0, 1));
}

#[test]
fn two_blocks_with_unknown_records_warn_once() {
    let mut tail = unknown_block();
    tail.extend(unknown_block());
    assert_eq!(read(&tail), (0, 1));
}

#[test]
fn empty_alternate_content_block_does_not_warn() {
    let mut tail = record(37, &[1, 0, 0x0e, 0, 0, 0]);
    tail.extend(record(38, &[]));
    assert_eq!(read(&tail), (0, 0));
}

#[test]
fn sparklines_and_an_unknown_record_warn_once() {
    let mut tail = sparkline_block();
    tail.extend(unknown_block());
    assert_eq!(read(&tail), (3, 1));
}
