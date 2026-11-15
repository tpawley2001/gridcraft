# Target-app parity: GridCraft vs Microsoft Excel

> **Last reviewed:** 2026-10-11 · **Last updated:** 2026-11-15 · **Change:** minor (G0403: Liberation Sans, generic Sans names, Helvetica variants, Geneva, Clear Sans and Droid Sans Fallback use Arial's printed digit width; Cambria uses the same measured width. Earlier: G0402: MS Sans Serif and the Arial charset names print columns at Arial's digit width, SimSun and MS (P)Gothic at half an em; earlier: pull of #3872: page breaks: columns paginate at Excel's printed width and fit-to-page scales are whole percents; earlier: G0389: cells holding only formatting extend the printed range only when they show a fill or a border, and cells in hidden rows do not, as Excel prints; no score change; earlier: G0386: `.xls` rows also fit the font their empty cells show (the row format, else the column formats), as Excel re-fits them; no score change; earlier: G0382: `.xls` rows that are not custom-height print at the default row height of the largest font in them, even below the Normal font's default, as Excel re-fits them on open; no score change; earlier: G0381: rows print at Excel's printed height for 13 more Normal fonts, including localized East Asian font names; no score change; earlier: G0380: `.xlsx` rows stored without a custom height print at the default row height of the largest font in them, as Excel prints; no score change; earlier: G0379: rows print at Excel's printed height for the workbook's Normal font (600-dpi row pitch), so long sheets print Excel's page count; `.xls` files without margin records print with Excel's old default margins; no score change; earlier: G0378: text running on past a column wider than the printable page continues across the pages of the columns printed after it, not on pages of its own, as Excel prints; no score change; earlier: G0377: centred text in a column wider than the printable page is centred on the cut and continues past it on the following pages, as Excel prints; no score change; earlier: G0376: `.xls` picture objects whose image is not imported (embedded-object previews, ActiveX controls) extend the printed range, as Excel prints; no score change; earlier: G0375: a column wider than the printable page is cut to the page width; only text longer than it continues on the following pages, as Excel prints; G0374: embedded (OLE) objects and ActiveX controls in `.xlsx` files extend the printed range, as Excel prints; no score change; earlier: G0373: blank pages before the last page with content are printed, as Excel prints them; no score change; earlier: G0371: a page holding only overflowing text or part of a table prints, as Excel prints; no score change; earlier: G0370: without a print area, the empty pages below a column band's last cell are left out, as Excel prints; no score change; earlier: G0366: a column wider than the printable page prints across pages; no score change; earlier: RTD answers `#N/A` instead of `#NAME?`: functions by name 514 / 523; earlier: G0340: tables using a custom table style are drawn in it; no score change; earlier: G0278: form controls with Print object on print with the sheet and extend the printed range; no score change; earlier: G0218: Review row: Check Accessibility and opening encrypted files are in, saving with a password is missing; ToolPak tools that draw charts; no score change)

This is the authoritative parity assessment. [`ROADMAP.md`](../ROADMAP.md) summarizes it,
[`gaps.md`](gaps.md) lists every shortfall, and the area checklists hold the detail:
[functions](function-parity.md), [charts](chart-parity.md), [file formats](file-format-parity.md),
[UI](ui-parity.md), [hardware](hardware-parity.md), [localization](localization-parity.md). The
generated ribbon checklist is [`parity-checklist.md`](parity-checklist.md).

## Summary

| Number | Value | Kind |
|---|---|---|
| Feature breadth (weighted by use) | **~75%** | estimated from the area table below; anchored on two measurements |
| Ribbon and menu catalog | **285 / 300 (95.0%)** | measured: `cargo xtask parity` (`crates/engine/src/catalog.rs` ∩ registered command ids) |
| Worksheet functions, by name | **514 / 523 (98.3%)** | measured: registered functions vs Microsoft's published function list (method below) |
| Worksheet functions that do real work | **504 / 523 (96.4%)** | measured: excludes 10 stubs (7 CUBE functions, RTD, WEBSERVICE, FILTERXML) |
| **Ready for real work** | **~50%** | estimated; written weights below (51.6 computed) |
| **Mainstream practitioner** | **~50%** | estimated; [method](#mainstream-practitioner) (69.5 depth × 0.90 × 0.92 × 0.88 = 50.6) |
| **Essentials user** | **~63%** | estimated; [method](#essentials-user) (80.2 depth × 0.92 × 0.95 × 0.90 = 63.1) |
| Stage | **alpha** | see [Stage](#stage) |
| Remaining to beta | **~160–240 Opus 5.5 agent-hours** | estimated; [roadmap.md](roadmap.md#beta-gates) |
| Remaining to full parity | **~700–1,100 Opus 5.5 agent-hours** | estimated; sum of the rows below |

### Readiness by audience

| Audience | Ready % | Opus 5.5 agent wall-clock hours to ~95% | Work that dominates |
|---|---|---|---|
| Full target (ready for real work) | ~50% | ~650–1,050 | Get & Transform and the data model, localization (twelve languages), charts depth, file fidelity (`.xls`, corpus, pass-through), PivotTables, collaboration, mobile |
| Mainstream practitioner | ~50% | ~200–310 | File exchange with Excel users (corpus and Excel verification, pass-through, large files, `.xls`, encryption: 63–97 h), common-chart depth (30–45 h), PivotTables (20–30 h), keyboard and interaction (18–28 h), calc correctness and functions (20–33 h) |
| Essentials user | ~63% | ~60–100 | Opening files people send (`.xls` import and saving with a password 24–36 h; macOS Finder open 2–4 h), launch stability (6–10 h), basic chart polish (5–8 h), print and Page Layout (8–12 h), small editing and UI fixes (10–16 h) |

Each tier is a subset of the one above, so its hours are too. Calibrated as in [Effort and
calibration](#effort-and-calibration) (~6–10 h per readiness point for the remaining, verification-heavy
work; single features measured from PRs #66, #100, #101). Parallelism: the full and mainstream
work splits ~65–70% across 4–6 agents by area; the essentials work is mostly small fixes plus one
long `.xls` track, so about half parallelizes (≈30–50 wall-clock hours with three agents).

## What we measured against, and how

- **Target:** Microsoft Excel (Microsoft 365 channel). Version measured: **Excel for Mac
  16.113.4**, installed on the owner's Mac (version read from Spotlight metadata with `mdls`).
  Where Excel for Windows has more (Power Pivot, VBA editor depth, COM add-ins, 3D Maps), the
  Windows feature counts as part of the target.
- **Clean-room limits.** This repo's rule (AGENTS.md, "Non-negotiables") forbids reading anything
  inside the Office bundle, so the bundle's `Info.plist`, `.lproj` folders and resources were
  deliberately **not** inspected, and Excel was not launched for this pass. Excel's file formats,
  functions and features come from Microsoft's public documentation, the owner's earlier
  black-box observation (`plan/excel/observed-ui.md`, local only), and knowledge of the product.
- **Function list:** Microsoft's "Excel functions (alphabetical)" support page, fetched
  2026-10-10 and parsed (506 rows), plus 15 names the page lists under combined entries (FIND/FINDB,
  LEFT/LEFTB, LEN/LENB, MID/MIDB, REPLACE/REPLACEB, RIGHT/RIGHTB, SEARCH/SEARCHB, BETA.INV) and
  COPILOT and PY: **523 functions**. Ours: every `f!(…)` spec in `crates/functions/src/*.rs` plus
  `SPECIAL_FUNCTIONS` in `crates/calc/src/eval.rs` (evaluator special forms), minus the test-only
  `ADD2`: **505**, all of them Excel names.
- **Ribbon catalog:** `crates/engine/src/catalog.rs` (298 unique ids across 16 tabs) against the
  321 command ids registered in `crates/engine/src/cmd/*.rs`, recomputed from source by script and
  identical to the committed `parity-checklist.md`. The catalog is our own transcription of
  Excel's main ribbon tabs; it leaves out most contextual tabs (Picture Format, Shape Format,
  Header & Footer, Sparkline, Slicer, Timeline), most of the Developer tab (its form controls are
  in), the Help tab and sub-menu items, so
  89% of it is roughly **60–65% of Excel's full command surface** (estimated).
- **Depth and readiness:** source reading (the model types in `crates/model/src/features.rs`,
  readers and writers in `crates/xlsx/src`, the evaluator), 478 `#[test]` functions, and the
  issue tracker: 60 open issues and 50 open PRs on 2026-10-10, 82 issues filed in total.
- No cargo build or test ran for this pass (disk constraints); numbers come from source.

## Weights

Percentages are weighted by what Excel users actually do, not by item count. Feature-area
weights (sum 100) reflect a typical business, finance and analyst workload: formulas, functions,
editing and formatting dominate; Power Query, VBA and the data model matter to fewer users but
are decisive for those who use them.

Dimension weights for **ready for real work**: features 45, file formats 15, UI/UX 12,
performance 8, stability 8, localization 4, hardware 2, platforms 2, ecosystem 2, AI 2.

## Feature areas

| Area | Weight | Breadth | Ready | Hours | Evidence |
|---|---|---|---|---|---|
| Formula language and calculation | 18 | ~92% | ~72% | 20–30 | A1/R1C1 parser, reference adjustment, dependency-graph recalc, dynamic arrays and `#SPILL!`, `A1#`, LET/LAMBDA and helpers, structured references, names, 3-D references. Single-threaded |
| Worksheet functions | 12 | 98.3% (measured) | ~82% | 16–26 | 514 / 523 names, 504 working. Missing: GETPIVOTDATA, IMAGE plus cloud functions. No conformance suite against Excel-computed oracles. [function-parity.md](function-parity.md) |
| Editing, navigation, clipboard, fill | 12 | ~90% | ~70% | 15–25 | Point mode, F4, AutoComplete, Paste Special, AutoFill, Flash Fill, Find/Replace, Go To Special. [ui-parity.md](ui-parity.md) |
| Cell formatting, styles, conditional formatting | 10 | ~95% | ~80% | 8–14 | Full number-format language, borders, 47 cell styles, 60 table styles plus a workbook's custom table styles, all CF rule kinds including data bars, colour scales and icon sets. Gaps: font weights beyond regular/bold (#168), themes |
| Sort, filter, tables, validation, outline | 8 | ~82% | ~70% | 10–16 | Multi-level sort, AutoFilter, Advanced Filter, tables with totals, validation, outline, subtotals, consolidate. Advanced Filter copy-to preserves cells outside its result and rejects worksheet overflow (#1031). No slicers on tables |
| PivotTables | 7 | ~65% | ~45% | 25–40 | Insert/Recommended, field list, three layouts, values on rows or columns, date grouping, show-values-as, refresh, calculated fields, XLSX round-trip. Missing: slicers, timelines, calculated items, PivotCharts, value/label filters, number grouping, GETPIVOTDATA, data-model pivots. Round-trip never checked in Excel |
| Charts and sparklines | 8 | ~50% | ~30% | 40–60 | 25 chart kinds, all 2-D. Doughnut charts draw every series as a concentric ring (first series innermost), with editable hole size and first slice angle. The chart model holds title, legend position, a data-labels flag, gridlines flag, axis titles and chart data tables (legend keys, borders, XLSX round-trip and print/PDF); no axis scaling, error bars, per-point formatting, chart sheets, 3-D, surface, map, Pareto. Kinds survive XLSX round trip (#158, #161), except stock charts: only 3–4 series are a stock chart, and any other count is written and reimported as a line chart. [chart-parity.md](chart-parity.md) |
| Page layout, print, PDF | 5 | ~80% | ~55% | 12–20 | Page setup, print titles, breaks, page order, Page Break Preview, headers/footers, PDF export and printing. Pagination sizes columns as Excel prints them (the default font's digit width at 600 dpi; MS Sans Serif and the Arial charset names at Arial's, as Excel prints them in Arial; Liberation Sans, generic Sans names, Helvetica variants, Geneva, Clear Sans, Droid Sans Fallback and measured Cambria also use Arial's width) and fits to pages in whole percents, as Excel does; the drawn page keeps on-screen column proportions. Freeform shapes and rounded rectangles print with their outlines. Form controls with Print object on print with the sheet and extend the printed range; embedded (OLE) objects and ActiveX controls read from `.xlsx` extend it too (they are not drawn yet), as do `.xls` picture objects whose image is not imported (EMF/WMF previews, ActiveX controls). A column wider than the printable page prints alone on its page, cut to the printable width; text running on past the cut continues across the pages of the columns printed after it, shifted left by one printable width per page, and gets pages of its own only past the last column (centred text is centred on the cut), as Excel prints it. Rows print at Excel's printed height: on the printer's 600-dpi grid, a whole number of dots that depends on the workbook's Normal font (Calibri 11: 47 default rows on a Letter page, not 45; measured for 38 Normal fonts, including the localized East Asian names such as ＭＳ Ｐゴシック, 宋体 and 游ゴシック); `.xlsx` rows stored without a custom height print at their fitted height (the default row height of the largest font in them, never below the sheet's default); `.xls` rows without fUnsynced print at the default row height of the largest font in them with no floor (empty rows keep the stored or sheet default height); once a cell counts, the font its empty cells show counts too (the row's own format, else the column format of each column with no cell in the row), as Excel shows them, and `.xls` sheets without margin records print with Excel's old defaults (0.75 in sides, 1 in top and bottom). Without a print area, the printed range ends at the last cell holding a value or formula, a fill or a border, or a merged range (a merge over whole columns or rows reaches only across or down; a sheet where nothing counts prints its stored cells, as before; a cell holding only a font, number format or alignment, and any cell in a hidden row, add no pages; text overflowing past that still prints up to the last stored column), as Excel prints; printing stops after the last page with anything on it and blank pages before it are printed, as Excel does; a page holding only text overflowing from the columns before it, or part of a table, still prints. Paper sizes match Japanese Excel: B4/B5/B6 are the JIS sizes, ISO B4/B5/B6 stay reachable by name, and Japanese Postcard (はがき), Japanese Double Postcard (往復はがき), the chou/kaku envelopes and A6 are available. No true Page Layout view; the PDF writer uses only the 14 standard fonts without embedding (no CJK or other non-Latin text in PDF); web print only exports (#141, #143) |
| What-if and analysis | 4 | ~50% | ~40% | 22–35 | Goal Seek, Scenario Manager, data tables, Analysis ToolPak (all 19 tools; Histogram, Moving Average and Exponential Smoothing chart output and the Regression Residual, Line Fit and Normal Probability plots), Solver (Simplex LP, a local nonlinear method and Evolutionary; Answer, Sensitivity, Limits and Evolutionary Population reports; Load/Save Model, Reset All and Solver Options), Forecast Sheet. Missing: Analyze Data |
| Get & Transform, external data, data model | 5 | ~15% | ~10% | 60–100 | Import of CSV/TSV, JSON, HTML. No Power Query editor, no From Web, database or folder sources, no connections or refresh, no relationships or data model, no external workbook links (dropped with a warning) |
| Insert objects | 3 | ~55% | ~45% | 25–40 | Pictures, shapes (over 20 Excel presets draw their own outline; Excel has ~170), text boxes, WordArt (10 own styles; no warps or gradient fills), own icon set, ink with Ink to Shape, hyperlinks, checkboxes, form buttons, scroll bars, spin buttons, check boxes and option buttons, equations (LaTeX with a live preview, Office Math in xlsx, Symbol font in PDF). Missing: SmartArt, signature line, objects, 3-D models, screenshot, pictures in cells (PR #105) |
| Review, comments, protection, collaboration | 4 | ~55% | ~40% | 35–55 | Threaded comments, notes, spelling (system dictionary), thesaurus, sheet and workbook protection, Allow Edit Ranges with Excel-compatible password hashes (#307), Check Accessibility, opening password-encrypted `.xlsx` (Agile and Standard) and `.xls` files with their password. Missing: Show Changes, co-authoring, saving with a password to open, RC4-encrypted `.xlsx` |
| Automation | 4 | ~30% | ~20% | 12–20 | Recorded, editable GridCraft scripts (Automate tab). Typed macros in the engine (`macro.*`: recorder, runner with input checks and a footprint dry run, debugger; [macros.md](macros.md)), recorded from Automate › Macros, edited and debugged in a step editor, and run from the Macros dialog, a Ctrl+Shift shortcut or a form button (which can also run one command). VBA is out of scope by owner decision: an `.xlsm` keeps its VBA project unchanged on save and never runs it. No Office Scripts compatibility |
| **Weighted** | 100 | **~75%** | **~59%** | **300–485** | |

## Dimensions

| Dimension | Weight | Ready | Hours | Evidence |
|---|---|---|---|---|
| Features | 45 | ~59% | 300–485 | Table above |
| File formats | 15 | ~50% | 80–120 | XLSX read/write is broad but unproven on a real corpus and in Excel; `.xls` (BIFF5/8) and `.xlsb` import formulas, formatting, names, tables and more; XML Spreadsheet 2003 imports; SYLK and DIF read and write; ODS is values only; no `.prn`, XLSB or ODS write; VBA dropped; 100 MB XLSX fails (#175). [file-format-parity.md](file-format-parity.md) |
| UI/UX fidelity | 12 | ~55% | 50–80 | Excel-style ribbon, galleries, dialogs, formula bar, status bar; 59 commands with shortcuts vs Excel's 200+; key tips on part of the Home tab only; short context menu; a native window per workbook window on the desktop (one on the web). [ui-parity.md](ui-parity.md) |
| Performance | 8 | ~35% | 35–55 | [Performance](#performance) |
| Stability | 8 | ~60% | 15–25 | Never-crash rules enforced by lint, every command fuzzed with hostile params, random command sequences fuzzed against undo/redo, XLSX round-trip and full-recalc invariants, last-resort guard in `Session::execute`. Field reports: Windows Intel GPU crash (#120), error on close (#117), freeze opening from Finder (#166), screen flashing (#76) |
| Localization | 4 | ~10% | 100–165 | Ribbon-only catalog in five languages (ja, zh, ko, pt-BR, ru; ~12% of strings each), CJK/Thai/Hebrew font fallback, no shaping or RTL. [localization-parity.md](localization-parity.md) |
| Hardware | 2 | ~40% | 25–40 | GPU UI via wgpu; single-threaded calc; no pen pressure; a native window per workbook window, Arrange All on one monitor. [hardware-parity.md](hardware-parity.md) |
| Platforms | 2 | ~80% | 30–60 | [Platforms](#platforms) |
| Ecosystem / add-ins | 2 | ~10% | 25–40 | [Ecosystem and AI](#ecosystem-and-ai) |
| AI features | 2 | ~15% | 30–50 | [Ecosystem and AI](#ecosystem-and-ai) |
| **Ready for real work** | 100 | **~50%** (51.6 computed: 0.45 × 59.0 features + the other rows × their weights) | **~700–1,100** | |

How the full number is built: features ready is the weighted mean of the area table (59.0 with
charts at 30%); ready for real work is the weighted mean of this table with the weights written
under [Weights](#weights). Re-checked after the 10-10 merges: charts were corrected from 38% to
30% (round-trip loss, #190 chart resize) and localization rose from 5% to 10% (#31, #104, #82,
#38). The two moves cancel: 51.6 before and after, so **~50%** stands.

## Mainstream practitioner

The typical professional Excel user: an analyst, accountant or manager who builds and maintains
models, cleans and summarizes data, makes charts and pivots, and exchanges workbooks with Excel
users every week. Left out: Power Query and the data model, VBA and add-ins, Copilot and cloud
functions, co-authoring and admin, specialist hardware, and languages beyond the user's own
(localization is reported separately).

| Area (weekly use) | Weight | Depth | Basis |
|---|---|---|---|
| Formula language and calculation | 20 | 72 | Feature-area table |
| Common worksheet functions | 15 | 85 | Missing functions are niche (ETS, odd-period bonds, cloud) |
| Editing, navigation, clipboard, fill | 15 | 70 | Feature-area table |
| Cell formatting, styles, conditional formatting | 13 | 80 | Feature-area table |
| Sort, filter, tables, validation | 10 | 70 | Feature-area table |
| PivotTables (core: fields, layouts, grouping, refresh) | 8 | 60 | Slicers and calculated fields excluded as less frequent |
| Charts (column, bar, line, pie, scatter, combo) | 9 | 45 | Common kinds work and survive save; trendlines read, written and drawn; no axis options; moved and resized from eight handles (#190) |
| Page layout, print, PDF | 5 | 60 | Latin text exports |
| Comments, notes, protection | 3 | 55 | Feature-area table |
| Pictures and shapes | 2 | 45 | Feature-area table |
| **Average depth** | 100 | **69.5** | |

Discounts for what still stops real work:

| Discount | Factor | Evidence |
|---|---|---|
| Interaction fidelity | × 0.90 | 59 command shortcuts vs Excel's 200+ |
| Stability on real machines | × 0.92 | Never-crash lints, fuzzed commands and the `Session::execute` guard; but field reports of a startup crash on Intel UHD (#120), an error on close (#117), a freeze opening from Finder (#166) and screen flashing (#76) |
| File exchange with Excel users | × 0.88 | Excel opening our output is unverified across features; unmodelled parts dropped on save; advanced chart kinds lose their type; 100 MB files fail (#175); `.xls` and encrypted files are refused with a clear message (#17) rather than opened. Harsher than a drawing app's discount because spreadsheets are exchanged constantly |

**Mainstream practitioner: 69.5 × 0.90 × 0.92 × 0.88 = 50.6 → ~50%** (estimated). Unlike most
apps it is not higher than the full number: GridCraft's breadth is wide (functions, formatting,
ribbon), so excluding the long tail removes little, while the discounts land on exactly what
mainstream users do every week (exchange files with Excel users).

## Essentials user

Someone who only touches the core: opens a workbook someone sent, types data and simple formulas,
formats, sorts, makes a basic chart, saves and prints. Advanced options, pro workflows and exchange
edge cases are left out, along with everything excluded from the mainstream number.

| Core feature | Weight | Depth | Basis |
|---|---|---|---|
| Typing data, simple formulas, AutoSum (SUM, AVERAGE, arithmetic) | 20 | 88 | Solid |
| Basic formatting: fonts, bold, fills, borders, currency/percent/date formats | 15 | 85 | |
| Copy, paste, fill handle, undo/redo | 15 | 78 | No open bugs listed |
| Rows and columns: insert, delete, widths, AutoFit | 10 | 82 | AutoFit fixed (#42, #88); rows auto-fit oversized fonts on open and row formatting (#833) |
| Sort and filter | 10 | 82 | |
| Save, Save As, reopen XLSX | 10 | 85 | |
| A basic chart | 8 | 55 | Created easily; moved and resized from eight handles (#190) |
| Print and PDF | 7 | 65 | Desktop works; web PDF gives no file or feedback (#141) |
| Find and replace | 5 | 85 | |
| **Average depth** | 100 | **80.2** | |

| Discount | Factor | Evidence |
|---|---|---|
| Launch and stability | × 0.92 | Startup crash on some Intel GPUs (#120), "Issue executing" (#59), error on close (#117) |
| Discoverability and UI clarity | × 0.95 | The Excel-style ribbon is familiar and labelled; system language by default (#31); small gaps (#170 macOS window controls) |
| Opening files people send them | × 0.90 | Double-clicking a workbook in Finder freezes or errors on macOS (#166, #69; drag-and-drop works; fixes in PRs #164, #167); `.xls` and encrypted files are refused with a message |

**Essentials user: 80.2 × 0.92 × 0.95 × 0.90 = 63.1 → ~63%** (estimated).

## User evidence

Counted from all 83 GitHub issues and 109 PRs on 2026-10-10; none of the issues were filed by
the maintainer.

- **Praise:** 3 explicit ("this is brilliant. Tiny fast native binaries", #16; "Thanks for the
  great work on GridCraft", #61; "i didnt have excel kind app on my macos", #69), plus polite
  thanks in feature requests (#39, #45, #142).
- **"Switched from Excel" reports:** 0. One counter-signal: "without macro support most
  professionals cannot switch from excel" (#6).
- **Issues by kind:** 50 core-path bugs (30 open, 20 closed) and 33 niche or feature requests
  (25 open, 8 closed). 18 of the core-path bugs came from one auditing contributor (coygeek).
- **Contributors:** 104 PRs from 28 people outside the maintainer in five days, which shows
  strong interest but not yet daily use.

## Performance

Excel recalculates on every core (multi-threaded recalculation), streams large files, and handles
1,048,576 × 16,384 sheets with millions of formulas.

| Measure | GridCraft | Excel | Kind |
|---|---|---|---|
| Sheet size limits | 1,048,576 × 16,384 | same | measured (`crates/core/src/addr.rs`) |
| Recalc threads | 1 | all cores | measured (no threading in `crates/calc`) |
| Edit → recalc, 100k dependents | ~0.4 s (2026-10-07), target < 100 ms | tens of ms | measured on the owner's machine; #66 since made range-formula recalc linear, not re-timed |
| Grid scrolling | virtualised, O(log n) geometry with custom row heights | smooth | measured in code |
| Open a ~100 MB XLSX | fails (#175) | opens | user report |
| Whole-sheet operations | Fill and Remove Duplicates on a whole-sheet selection run out of memory or time (fix in PR #152) | fine | open PR |

Work: streaming XLSX reader with shared-string and style dedup, multi-threaded recalc of
independent chains (native only; WASM stays single-threaded), whole-column reference
clamping, a `cargo xtask bench` with fixed workbooks so the numbers become measured.

## Platforms

| Platform | Excel | GridCraft |
|---|---|---|
| Windows x64 / arm64 | yes | yes (signed MSI and portable zip; also x86) |
| macOS (Apple silicon, Intel) | yes | yes (signed, notarized universal DMG) |
| Web | Excel for the web | yes (WASM; local files, no cloud) |
| iOS / iPadOS / Android | yes | no |
| Linux | no | yes (AppImage, Flatpak, deb, rpm, tarball; x86_64, aarch64, riscv64) |
| FreeBSD | no | yes |

Mobile is the gap (30–60 h for a touch layout over the same engine). GridCraft is ahead on
Linux, BSD and RISC-V, where Excel doesn't run.

## Ecosystem and AI

- **Add-ins:** Excel has Office JavaScript add-ins, COM/XLL add-ins, VBA, Office Scripts and
  Power Automate. GridCraft has its own recorded scripts, plus a control channel, MCP server and
  CLI that reach every command. VBA is out of scope (owner decision). A plugin or custom-function
  API (for example LAMBDA libraries and WASM custom functions) is the open-equivalent path: 25–40 h.
- **AI:** Excel has Copilot, the COPILOT() function, Analyze Data, Python in Excel and Flash
  Fill. GridCraft has Flash Fill and best-in-class agent control (any MCP client can drive every
  command headless or live), but no in-app assistant, no natural-language formulas and no
  Analyze Data. Local-model equivalents: 30–50 h; model choice needs the owner.

## Stage

**Alpha.** All six core workflows in the [alpha gate](roadmap.md#alpha-gate) pass end to end on macOS with save and reopen (two pass with non-blocking fidelity loss). Core workflows exist end to end, but depth, fidelity and file compatibility are rough
(ready ~50%, inside the 40–75% alpha band). Not beta: the main format, XLSX, has not been proven
on real-world files or confirmed to open in Excel without repair, large files fail, `.xls` is
missing and text in several scripts doesn't render. Distance to beta: ~25 points of readiness and
~160–240 h, itemized in [roadmap.md](roadmap.md#beta-gates).

## Effort and calibration

Hours are **Opus 5.5 agent wall-clock hours**, one agent working sequentially, including tests
and verification.

Calibrated against this repo's history:

- **First pass, 2026-10-05 → 10-07:** M0–M13 (about 72,000 lines of Rust: engine, 478 functions,
  XLSX, UI, charts, PivotTables, PDF, MCP, CLI, web, packaging) landed in commits from 22:09 on
  10-05 to 02:08 on 10-06, built by several parallel agents with work that preceded the first
  commits; we put it at **~50–80 agent-hours** for 0 → ~50% ready, so **~1–1.5 h per point**
  for the broad first pass.
- **Recent single features:** ODS import (#100, +2,232 lines with tests) and XLSB import (#101,
  +1,212 lines) each fit in one agent session of roughly 2–3 h; the linear-recalc fix (#66,
  +477 lines) about 1 h; single-function fixes (ROUND, DATE 1900, SUMIF sizing) 0.3–0.5 h each.
  That is roughly **600–1,000 lines per agent-hour** including tests.
- **Tail factor:** what remains is verification-heavy (Excel as the oracle for files, functions
  and rendering), and much of it is large self-contained subsystems (Power Query, Solver,
  localization, `.xls`). We apply **5–7× the first-pass rate**: ~6–10 h per point of readiness.

**Parallelism:** ~70% of the work splits cleanly by area (functions, charts, pivots, formats,
localization, Power Query) across 4–6 agents with one integrator; with five agents, beta is about
40–60 wall-clock hours away. **Needs a human:** checking our files in Excel (the owner's Mac),
native-speaker review of every language, scope decisions (co-authoring, cloud functions,
in-app AI models, mobile), and Windows hardware for GPU-driver crashes.

## What works today

The inventory carried over from the 2026-10-07 ROADMAP.md, updated for what landed since. Presence, not parity: the tables above grade depth.

- Engine: A1/R1C1 formula parser with reference adjustment, dependency-graph recalculation with
  dynamic arrays (spill + `#SPILL!`), cycle detection, LET/LAMBDA/MAP/REDUCE/SCAN/BYROW/BYCOL/
  MAKEARRAY, structured table references, defined names, INDIRECT/OFFSET/INDEX references,
  505 worksheet functions (478 at M0) (math, statistics and distributions, text and regex, lookup incl.
  XLOOKUP/FILTER/SORT/UNIQUE/VSTACK, dates, financial incl. bonds, engineering incl. complex
  numbers and Bessel, database, information).
- Number formats: Excel's full format-code language (sections, conditions, colours, dates and
  elapsed time, fractions, scientific, accounting fill), General narrowing to column width.
- Files: XLSX read/write (styles, themes, merges, CF, validation, tables, comments, hyperlinks,
  charts, pictures, sparklines, print settings, defined names, shared/array/dynamic formulas),
  CSV/TSV with encoding and delimiter detection, HTML export, JSON; XLSB and ODS data import (values only, 10-10).
- Editing: cell entry with type detection and auto-formats, in-cell and formula-bar editing,
  point mode (click/drag/arrow references), reference colouring, F4 anchor cycling, function
  autocomplete and argument hints, copy/cut/paste and Paste Special (values, formats, transpose,
  operations, link, skip blanks), AutoFill series (numbers, dates, months, weekdays, custom lists,
  text+number), Fill Series, Flash Fill, find/replace with wildcards, Go To / Go To Special,
  undo/redo with history, format painter. Quick selection drags retain their final endpoint;
  the fill handle explains how to fill versus select cells.
- Formatting: fonts, fills, all border styles, alignment (wrap, indent, rotation, merge), number
  formats, 47 cell styles, 60 table styles, conditional formatting (cell rules, text, dates,
  duplicates, top/bottom, averages, data bars, colour scales, icon sets, formulas), automatic
  row heights, autofit, hide/unhide, freeze panes.
  Border presets include visual diagrams; Format Cells previews draft borders before applying.
- Data: sort (multi-level, by colour, custom lists), AutoFilter (values, custom, top 10,
  average, colour), tables with totals rows, remove duplicates, text to columns, data validation
  with error alerts, grouping/outline, subtotals.
- PivotTables: Insert/Recommended PivotTables, field list pane, compact/outline/tabular layouts,
  values on rows or columns, date grouping, sort, filters, show-values-as, refresh, and XLSX round-trip.
- Page layout & print: Page Break Preview, print settings, PDF export and printing.
- What-if: Goal Seek, scenarios, data tables. Spelling (system word list) and thesaurus.
- Draw: pen, highlighter, eraser, ink strokes and Ink to Shape. Task panes: Comments (threaded
  replies, resolve), Watch Window, Selection, Format Chart.
- Charts: column, bar, line, area, pie, doughnut (one concentric ring per series), scatter,
  bubble, radar, histogram, waterfall, funnel, treemap, sunburst, box & whisker, stock and combo
  charts; sparklines.
- UI (egui): Excel-style title bar with Quick Access Toolbar and AutoSave, the full ribbon with
  galleries (cell styles, table styles, colour palettes, conditional formatting menus),
  contextual Table/Chart Design tabs, formula bar with Name Box, virtualised grid (smooth
  scrolling, zoom, frozen panes, text overflow), sheet tabs (rename, reorder, colour, hide),
  status bar with Sum/Count/Average and zoom, dialogs (Format Cells, Insert Function, Find and
  Replace, Sort, Name Manager, Data Validation, Paste Special…), command palette, dark mode.
- Agents: every action is an engine command; JSON control channel in the desktop app (pointer,
  keyboard, dialogs, screenshots); MCP server (headless or bridged); CLI (`info`, `convert`,
  `eval`, `cat`, `run`, `commands`, `functions`, `mcp`, `send`).
- Platforms: macOS, Windows, Linux, FreeBSD; web (WASM via trunk). Release workflows for
  signed/notarized macOS universal DMG, signed Windows x64/x86 MSI + zip, Linux AppImage/deb/rpm/
  tar.gz + Flatpak manifest, FreeBSD tarball and a web zip.

## Not measured yet

- Pixel comparison against Excel screenshots (planned; screenshots stay in `plan/excel/`).
- Function results against Excel-computed oracles (a corpus repo is the planned home).
- Recalc and open-time benchmarks on fixed workbooks (no `cargo xtask bench` yet).
- Whether Excel opens our XLSX output without repair, across features.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-15 | minor | G0403: Liberation Sans, generic Sans names, Helvetica variants, Geneva, Clear Sans and Droid Sans Fallback use Arial's printed digit width; Cambria uses the same measured width. Matching accepts one pair of surrounding quotes; unknown fonts keep the Calibri fallback. No readiness score change. |
| 2026-11-15 | minor | G0402: Page layout and print: a Normal font of MS Sans Serif or an Arial charset name (Arial Cyr, Tur, CE, Baltic, Greek) paginates columns at Arial's digit width, as Excel prints them in Arial (MS Sans Serif 10 default columns break after column I on Letter portrait); SimSun, NSimSun, MS Gothic and MS PGothic use half an em. No readiness score change. |
| 2026-11-14 | minor | Pull of storytold/gridcraft#3872: Page layout and print: page breaks and fit-to-page scales count a column at its printed width, the default font's digit width at a 600 dpi printer (Calibri 11: 5.64pt a character, not the 5.25pt of 7 screen pixels), and fit-to-page scales are whole percents rounded down, as measured in Excel 365. Default columns on Letter portrait now break after column I, not J. No readiness score change. |
| 2026-11-14 | minor | G0389: page layout, print, PDF: without a print area, a stored cell with no value or formula counts toward the printed range and its pages only when its style has a fill or a border side (font, number format, alignment and protection alone add no pages); every merged range counts, even with no stored cell; cells in hidden rows count nowhere; text overflowing past the last counted column still extends the range, no further than the last stored column; a merge over whole columns does not extend the range down, one over whole rows not right; a sheet where no cell, merge, spill range, table or drawing object counts keeps its block of stored cells (no score change) |
| 2026-11-13 | minor | G0381: page layout, print, PDF: printed row pitch measured for 13 more Normal fonts (SimSun 11, Yu Gothic 10/11, DengXian 11, PMingLiU 11/12, Trebuchet MS 11, Garamond 11, Century Gothic 9, Arial Cyr 10, Calibri 14); the localized names ＭＳ Ｐゴシック, 游ゴシック, 宋体, 等线 and 新細明體 resolve to them, also for `.xls` default row heights (no score change) |
| 2026-11-14 | minor | G0386: page layout, print, PDF: an `.xls` row without fUnsynced also counts the font of each column format (COLINFO ixfe) on a column with no cell in the row (Arial 10 cells, Arial 12 on every column, stored 15 pt: kept at 15 pt, not 12.75 pt); a row with its own format (ROW fGhostDirty + ixfe) counts that format's font instead of the column formats; rows where no cell counts (none, or only cells in merges) still keep the stored / sheet default height (no score change) |
| 2026-11-14 | minor | G0382: page layout, print, PDF: an `.xls` row without fUnsynced prints at the default row height of the largest cell font in it with no floor at the Normal default (Arial 8 cells under MS Sans Serif 10: 11.25 pt, Arial 10 rows stored at 13.55 pt: 12.75 pt); rows without cells keep the stored / sheet default height (no score change) |
| 2026-11-13 | minor | G0380: page layout, print, PDF: an `.xlsx` row without customHeight prints at the default row height of the largest cell font in it (Calibri 20 cells on a Calibri 11 sheet: 26 rows a page, not 47), with or without a stored `ht`; wrapped rows and unmeasured bigger fonts keep the stored height (no score change) |
| 2026-11-13 | minor | G0379: page layout, print, PDF: rows print at Excel's printed row height for the Normal font (whole 600-dpi dots, scaled sheets rounded per row), pages hold rows while their dots fit the printable height less 8; `.xlsx` rows without customHeight print at the fitted height; `.xls` default margins 0.75/0.75/1/1 when the records are absent, and a synced DefaultRowHeight follows the Normal font (no score change) |
| 2026-11-13 | minor | G0378: page layout, print, PDF: text running on past a column wider than the printable page is drawn across the pages of the next column bands (one printable width further left per page, stopped at the first non-empty cell); continuation pages of its own only for text reaching past the area's last column (no score change) |
| 2026-11-13 | minor | G0377: page layout, print, PDF: centred text in a column wider than the printable page is laid out centred on the cut (the printable width), not on the whole column; what runs past the cut continues on the following pages, as Excel prints it (no score change) |
| 2026-11-12 | minor | G0376: page layout, print, PDF: `.xls` picture objects whose image is not imported (an embedded object's EMF/WMF preview, an ActiveX control, a missing blip) keep their place and extend the printed range (no score change) |
| 2026-11-12 | minor | G0375: page layout, print, PDF: a column wider than the printable page is cut to the printable width (fill, borders and alignment within it) and printed alone on its page; following pages only continue text longer than the cut, and the next columns start a new page, as Excel prints. Corrects G0366, which split every such column into pieces (no score change) |
| 2026-11-12 | minor | G0374: page layout, print, PDF: without a print area, embedded (OLE) objects and ActiveX controls read from `.xlsx` extend the printed range like other objects (their pages print; the objects are not drawn yet) (no score change) |
| 2026-11-12 | minor | G0373: page layout, print, PDF: without a print area, blank pages before the last page with content are printed (in page order), as Excel prints them; only the pages after it are left out (no score change) |
| 2026-11-12 | minor | G0371: page layout, print, PDF: without a print area, a page that holds only text overflowing from the columns before it, or part of a table's range, is printed, as Excel prints (no score change) |
| 2026-11-12 | minor | G0370: page layout, print, PDF: without a print area, pages with nothing on them, below them or to their right are left out (each band of columns prints down to its last cell), as Excel prints; page numbers follow the printed pages (no score change) |
| 2026-11-12 | minor | G0366: page layout, print, PDF: a column wider than the printable page is split across pages (the next page shows the rest of the column), as Excel prints it; Print Preview lists each piece with its `columnOffset` (no score change) |
| 2026-11-12 | minor | G0340: formatting row: tables using one of the workbook's custom table styles are drawn in that style (screen, print, Convert to Range); no score change |
| 2026-11-12 | minor | G0336: Normal, Page Break Preview and Page Layout each keep their own zoom; Page Break Preview opens at 60% (no score change) |
| 2026-11-11 | minor | G0278: page layout, print, PDF: form controls with Print object on print with the sheet and extend the automatic print range; off leaves them out (no score change) |
| 2026-11-10 | minor | Rows no longer list fixed issues (#21, #28, #43, #57, #128-#130, #132-#134, #138-#140) as open bugs; system fonts list (#4), subtotal header (#131), COUNTBLANK outside the used range (#137) and the grid scroll bars (#176) were checked fixed and dropped too (no score change) |
| 2026-11-09 | minor | G0218: Review row lists Check Accessibility and opening password-encrypted `.xlsx` and `.xls` files; Missing now names saving with a password to open and RC4-encrypted `.xlsx`, as gaps.md §7 records. What-if row names the ToolPak tools that draw charts (no score change) |
| 2026-11-09 | minor | Insert objects: shapes row now says over 20 Excel presets draw their own outline; Review row drops comment navigation and notes-to-comments conversion from Missing (no score change) |
| 2026-11-09 | minor | G0213: File formats note now lists `.xls`/`.xlsb` formula and formatting import, XML Spreadsheet 2003 import and SYLK/DIF read/write, as in file-format-parity.md (no score change) |
| 2026-11-08 | minor | Several windows: View Side by Side, Synchronous Scrolling and Reset Window Position (no score change) |
| 2026-11-08 | minor | G0190: Formulas › Remove Precedent Arrows and Remove Dependent Arrows; catalog 283 / 298 → 285 / 300 |
| 2026-11-07 | minor | G0175: page layout, print, PDF: freeform shapes and rounded rectangles print with their outlines, not as boxes (no score change) |
| 2026-11-07 | minor | G0157: form check boxes and option buttons in Insert objects; catalog 281 / 296 → 283 / 298 (Developer › Controls) |
| 2026-11-06 | minor | G0141: Evolutionary Solver Population report: best value and final-generation mean, sample standard deviation, maximum and minimum for variable and constraint cells; offered in Solver Results. |
| 2026-11-02 | minor | Form controls (buttons, scroll bars, spin buttons) in Insert objects and Automation; catalog 277 / 292 → 281 / 296 (Developer › Controls) |
| 2026-11-02 | minor | Insert objects: equations; catalog count 277 / 292 (no score change) |
| 2026-11-02 | minor | What-if and analysis: Solver Sensitivity report (no score change) |
| 2026-11-02 | minor | Charts move and resize from eight handles like every floating object (#190): the Essentials chart work is polish only (5–8 h), and the chart rows and the interaction discount no longer list resizing as missing; the Mainstream chart row no longer lists trendlines as missing (no score change) |
| 2026-11-01 | minor | What-if and analysis: Analysis ToolPak has all 19 tools (was 13 of 19; no charts); no score change |
| 2026-11-01 | minor | Charts: data tables for column, bar, line, area and compatible combo charts, with legend keys and borders; live source values, native XLSX round-trip and shared print/PDF rendering. Readiness estimates unchanged. |
| 2026-11-01 | minor | Page layout and print: Page Setup's page order (Down, then over or Over, then down), kept in XLSX; no score change |
| 2026-10-31 | minor | The Insert Function dialog is named as in Excel (it was called Formula Builder); no score change |
| 2026-10-31 | minor | What-if and analysis: Solver Options (no score change) |
| 2026-10-31 | minor | What-if and analysis: Solver Load/Save Model and Reset All; Forecast Sheet moved from Missing to the list of tools, as gaps.md §13 records (no score change) |
| 2026-10-25 | minor | Automation: macro interface (Automate › Macros, the Macros and Record Macro dialogs, the step editor, shortcuts) (no score change) |
| 2026-10-25 | trivial | Catalog count 276 / 292 in the summary table and the method, as `cargo xtask parity` measures (no score change) |
| 2026-10-22 | minor | Several windows on the desktop: a native window per workbook window, New Window on the same workbook, Switch Windows and Arrange All; "one window" removed from UI/UX fidelity, Hardware and the interaction-fidelity discount; no score change |
| 2026-10-21 | minor | Page Setup paper sizes: `B4`/`B5`/`B6` are the JIS sizes Japanese Excel uses (ISO B4/B5/B6 by name), and Japanese Postcard, Japanese Double Postcard, the chou/kaku envelopes and A6 are added; PDF MediaBox and `file.print` follow, and the Size menu lists them |
| 2026-10-18 | trivial | Stale chart notes: all chart kinds survive XLSX round-trip (PRs #158, #161) and trendlines landed |
| 2026-10-16 | minor | Advanced Filter copy-to clears only its actual output and rejects overflow without changing cells (#1031) |
| 2026-10-15 | minor | Row auto-fit: line breaks count only in wrapping cells (Excel: wrap off shows one line, AutoFit gives one line); unwrapped multi-line cells from files no longer grow rows on open (no score change) |
| 2026-10-21 | minor | Automation: typed macros in the engine and `macro.*` commands, no interface yet (no score change); catalog 274 / 290 → 275 / 291 (View › Macros › Record Macro) |
| 2026-10-12 | minor | Essentials AutoFit evidence: rows auto-fit oversized fonts on open and row formatting (#833) |
| 2026-10-12 | minor | Sheet tabs: reveal the selected tab on navigation and resize while preserving manual scrolling (#681); no score change |
| 2026-10-12 | minor | PivotTables: values on rows (Σ Values in Rows at any position, all three layouts; `pivot.moveField` and the field pane move it; `dataOnRows` round-trips) (no score change) |
| 2026-10-11 | minor | Stability: command-sequence fuzz harness (`crates/engine/tests/it/command_sequence_fuzz.rs`) checks random command sequences against undo/redo, XLSX round-trip, full-recalc and structure invariants; the sequence bugs it found are fixed (no score change) |
| 2026-10-11 | minor | Password-encrypted workbooks open (#11): Essentials file-opening work 24–36 h |
| 2026-10-11 | minor | Worksheet functions 505 / 523 (96.6%; 496 working): ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD |
| 2026-10-11 | minor | Worksheet functions 503 / 523 (96.2%; 494 working): PERCENTOF, EUROCONVERT |
| 2026-10-11 | minor | Worksheet functions 502 / 523 (96.0%; 493 working): BAHTTEXT |
| 2026-10-11 | minor | Worksheet functions 502 / 523 (96.0%; 493 working): TRIMRANGE |
| 2026-10-11 | minor | View › Formula Bar and Zoom 100% are engine commands (CLI/MCP); catalog 258 → 260 / 290 (#289) |
| 2026-10-11 | minor | Review › Allow Edit Ranges; catalog 258 → 259 / 290 (#307) |
| 2026-10-11 | minor | PivotTables: calculated fields (`pivot.calculatedField`) |
| 2026-10-11 | minor | PivotTables: PivotCharts (`insert.pivotChart`) |
| 2026-10-10 | minor | Readiness-by-audience table: hours to ~95% for each of the three numbers (full 650–1,050, mainstream 200–310, essentials 60–100). Full number confirmed as the additive weighted sum over the written dimension weights (no change) |
| 2026-10-10 | minor | Added Mainstream practitioner (~50%) and Essentials user (~63%) with written weights and discounts, and user-evidence counts; re-checked the full number after the 10-10 merges (charts 38→30, localization 5→10; still 51.6, ~50%) |
| 2026-10-10 | minor | Stage checked against the new alpha gate (six core workflows, all pass): stays alpha |
| 2026-10-10 | major | Created. Full re-measure against Excel for Mac 16.113.4: functions 501 / 523 measured against Microsoft's list, catalog 258 / 290, breadth ~75%, ready ~50%, alpha; replaces the estimate tables previously in ROADMAP.md and keeps its "Working today" inventory |
