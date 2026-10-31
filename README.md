<p align="center">
  <img src="assets/app-icon/hicolor/256x256/apps/tech.hillyard.plunderledger.png" width="128" height="128" alt="Plunder Ledger icon: a treasure chest whose front is a ledger grid, on a deep-sea tile">
</p>

<h1 align="center">Plunder Ledger</h1>

<p align="center">
  <b>The spreadsheet of the Jolly Roger Office suite.</b><br>
  A fast, open-source spreadsheet written in Rust, with a parchment-and-brass pirate skin.
</p>

<p align="center">
  <i>Based on GridCraft by the ArtCraft team (MIT OR Apache-2.0).</i>
</p>

## What it is

Plunder Ledger opens, edits and saves Excel workbooks (.xlsx, .xlsm), and reads .xlsb, .xls,
.ods, .csv and .tsv. Formulas, number formats, tables, charts, pivot tables, conditional
formatting, sorting and filtering behave as in the upstream engine; the pirate theme is skin and
copy only. Menus, commands, formula names and error messages stay in plain English.

- **Parchment** (light) and **Midnight Sea** (dark) themes, under File ▸ Options ▸ Display theme.
- **Pirate speak** (File ▸ Options ▸ General) turns the flavour lines on the start page and in
  the About box on or off. It is on by default and changes nothing else.
- **Local only.** There is no self-updater, no telemetry and no account. Updates come from
  whoever built your copy.

## Quick start

```sh
cargo run --release -p gridcraft                              # desktop app
cargo run --release -p gridcraft -- --sample sales            # open a sample (sales, budget, grades)
cargo run --release -p gridcraft -- book.xlsx                 # open a workbook
cargo run --release -p gridcraft -- --sample sales --control 7979   # + JSON control channel
cargo xtask ci                                                 # fmt, clippy, tests, assets, layering, wasm
```

The desktop binary is `plunder-ledger`; crate names (`gridcraft`, `gridcraft-cli`, …) are kept
from upstream so the code stays easy to compare with it. Product names,
the About text and links live in one place: `crates/ui-egui/src/brand.rs`.

### Screenshots without a window

```sh
cargo run --release -p gridcraft-ui-egui --example snapshot -- --sample sales out.png
cargo run --release -p gridcraft-ui-egui --example snapshot -- --sample sales --dark out-dark.png
cargo run --release -p gridcraft-ui-egui --example app_icon -- assets/app-icon   # regenerate the icons
```

### Logs

The desktop app writes its log to standard error and to `gridcraft.log` in the `logs` folder
next to its settings: `~/.config/plunder-ledger/logs` on Linux and BSD
(`$XDG_CONFIG_HOME/plunder-ledger/logs` when set), `~/Library/Application Support/Plunder Ledger/logs`
on macOS and `%APPDATA%\Plunder Ledger\logs` on Windows.

## For agents: MCP, CLI and the control channel

```sh
gridcraft-cli mcp                                              # headless MCP server
gridcraft-cli mcp --connect 7979                               # MCP bridged to a running window
gridcraft-cli eval '=XLOOKUP("b",{"a","b"},{1,2})'             # → 2
gridcraft-cli run --sample budget --cmd 'home.bold={"range":"B4:F4"}' --out budget.xlsx
gridcraft-cli cat budget.xlsx --range B4:F15                   # aligned text table
gridcraft-cli commands --search chart                          # every command and its params
```

See [`docs/mcp.md`](docs/mcp.md), [`docs/cli.md`](docs/cli.md) and
[`docs/control-protocol.md`](docs/control-protocol.md).

## Architecture

An engine-first Cargo workspace with enforced layering (`cargo xtask layers`): `core`/`numfmt` →
`formula`/`functions` → `model`/`calc`/`xlsx` → `chart` → `engine` → `ui-egui`/`mcp` → `apps/*`.
The egui frontend is a separate crate, so the UI can be swapped without touching the engine.
Detail: [`docs/architecture.md`](docs/architecture.md), work list: [`docs/gaps.md`](docs/gaps.md).

## License and credits

Plunder Ledger is dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option, like the code it is based on.

- **Based on GridCraft by the ArtCraft team (MIT OR Apache-2.0).** Upstream copyright and
  required notices are kept in [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE),
  [NOTICE](NOTICE) and [ATTRIBUTION.md](ATTRIBUTION.md).
- Changes for Plunder Ledger and Jolly Roger Office: Copyright (c) 2026 Hillyard Tech, under the
  same licences.
- Plunder Ledger is not affiliated with or endorsed by ArtCraft. The ArtCraft and GridCraft names
  are used here only to credit the upstream project.
- The app icon is drawn in code (`crates/ui-egui/src/brand/icon.rs`); every asset is listed in
  [ATTRIBUTION.md](ATTRIBUTION.md).
