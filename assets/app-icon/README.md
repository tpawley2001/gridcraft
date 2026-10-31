# Plunder Ledger app icon

A treasure chest whose front is a ledger grid (gold header row, ruled parchment cells), with
brass straps and a lock plate, on a rounded deep-sea tile. It is drawn in code, in
`crates/ui-egui/src/brand/icon.rs`, from the Jolly Roger Office palette (navy `#1F3A4D`,
parchment `#F4E9D0`, brass `#B8860B`, gold `#D4A017`, ink `#2B1D0E`, wood `#8C6D46`). The app
draws the window icon and the in-app pictures straight from that code; the files here are
renders of it for packaging only.

## Files

| File | Use |
|---|---|
| `plunder-ledger-1024.png` | 1024 px render of the full tile |
| `plunder-ledger-macos-512.png` | The tile on Apple's grid (824/1024 body, transparent margin) |
| `plunder-ledger.icns` | macOS bundle icon (PNG entries, 16–1024 px) |
| `plunder-ledger.ico` | Windows exe icon, 16–256 px (embedded by `apps/gridcraft/build.rs`) |
| `hicolor/<size>/apps/tech.hillyard.plunderledger.png` | Linux icon theme, 16–512 px |
| `LICENSE.txt` | MIT OR Apache-2.0, Hillyard Tech |

The app ID is `tech.hillyard.plunderledger` (`brand::APP_ID`).

## Regenerate

```sh
cargo run --release -p gridcraft-ui-egui --example app_icon -- assets/app-icon
```

(`packaging/icons.sh` runs the same command.)
