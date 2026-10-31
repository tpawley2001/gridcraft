//! Windows only: embed the app icon and version info (VERSIONINFO) into `plunder-ledger.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `GRIDCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/plunder-ledger.ico");
    println!("cargo:rerun-if-env-changed=GRIDCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/plunder-ledger.ico")
        .set("ProductName", "Plunder Ledger")
        .set("FileDescription", "Plunder Ledger spreadsheet")
        .set("CompanyName", "Hillyard Tech")
        .set("LegalCopyright", "Copyright (c) 2026 Hillyard Tech. Based on GridCraft by the ArtCraft team. MIT OR Apache-2.0.")
        .set("OriginalFilename", "plunder-ledger.exe")
        .set("InternalName", "plunder-ledger");
    if let Err(e) = res.compile() {
        if std::env::var_os("GRIDCRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=plunder-ledger.exe built without icon/version resources: {e}");
    }
}
