//! The product's names, About text, links and flavour copy, in one place.
//!
//! Plunder Ledger is the spreadsheet of the Jolly Roger Office suite, a fork of GridCraft. Every
//! user-visible product name, credit and link comes from here, so a rename touches one file. The
//! app icon is drawn in code ([`icon`]), not loaded from an image file.
//!
//! Flavour strings (the start-page greeting, empty states, the About tagline) follow the
//! `pirate_speak` preference; menus, commands, formulas, dialogs and errors stay plain English
//! whatever it says.

pub mod icon;

pub use gridcraft_engine::core::brand::{APP_ID, APP_NAME, DEFAULT_USER, PUBLISHER, SUITE_NAME};

/// The settings folder's name: `~/.config/<this>` on Linux and BSD.
pub const CONFIG_DIR: &str = "plunder-ledger";
/// The settings folder's name on macOS (Application Support) and Windows (%APPDATA%).
pub const CONFIG_DIR_TITLE: &str = "Plunder Ledger";
/// One line for the About box and the product information page.
pub const DESCRIPTION: &str = "The spreadsheet of the Jolly Roger Office suite.";
/// The upstream credit the licence asks us to keep visible.
pub const BASED_ON: &str = "Based on GridCraft by the ArtCraft team (MIT OR Apache-2.0).";
/// The fork's copyright line (upstream's own notices stay in LICENSE-MIT, LICENSE-APACHE and NOTICE).
pub const COPYRIGHT: &str = "Copyright (c) 2026 Hillyard Tech. Licensed MIT OR Apache-2.0.";
/// The publisher's site, the one link the About box shows.
pub const HOMEPAGE: &str = "https://hillyard-tech.com";
/// Accent colour of the brand (brass), as RGB.
pub const ACCENT_RGB: [u8; 3] = [0xB8, 0x86, 0x0B];

/// The ribbon's two-line About button (Automate ▸ About).
pub const ABOUT_BUTTON: &str = "About\nPlunder Ledger";

/// "About Plunder Ledger".
pub fn about_title() -> String {
    format!("About {APP_NAME}")
}

/// A flavour string: the pirate line when `pirate_speak` is on, the plain one otherwise.
pub fn flavour(pirate_speak: bool, pirate: &'static str, plain: &'static str) -> &'static str {
    if pirate_speak { pirate } else { plain }
}

/// The start page's greeting.
pub fn greeting(pirate_speak: bool) -> &'static str {
    flavour(pirate_speak, "Ahoy, Captain!", "Welcome to Plunder Ledger")
}

/// The line under the greeting.
pub fn greeting_sub(pirate_speak: bool) -> &'static str {
    flavour(pirate_speak, "Chart yer numbers and count yer plunder.", "Create a new workbook or open a recent one.")
}

/// The start page's recent list when it is empty.
pub fn no_recent(pirate_speak: bool) -> &'static str {
    flavour(pirate_speak, "No ledgers in the hold yet. Open a workbook to stow it here.", "No recent workbooks yet.")
}

/// The About box's tagline.
pub fn tagline(pirate_speak: bool) -> &'static str {
    flavour(pirate_speak, "Keep a true count of every doubloon.", "A fast, open-source spreadsheet.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_carry_no_upstream_brand() {
        for s in [APP_NAME, SUITE_NAME, APP_ID, DESCRIPTION, HOMEPAGE, COPYRIGHT] {
            let l = s.to_lowercase();
            assert!(!l.contains("artcraft") && !l.contains("gridcraft") && !l.contains("storyteller"), "{s}");
        }
        assert!(BASED_ON.contains("GridCraft") && BASED_ON.contains("MIT OR Apache-2.0"));
    }

    #[test]
    fn flavour_follows_the_setting() {
        assert_eq!(greeting(true), "Ahoy, Captain!");
        assert!(!greeting(false).contains("Ahoy"));
        for on in [true, false] {
            assert!(!greeting_sub(on).is_empty() && !no_recent(on).is_empty() && !tagline(on).is_empty());
        }
    }
}
