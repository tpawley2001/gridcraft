//! Command-equivalent tokens require a macro sheet, while defined names keep decoding them.

use super::*;

#[test]
fn command_tokens_respect_sheet_kind_in_each_grid_and_token_class() {
    let links = Links::default();
    for grid in [Grid::Biff5, Grid::Biff8, Grid::Biff12] {
        for token in [0x22, 0x42, 0x62] {
            let rgce = [token, 0, 0xA3, 0x80]; // WINDOW.RESTORE, no arguments.
            for mode in [Mode::Cell, Mode::Shared] {
                assert_eq!(
                    decode_in(&rgce, &[], CellRef::new(0, 0), mode, &links, grid, Site::default(), None),
                    Err(Unsupported("macro sheet commands"))
                );
                let expr = decode_in(&rgce, &[], CellRef::new(0, 0), mode, &links, grid, Site { commands: true, ..Site::default() }, None).unwrap();
                assert_eq!(gridcraft_formula::print(&expr), "WINDOW.RESTORE()");
            }
            let expr = decode_in(&rgce, &[], CellRef::new(0, 0), Mode::Name, &links, grid, Site::default(), None).unwrap();
            assert_eq!(gridcraft_formula::print(&expr), "WINDOW.RESTORE()");
        }
    }
}
