//! The product's names, shared by every layer (the engine's messages, file metadata, the MCP
//! server and the UI). The UI's `brand` module re-exports these and adds the About text, links,
//! flavour copy and the code-drawn icon; nothing else spells a product name out.

/// The application's name, as window titles, messages and file metadata show it.
pub const APP_NAME: &str = "Plunder Ledger";
/// The suite this app belongs to.
pub const SUITE_NAME: &str = "Jolly Roger Office";
/// The reverse-DNS application id: Wayland app id, desktop file and icon theme names.
pub const APP_ID: &str = "tech.hillyard.plunderledger";
/// Who publishes this fork.
pub const PUBLISHER: &str = "Hillyard Tech";
/// The default author name stamped on new workbooks until the user sets their own.
pub const DEFAULT_USER: &str = "Plunder Ledger User";
