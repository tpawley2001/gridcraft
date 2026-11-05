# Gaps

> **Last reviewed:** 2026-10-11 · **Last updated:** 2026-11-04 · **Change:** minor (XLSX web publishing preferences survive open and save; XLSX workbook image, link, display and compatibility options survive saving (G0108); XLSX worksheet defaults, inactive protection permissions and validation prompt positions survive saving (G0107); §19: XLSB cell-bound query parameters keep their connections on Save As `.xlsx`; XLSX theme format schemes and picture fills survive saves; legacy VML form controls without worksheet `<controls>` survive XLSX saves; GROUPBY/PIVOTBY automatic headers are hidden (G0099); ODS valued rows repeated past the worksheet height read once with one warning per file; `.xls` empty LABEL strings without a final flags byte open; binary workbook sheet compatibility (G0092); `.xls` and XLSB Custom Views survive saving as `.xlsx`; SYLK export added; §7: `.xls` query tables and their data connections kept on save as .xlsx; DIF typed-value import and active-sheet export; SYLK import: values, R1C1 formulas, number formats and column widths; §7: `.xls` imports Excel 2007+ AutoFilter criteria (value lists, colour, icon, above/below average); §17: equations; earlier: §13: Solver Sensitivity report; earlier: §4: chart resizing is no longer a gap; earlier: §13: Analysis ToolPak has all 19 tools; earlier: §4: chart data tables implemented with XLSX round-trip and print/PDF; earlier: §13: Solver Options; earlier: §13: Solver Load/Save Model and Reset All; earlier: §19: XLSB data connections and query tables kept on save as .xlsx; .xls and .xlsb keep Allow Edit Ranges and their passwords on XLSX save; §7: Excel 5.0/95 (BIFF5) `.xls` workbooks open; §19: XLSB imports pictures, shapes, text boxes and charts; §7: `.xls` imports PivotTables that refresh and save as PivotTables; §19: XLSB CF/DV formulas keep other sheets, names and array constants; §7: `.xls` imports embedded charts and chart sheets as live charts; §7, §19: binary workbook window settings imported; `.xls` and `.xlsb` import document properties; §19: XLSB structured references, table calculated column and custom totals formulas imported; §19: XLSB imports data validation and conditional formatting; §7: `.xls` import reads tables; §4: chart data tables implemented with XLSX round-trip and print/PDF; §13: Solver Options, Load/Save Model and Reset All; §19: XLSB array constants and memory caches import as live formulas; §3: Office add-in content (web extension anchors and task panes) survives XLSX saves; §3, §17: XLSX grouped drawing objects kept on save; §7: `.xls` imports the sheet AutoFilter; XLSB imports sheet and workbook structure protection and calculation settings; XLSX saved table ids never reuse a single mapped XML cell id (Excel repair prompt); §7: `.xls` import reads conditional formatting; §19: XLSB imports number formats; §19: XLSB imports print setup, margins, print options, headers/footers and page breaks; XLSB workbook themes and theme tab colours imported; §19: XLSB imports tables and AutoFilters; `.xls` and `.xlsb` restore the saved active visible sheet; §7: `.xls` imports sheet protection, allowed actions and workbook structure protection; §7: `.xls` import reads Excel 2007+ conditional formats (CF12/CFEx); §7: `.xls` import reads page setup, margins, headers/footers, print options, page breaks and the tab colour; §19: XLSB imports hyperlinks and notes; §19: XLSB imports defined names and other-sheet references; §7: `.xls` import reads conditional formatting; §19: XLSB imports number formats; §7: `.xls` data validation imported; §19: XLSB imports cell formulas; §19: XLSB imports shared and array formulas; §7: `.xls` import reads conditional formatting; §19: XLSB imports number formats; §7: `.xls` data validation imported; §19: XLSB imports cell formulas; §19: XLSB imports column widths, row heights, hidden and outlined rows/columns, default sizes, frozen panes, sheet view options and tab colours; §3: XLSX keeps the volatile dependencies part (RTD and CUBE topics) on save; §19: XLSB imports fonts, fills, borders, alignment and protection; XLSX saved table ids never reuse a single mapped XML cell id (Excel repair prompt); §7: `.xls` import reads conditional formatting; §19: XLSB imports number formats; §7: `.xls` data validation imported; §19: XLSB imports cell formulas including text constants; hidden DrawingML objects keep their hidden flag on save and are left out of rendering and printing; §3: worksheet custom properties survive XLSX saves; §3, §17: XLSX SmartArt graphics kept on save; `check` accepts fragment-only relationship targets; XLSX zero page setup scale uses the default 100%; XLSX object click hyperlinks preserved; §8: PivotTable Enable show details; earlier: PivotTable Show Details) · **Target:** Microsoft Excel (Microsoft 365)

Every known shortfall against Excel, one entry each, ranked by user impact (what blocks someone
using GridCraft for real work first). This is the work list: pick from the top, check the open
PRs first (50 open on 2026-10-10; several already address entries here), and update the entry
when you land work. Estimates are Opus 5.5 agent-hours. Area detail lives in the linked parity
docs; the summary is in [target-app-parity.md](target-app-parity.md).

Kinds: **F** feature · **U** UI/UX · **FF** file format · **H** hardware · **L** localization ·
**P** performance · **S** stability.

## Beta blockers

No alpha blockers: every core workflow passes the [alpha gate](roadmap.md#alpha-gate).


### 1. XLSX output not proven in Excel or on real files (FF)
- **Missing:** a run over a real-world XLSX corpus, and confirmation that Excel opens what we save
  with no repair prompt (PivotTables, charts, CF, validation, dynamic arrays, tables).
- **Evidence:** round-trip tests cover our own files only (`crates/xlsx/src/tests/`); PivotTable
  round-trip noted as "unverified in real Excel" since 10-07; #64 (dynamic-array flag) shows the
  class of bug.
- **Progress (2026-11-04):** theme gradients, effects, picture fills and theme relationships survive XLSX saves while colours and fonts remain editable.
- **Progress (2026-11-01):** saved table ids never collide with kept single mapped XML cell ids (Excel repair prompt).
  Worksheet defaults, inactive protection permissions/passwords and validation container prompt settings now survive XLSX saves (G0107); corpus verification remains open.
- **Impact:** the main format; a repair prompt or silent loss destroys trust.
- **Estimate:** 15–25 h, plus owner time in Excel. [file-format-parity.md](file-format-parity.md)

### 2. Large workbooks don't open; recalc is single-threaded (P, H)
- **Missing:** streaming read for ~100 MB XLSX; multi-threaded recalc; 100k-dependent edit under
  100 ms (was ~0.4 s); whole-sheet operations bounded.
- **Evidence:** #175; no threading in `crates/calc`; PR #152 (whole-sheet Fill/Remove Duplicates
  out of memory), PR #151 (current region past 10,000 rows).
- **Impact:** finance and data users routinely have big workbooks.
- **Estimate:** 25–40 h. [target-app-parity.md](target-app-parity.md#performance), [hardware-parity.md](hardware-parity.md)

### 3. Some unmodelled XLSX content is dropped on save (FF)
- **Missing:** slicers and their caches, timelines, the data model, query tables attached to
  tables, unmodelled `extLst` entries, chart details. Pass-through (#330) keeps custom XML,
  external links, connections, sheet query tables, printer settings, worksheet custom properties,
  workbook web publishing preferences (`webPublishing`, encoding and image/layout attributes),
  the volatile dependencies part (RTD and CUBE topics), form and ActiveX controls (including
  legacy VML controls without worksheet `<controls>`), OLE objects,
  sheet backgrounds, SmartArt graphics (diagram parts), grouped drawing objects (`grpSp`) and
  Office add-ins (web extensions: the content add-in's drawing anchor, its `webextension` part and
  its fallback picture, and the workbook's task panes part); never checked in Excel (repair
  prompts unknown).
- Workbook image, link-update, display and compatibility options now survive XLSX saves, including explicit false/default values and the saved theme version (G0108). Links are not refreshed.
- **Evidence:** `crates/xlsx/src/passthrough.rs` (what is kept, and why the rest isn't).
- **Impact:** open-edit-save still simplifies workbooks with slicers, timelines or Power Pivot.
- **Estimate:** 6–10 h. [file-format-parity.md](file-format-parity.md)

### 4. Charts lose options on round trip; shallow chart model (F, FF)
- **Missing:** axis scaling, label options, error bars, per-point formatting.
- **Added:** data tables beneath column, bar, line, area and compatible combo charts, with legend keys, border controls, live source values, native XLSX round-trip and print/PDF.
- **Evidence:** `Chart` holds only title, legend position, two flags and axis titles
  (`crates/model/src/features.rs`); all 2-D kinds survive XLSX round trip (#158, #161); charts
  resize from eight handles (#190). Smooth lines (#62), Switch Row/Column (#116) and trendlines
  (read, written, drawn) landed. Multi-level category axes (year beside month) are read, drawn
  and written back.
  can't be resized (#190). Smooth lines (#62), Switch Row/Column (#116) and trendlines (read,
  written, drawn) landed.
- **Plunder Ledger fork (2026-10-31):** value-axis scaling (Format Axis bounds, major/minor
  units, log base, reverse order) is now modelled (`Chart::scale`), read, written back in schema
  order, and bounds + major unit are drawn (bars clip at the bounds). Reverse order and log scale
  are kept but not yet drawn.
- **Impact:** charts are in most business workbooks.
- **Estimate:** 25–35 h for beta (40–60 h full). [chart-parity.md](chart-parity.md)

### 5. Text in non-Latin scripts doesn't render or export (L)
- **Missing:** CJK font fallback, complex shaping (Arabic, Devanagari, Thai), bidi, non-Latin PDF.
- **Evidence:** system-font fallback for CJK, Thai and Hebrew landed in #38 (closed #32); #142
  (Chinese text) awaits confirmation; #54 (symbols as boxes on Fedora); no shaping for Arabic or
  Devanagari; PDF writer uses 14 standard fonts only.
- **Impact:** blocks most of the world's users even with an English UI.
- **Estimate:** 12–20 h. [localization-parity.md](localization-parity.md)

### 6. Calculation correctness bugs (F)
- **Missing:** nothing known. The four this entry tracked have landed: 15-significant-digit
  equality (#57, so `=0.1+0.2=0.3` is TRUE and MATCH and COUNTIF agree), spill updates after
  structural edits and cleared blockers (#133, #139; PRs #148, #157), 3-D span deletion (#140),
  and qualified names after a sheet rename (#134). Iterative calculation for circular references
  landed too, with its settings in File › Options › Formulas.
- **Impact:** wrong numbers are worse than missing features, so this entry is kept until the
  oracle suite of §15 can say the category is clear rather than only these five reports being so.
- **Estimate:** — [function-parity.md](function-parity.md#formula-language)

### 7. Legacy `.xls` only partly read; encrypted workbooks can't be saved with a password (FF)
- Empty BIFF8 LABEL strings may omit the flags byte at record end; cells retain empty text and their XF. Truncated nonempty strings are still refused (`xls_label_tail_tests.rs`).
- BIFF5 `.xls` sheets with BIFF8 BOFs open using BIFF5 layouts; `.xls` and `.xlsb` repair invalid or duplicate sheet names with one warning, preserving formula and defined-name targets.
- Workbook window display settings now import from the first `.xls` Window1 / `.xlsb` BrtBookView: sheet tabs, scroll bars and tab ratio survive Save As `.xlsx` (`binary_workbook_view_tests.rs`).
- Document properties from `.xls` OLEPS streams and `.xlsb` OPC parts survive Save As `.xlsx`.
- `.xls` query tables (ODBC, OLE DB, web, text imports) and their connections survive Save As `.xlsx`; parameter queries, saved sorts, OLAP and DAO/ADO sources are dropped with a warning.
- **Evidence:** #121: Excel 97-2003 (BIFF8) `.xls` files now open (values, formulas, number
  formats, merges, sizes, names, data validation, conditional formats (cell-value and formula rules), tables); fonts,
  formats, merges, sizes, names, and cell formatting: fonts, fills, borders, alignment); exact theme colours (XFExt), charts, pictures, comments, conditional
  formats, validation are not read, and `.xls` is never written. Password-encrypted
  formats, merges, sizes, names, data validation, conditional formats (cell-value and formula rules),
  the sheet AutoFilter (range, simple criteria and Excel 2007+ value list, colour, icon and average filters)); fonts,
  formats, merges, sizes, names, sheet protection (allowed actions and legacy passwords), workbook
  structure protection, Allow Edit Ranges (titles, references and legacy passwords), data validation,
  conditional formats (cell-value and formula rules)); fonts,
  formats, merges, sizes, names, data validation, conditional formats including the Excel 2007+
  kinds (colour scales, data bars, icon sets, top/bottom, average, text and date rules)); fonts,
  fills, borders, charts, pictures, comments are not read, and `.xls` is never written. Password-encrypted
  formats, merges, sizes, names, data validation, conditional formats (cell-value and formula rules), page setup,
  margins, headers/footers, print options, manual page breaks, tab colour); fonts,
  fills, borders, charts, pictures, comments, Excel 2007+ conditional formats (CF12/CFEx)
  are not read, and `.xls` is never written. Excel 5.0/95 (BIFF5) files open through the same
  reader (values, number formats, names and formulas; 8-bit text in Windows-1252, 1251 or 1256);
  BIFF2-4 files are refused. Password-encrypted
  `.xlsx` workbooks (Agile and Standard encryption) open with their password since #11, but are
  saved without it: GridCraft can't write an encrypted file yet. Encrypted `.xls` (FILEPASS) and
  RC4-encrypted files are still refused with a message.
- **Impact:** common in finance, government and anything older than 2007; encrypted files are
  routine in business.
- **Estimate:** 14–21 h (`.xls` objects 10–15, encryption 4–6). [file-format-parity.md](file-format-parity.md)

### 8. PivotTables: no slicers, calculated items, PivotCharts; unverified in Excel (F)
- **Evidence:** `parity-checklist.md` missing `insert.slicer`, `insert.timeline`,
  `insert.pivotChart`, `table.insertSlicer`; calculated items and GETPIVOTDATA missing (calculated fields
  and values on rows done). Show Details (double-click a value, subtotal or grand total cell) lists
  its source records on a new sheet. The "Enable show details" option (`pivot.options`
  `enableShowDetails`, saved as `enableDrill`) turns it off; it has no PivotTable Options dialog
  yet (command only).
- **Impact:** pivots are the analyst's main tool.
- **Estimate:** 20–30 h for beta (25–40 h full).

### 9. Keyboard-first users: key tips, shortcuts, context menus (U)
- Next/previous sheet navigation now skips Hidden and VeryHidden worksheets without activating a
  hidden sheet at the workbook boundary. Other keyboard gaps below remain.
- **Evidence:** #33, #49 (key tips: initial Home-tab support landed in #43 on 10-10, the rest of the ribbon not yet), #110, #28 (selection),
  59 command shortcuts vs Excel's 200+.
- **Estimate:** 12–20 h. [ui-parity.md](ui-parity.md)

### 10. Crashes and freezes reported from the field (S)
- **Evidence:** #120 (Intel UHD crash on Windows), #117 (error on close, macOS), #166 (freeze opening
  from Finder; PRs #164, #167), #76 (flashing on Windows 11), #130 (caught panic in table styles).
- **Estimate:** 8–12 h plus an autosave/recovery soak.

## After beta

### 11. No localization at all (L)
- Catalog: ribbon-only in five languages (#31, #104, #82; ~12% of strings each), and 943 strings
  (~43%) in Japanese (10-17) and Brazilian Portuguese, including the dialog labels, messages, File
  view and menus. The remaining dialogs, menus and messages are English; en-US function names and
  separators (#45); German in PR #163. **Estimate:**
  100–165 h including #5. [localization-parity.md](localization-parity.md)

### 12. Get & Transform, external data, data model (F)
- No Power Query editor, connections, refresh, From Web/database/folder sources, relationships or
  data model (`data.fromWeb`, `data.relationships` missing). **Estimate:** 60–100 h.

### 13. Analysis tools: Solver, Analysis ToolPak, Forecast Sheet (F)
- Forecast Sheet (`data.forecastSheet`) and the FORECAST.ETS family are in, including Forecast
  Start (hindcast, #785).
- Solver (`data.solver`, #327) has Simplex LP with integer/binary branch and bound, a local
  nonlinear method (augmented Lagrangian, not GRG), the Evolutionary method, and Answer,
  Sensitivity and Limits reports. Report refusal leaves the solution, saved model and history unchanged; a
  successful solve and its reports share one Undo step (#1050). Load/Save Model writes the
  model on cells of its sheet and reads it back; Reset All removes it. Solver Options sets the
  evaluation limit, Integer Optimality and the Evolutionary options, which are saved with the
  model. AllDifferent constraints and the Population report remain.
- The Analysis ToolPak (`data.analysisToolPak`) has all 19 of Excel's tools; their chart output
  remains. **Estimate:** 12–20 h.

### 14. Page Layout view and print polish (U, F)
- XLSX `pageSetup scale="0"` uses the default 100% and is omitted on save.
- Page Layout view landed (#329): paper view with rulers and in-place header/footer editing.
  Margins are dragged on the rulers (#792). Remaining: pictures in headers/footers, Different First Page / Odd & Even pages. Desktop now submits directly through craft-print with printer choice, copies, duplex and grayscale; browser printing stays available. Print ranges and driver tray/options UI remain. **Estimate:** 5–10 h.

### 15. Remaining functions and an oracle suite (F)
- 18 missing, 9 stubs; no Excel-computed oracle corpus; #137, #138. **Estimate:** 16–26 h.
- 20 missing, 9 stubs; no Excel-computed oracle corpus; #137, #138. **Estimate:** 17–29 h.
- 21 missing, 9 stubs; no Excel-computed oracle corpus; #137, #138. **Estimate:** 18–30 h.
- 18 missing, 9 stubs; no Excel-computed oracle corpus; #137, #138. **Estimate:** 18–30 h.
  [function-parity.md](function-parity.md)

### 16. Collaboration and review (F)
- No co-authoring, Show Changes, comment navigation, notes ↔ comments
  conversion; cloud storage requested (#86). **Estimate:** 35–55 h (co-authoring scope needs the owner).

### 17. Insert objects (F)
- Ink strokes now survive XLSX save/reopen as editable polylines. Inserted icons still lose their
  identity/artwork on save (#551); native Office InkML import is not implemented.
- SmartArt, WordArt, signature line, objects, 3-D models, screenshot; 8 shape kinds vs ~170;
  pictures in cells (#61, PR #105). Equations exist (LaTeX source with a live preview, Office Math in xlsx); there is
  no structured (WYSIWYG) equation editing and no ink equations. **Estimate:** 25–40 h.
- SmartArt and grouped drawing objects are kept through XLSX open + save but not shown, printed or editable; WordArt, equations, signature line, objects, 3-D models, screenshot; 8 shape kinds vs
  ~170; pictures in cells (#61, PR #105). **Estimate:** 25–40 h.

### 18. Accessibility (U)
- AccessKit on, but no accessible grid/ribbon tree, no high contrast, no checker. **Estimate:** 12–18 h.
  [ui-parity.md](ui-parity.md#accessibility)

### 19. XLSB/ODS beyond values; minor formats (FF)
- Workbook window display settings now import from the first `.xls` Window1 / `.xlsb` BrtBookView: sheet tabs, scroll bars and tab ratio survive Save As `.xlsx` (`binary_workbook_view_tests.rs`).
- XLSB imports values; cell, shared, array and defined-name formulas (references, operators,
  functions, text and array constants, PtgMemArea caches, other sheets, defined names, structured
  references) with their saved results; defined names; number formats, named cell styles, fonts,
  pattern fills, borders, alignment and protection; sheet layout (column widths, row heights, hidden
  and outlined rows/columns, frozen panes, view options, tab colours); workbook theme colours and
  fonts; hyperlinks and notes; tables (with calculated column and custom totals formulas) and
  AutoFilters; data validation and conditional formatting with their differential formats;
  PivotTables; print setup; pictures, shapes, text boxes and charts from each sheet's drawing part; sheet and workbook structure protection, Allow Edit Ranges (legacy and
  iterated passwords), and calculation settings; data connections and sheet query tables are kept on save as .xlsx,
  including cell-bound query parameters (no OLAP or data-model connections). Windows range permissions warn once per workbook. Left:
  links to other workbooks (kept as saved results), gradient fills, cell style (ixfeParent)
  inheritance, date/colour/icon filters; no XLSB write.
- ODS values only, no write; valued rows repeated past the worksheet height read once with one warning per file; SYLK reads and writes active-sheet values, R1C1 formulas, number formats and column widths (other formatting and objects omitted); DIF reads typed values and saves active-sheet values; no `.prn`, XML Spreadsheet 2003; CSV writes raw
  instead of displayed values; templates saved with the workbook content type.
  **Estimate:** 14–22 h.

### 20. Multiple windows; pen pressure (H)
- Desktop windows done: a native window per workbook window, New Window on the same workbook,
  Switch Windows, Arrange All. Left: Arrange All across monitors, one window on the web, no
  pressure or tilt in ink. **Estimate:** 5–8 h.
  [hardware-parity.md](hardware-parity.md)

### 21. Automation and add-ins (F)
- VBA out of scope (dropped on open, #6); no Office Scripts compatibility; no plugin or custom
  function API. **Estimate:** 25–40 h plus an owner decision.

### 22. AI features (F)
- No in-app assistant, natural-language formulas or Analyze Data; agent control via MCP is
  already strong. **Estimate:** 30–50 h; model choice needs the owner.

### 23. Mobile (platform)
- No iOS, iPadOS or Android layout. **Estimate:** 30–60 h.

### 24. Fonts and look (U)
- Font weights beyond regular/bold (#168), system font list (#4), Calibri metrics (#29), macOS
  chrome (#169, #170), Linux title bar (#54), icon consistency across apps (#173). **Estimate:** 7–12 h.

XLSX shapes, text boxes and pictures now retain click hyperlinks and tooltips on save (G0004); see [file-format-parity.md](file-format-parity.md).

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-04 | minor | XLSX `webPublishing` retains supported encoding, image and layout preferences, including explicit false flags and escaped character-set text, through repeated saves. Invalid optional numbers and unknown screen sizes are omitted; HTML export is unchanged. |
| 2026-11-04 | minor | G0108: XLSX workbookPr image, external-link update policy, display and compatibility settings retain explicit defaults through two saves; malformed values use schema defaults. The saved theme version is retained without synthesizing one; links are never refreshed. |
| 2026-11-04 | minor | G0107: XLSX preserves sparse default-hidden rows and visible exceptions, explicit default-height flags, protection permissions and password metadata even when inactive, and per-sheet validation container prompt settings; counts follow surviving rules and invalid unsigned coordinates are dropped. |
| 2026-11-04 | minor | G0106: XLSX theme format schemes, defaults, colour lists, extensions and picture-fill relationships survive open + save. |
| 2026-11-04 | minor | G0101: XLSX keeps legacy VML form controls and their macro assignments without worksheet `<controls>`, merging notes once. |
| 2026-11-04 | minor | G0099: GROUPBY and PIVOTBY omit a detected header row from both data and output when `field_headers` is omitted, matching Excel mode 1; explicit modes and detection rules are unchanged. |
| 2026-11-03 | minor | G0097: ODS valued rows repeated past the worksheet height read once, with one warning per file; following rows continue immediately. In-bounds expansion limits, blank-row clipping and merge bounds remain enforced. |
| 2026-11-03 | minor | G0096: §7: `.xls` empty LABEL strings without a final flags byte open; truncated nonempty strings remain errors. |
| 2026-11-04 | minor | §19: XLSB cell-bound query parameters (`BrtBeginECParam` `pbt` 2) keep their connection on Save As `.xlsx`: the BIFF12 formula becomes the `cell` attribute (one cell for ODBC/OLE DB, one row or column for a web query), with `refreshOnChange`, `sqlType` and `name`; an external workbook, a range where one cell is required, a formula or a malformed `cce` drops only that connection with one warning. |
| 2026-11-03 | minor | G0092: BIFF5 `.xls` sheets with BIFF8 BOFs open using BIFF5 layouts; `.xls` and `.xlsb` repair invalid or duplicate sheet names with one warning, preserving formula and defined-name targets. |
| 2026-11-02 | minor | SYLK (.slk) export saves active-sheet values, convertible R1C1 formulas, number formats and rounded column widths (G0084). |
| 2026-11-02 | minor | §7: `.xls` query tables and their data connections are kept on save as .xlsx |
| 2026-11-02 | minor | G0086: `.xls` and XLSB Custom Views translate to XLSX workbook and sheet view XML, including panes and selections; malformed views are dropped with one warning. Per-view print settings remain unsupported. |
| 2026-11-02 | minor | DIF opens typed values and saves the active sheet's values, with CP1252/UTF-8 encoding, CRLF output and bounded file size. |
| 2026-11-02 | minor | SYLK (.slk) import reads values, cached R1C1 formulas, number formats and column widths; detected by ID header for any extension. Saving SYLK remains unsupported (G0082). |
| 2026-11-02 | minor | §7: `.xls` imports Excel 2007+ AutoFilter criteria (AutoFilter12 value lists, cell/font colour, icon, above/below average); date filters still warn |
| 2026-11-02 | minor | §17: equations (Insert › Equation, LaTeX source, gallery, editor) |
| 2026-11-02 | minor | §13: Solver Sensitivity report: shadow prices, reduced costs and allowable ranges for Simplex LP; Lagrange multipliers and reduced gradients for GRG Nonlinear |
| 2026-11-02 | minor | §4: charts, like every floating object, move and resize from eight handles (#190), so resizing is no longer listed as missing; the evidence is one paragraph again, without the stale "PRs #158, #161 open" |
| 2026-11-01 | minor | §13: the Analysis ToolPak has all 19 of Excel's tools (two-factor Anova with and without replication, z-Test, Fourier Analysis, Random Number Generation, Sampling); its chart output remains. The stale "`data.solver`, `data.analysisToolPak` missing" and "no Forecast Start" sentences removed |
| 2026-11-02 | minor | §19: XLSB data connections and sheet query tables are kept on save as .xlsx |
| 2026-11-02 | minor | .xls and .xlsb import Allow Edit Ranges with titles, references and legacy or XLSB iterated passwords; retained on XLSX save. Binary Windows permissions warn once per workbook. |
| 2026-11-01 | minor | §7: Excel 5.0/95 (BIFF5) `.xls` workbooks open with values, number formats, names and formulas; BIFF2-4 still refused |
| 2026-11-01 | minor | §19: XLSB imports pictures, shapes, text boxes and charts from each sheet's DrawingML drawing part (BrtDrawing) |
| 2026-11-01 | minor | §7: `.xls` PivotTables (pivot cache streams plus SxView views) on worksheet-range or defined-name sources import as live PivotTables that refresh and save to `.xlsx` as PivotTables; unsupported sources, calculated fields/items, grouping and missing caches keep their cells as values with a warning. |
| 2026-11-01 | minor | §19: XLSB CF/DV formulas use workbook links and array extra data; formulas using other sheets, defined names and array constants are retained, while external workbook references are still skipped. |
| 2026-11-01 | minor | §7: `.xls` imports embedded charts and chart sheets as live charts (kind, series references, titles, legend, axis bounds, colours); chart sheets survive Save As `.xlsx` |
| 2026-11-01 | minor | §7, §19: `.xls` and `.xlsb` workbook window display settings are imported and retained on Save As `.xlsx`; truncated fields keep defaults and tab ratios clamp to 1000. |
| 2026-11-01 | minor | `.xls` imports title, subject, author, keywords, description, last author, created/modified dates, category and company from OLEPS streams; `.xlsb` reads shared OPC core/extended properties. Properties survive Save As `.xlsx`; synthetic malformed-stream and round-trip tests cover import. |
| 2026-11-01 | minor | §19: XLSB imports structured references (`PtgList`) in formulas and tables' calculated column and custom totals formulas; the duplicated §19 bullets are merged into one list |
| 2026-11-01 | minor | §19: XLSB imports data validation (BrtDVal) and conditional formatting (cell value, formula, text, blanks/errors, duplicates, top/bottom, average, dates, colour scales, data bars, icon sets) with their differential formats; rules with formulas the decoder can't read are skipped with one warning |
| 2026-11-01 | minor | §7: `.xls` import reads tables (name, range, header/totals rows, columns, table style) from Feature11/Feature12/List12 records |
| 2026-11-01 | minor | §4: chart data tables implemented for column, bar, line, area and compatible combo charts, with legend keys and border settings, live source values, XLSX round-trip and print/PDF; other chart gaps remain. |
| 2026-10-31 | minor | §13: Solver Options (Max Evaluations, Integer Optimality and the Evolutionary options), saved with the model |
| 2026-10-31 | minor | §13: Solver Load/Save Model (`data.solverSaveModel`, `data.solverLoadModel`) and Reset All (`data.solverReset`), with buttons in the Solver Parameters dialog |
| 2026-11-01 | minor | §19: XLSB array constants and PtgMemArea caches now stay live in cell, shared, array and defined-name formulas; malformed payloads keep saved results. |
| 2026-11-01 | minor | §3: Office add-in content survives XLSX saves: a content add-in's `mc:AlternateContent` drawing anchor, its `webextension` part (with its own rels) and its fallback snapshot picture, and the workbook's `webextensiontaskpanes` part; add-ins are not shown, and an add-in whose part is missing is left out so nothing dangles |
| 2026-11-01 | minor | §3, §17: XLSX keeps grouped pictures, text boxes and shapes (`grpSp`) and their images on save; not shown yet |
| 2026-11-01 | minor | XLSB imports sheet and workbook structure protection (legacy and ISO passwords) and calculation settings; malformed records are skipped with warnings. |
| 2026-11-01 | minor | §19: XLSB imports each sheet's print setup — page setup, margins, print options, headers/footers and manual page breaks — so it prints like Excel; print area and titles are names, still unimported. |
| 2026-11-01 | minor | XLSB imports workbook theme colours and major/minor fonts, plus theme tab colours with tints; unreadable theme parts keep defaults with one warning. |
| 2026-11-01 | minor | §19: XLSB imports tables (ListObjects: names, ranges, header/totals rows, columns, totals functions and labels, style options, filters) and the sheet AutoFilter; calculated column and custom totals formulas warn |
| 2026-11-01 | minor | `.xls` and `.xlsb` restore the first saved workbook view’s active sheet through the imported-sheet mapping; missing, malformed, skipped or hidden targets fall back to the first visible worksheet. |
| 2026-11-01 | minor | §7: `.xls` sheet protection, allowed actions, legacy password hashes and workbook structure protection survive import and Save As `.xlsx`. |
| 2026-11-01 | minor | §7: `.xls` import reads Excel 2007+ conditional formats (CondFmt12/CF12/CFEx): colour scales, data bars, icon sets, top/bottom, above/below average, text, date, blank, error and duplicate rules, and CFEx priority/stop-if-true on classic rules |
| 2026-11-01 | minor | §7: `.xls` imports page setup (paper, orientation, scale, fit to page), margins, headers and footers, print options, manual page breaks and the sheet tab colour |
| 2026-11-01 | minor | §19: XLSB imports hyperlinks and notes (comments part); note box size and visibility are not imported |
| 2026-11-01 | minor | §19: XLSB imports defined names, and formulas using names or other sheets of the same workbook stay live instead of becoming saved results |
| 2026-11-01 | minor | §19: XLSB imports shared (filled-down/right) and array formulas instead of keeping their saved results |
| 2026-11-01 | minor | §19: XLSB imports column widths, row heights, hidden and outlined rows/columns, default sizes, frozen panes, sheet view options and tab colours |
| 2026-11-01 | minor | §19: XLSB cell formulas now decode text constants (PtgStr); array constants still keep saved results. |
| 2026-11-01 | minor | §19: XLSB imports built-in and custom number formats, including formatted blanks; fonts, fills, borders and alignment remain unsupported |
| 2026-11-01 | minor | §7: `.xls` imports the sheet AutoFilter (range from `_FilterDatabase`, custom comparisons, top-10 and blanks); Excel 2007+ AutoFilter12 filters are skipped with a warning |
| 2026-11-01 | minor | §7: `.xls` imports data validation from DV records, including lists, formulas, prompts and ranges; rules survive Save As `.xlsx`. |
| 2026-11-01 | minor | §7: `.xls` import reads conditional formatting (CondFmt/CF cell-value and formula rules); CF12/CFEx still skipped |
| 2026-11-01 | minor | §19: XLSB import decodes cell formulas instead of keeping only their saved results; unsupported tokens keep the value with one warning |
| 2026-11-01 | minor | Hidden DrawingML shapes, connectors, pictures and charts keep `cNvPr hidden` on save and are excluded from drawing, hit-testing, printing and automatic print ranges |
| 2026-11-01 | minor | §3: worksheet custom properties and their binary parts survive XLSX saves with remapped relationships and distinct content types from printer settings |
| 2026-11-01 | minor | §3: XLSX keeps the volatile dependencies part (`xl/volatileDependencies.xml`, the RTD and CUBE topics) byte for byte on save, with its workbook relationship and content type override; not interpreted, and none is added to a workbook that has none |
| 2026-11-01 | minor | §3, §17: XLSX keeps SmartArt graphics (the drawing anchor and its data, layout, style, colours and diagram drawing parts) on save; not shown yet |
| 2026-11-01 | trivial | `gridcraft-cli check` treats fragment-only relationship targets as same-document references, avoiding false missing-part reports for internal drawing hyperlinks; duplicate ids and targets naming absent parts are still checked. |
| 2026-11-01 | minor | XLSX `pageSetup scale="0"` keeps the default 100% and is omitted on save; nonzero scales keep the existing 10–400 clamp, and zero paper size keeps the default paper. |
| 2026-11-01 | minor | Saved table ids are unique and never reuse a single mapped XML cell id (Excel repair prompt). |
| 2026-10-31 | minor | G0004: closed XLSX object click hyperlink loss for shapes, text boxes and pictures; external URLs and workbook locations retained with tooltips |
| 2026-10-24 | minor | §8: PivotTable Enable show details: `pivot.options` `enableShowDetails` turns Show Details off for a PivotTable, read and written in XLSX as `enableDrill` |
| 2026-10-23 | minor | §8: PivotTable Show Details: double-clicking a value cell lists its source records on a new sheet (#2364) |
| 2026-10-22 | minor | §20: a native window per workbook window, New Window on the same workbook and Arrange All on the desktop; left: Arrange All across monitors, the web's one window, pen pressure (estimate 10–14 h → 5–8 h) |
| 2026-10-20 | minor | §11: the Japanese catalog covers the dialogs, messages and menus — 943 keys (~43%), matching the pt-BR coverage; a test keeps the two catalogs in step |
| 2026-10-17 | minor | §6: the five reports it tracked are closed and their PRs merged (#57, #133, #139, #140, #134; PRs #148, #155, #156, #157); the entry stays open for §15's oracle suite |
| 2026-10-15 | minor | §6: Maximum Iterations and Maximum Change in File › Options › Formulas; dropped the stale "no Options dialog yet" |
| 2026-10-15 | minor | §13: Solver prepares reports before committing the solution, model and report sheets together; report refusal preserves the workbook and Undo/Redo (#1050) |
| 2026-10-13 | trivial | §4: multi-level category axes landed |
| 2026-10-12 | minor | §9: next/previous sheet commands retain the current visible sheet and selection when there is no visible destination |
| 2026-10-12 | minor | rows auto-fit oversized fonts on open and whole-row/column formatting (#833) |
| 2026-10-12 | minor | Page Layout view: dragging margins on the rulers (#792) removed from the §14 remainder |
| 2026-10-12 | minor | §13: Forecast Sheet Forecast Start (hindcast) landed (#785) |
| 2026-10-12 | minor | §17: ink geometry survives XLSX saves; the icon portion of #551 remains |
| 2026-10-12 | minor | §8: values on rows (Σ Values in Rows, `dataOnRows`) laid out, moved and round-tripped |
| 2026-10-12 | minor | Desktop File > Print now discovers printers and submits PDF jobs through pinned craft-print with printer choice, copies, duplex and grayscale; browser printing and PDF export remain available. Remaining print parity includes page ranges and driver-specific tray/options UI; physical printer fidelity needs hardware validation. |
| 2026-10-11 | minor | §6: iterative calculation for circular references landed (Maximum Iterations, Maximum Change) |
| 2026-10-11 | minor | §15: trim-reference operators (`A1:.E10`, `A1.:E10`, `A1.:.E10`) parse and evaluate |
| 2026-10-11 | minor | §4: trendlines (six types, forecast, equation and R²) read, written and drawn |
| 2026-10-11 | minor | §7: password-encrypted workbooks open with their password; saving with one remains (#11) |
| 2026-10-11 | minor | §16: comment navigation, Show Comments, Show All Notes and Convert to Comments landed (#281) |
| 2026-10-11 | minor | §7: `.xls` (BIFF8) import of values, formulas and number formats (#121) |
| 2026-10-11 | minor | §3: XLSX pass-through keeps custom XML, external links, connections, query tables, printer settings, controls and OLE objects (#330, #324) |
| 2026-10-11 | minor | §13: Forecast Sheet landed (#322) |
| 2026-10-11 | minor | §15: ODDFPRICE, ODDFYIELD, ODDLPRICE, ODDLYIELD added (18 functions missing) |
| 2026-10-11 | minor | §15: PERCENTOF and EUROCONVERT added (20 functions missing) |
| 2026-10-11 | minor | §15: BAHTTEXT added (21 functions missing) |
| 2026-10-11 | minor | §15: TRIMRANGE added (21 functions missing; trim-reference operators still missing) |
| 2026-10-11 | minor | §16: Allow Edit Ranges landed (#307) |
| 2026-10-11 | minor | §8: calculated fields added |
| 2026-10-11 | minor | §8: PivotCharts added |
| 2026-10-11 | minor | §13, §15: FORECAST.ETS family landed (#314) |
| 2026-10-10 | minor | Updated for the 10-10 merges (#17, #31, #38, #62, #82, #104, #116) and the new chart resize bug (#190) |
| 2026-10-10 | minor | Noted that no gap blocks the alpha gate |
| 2026-10-10 | major | Created: 24 ranked gaps with evidence, from the full re-measure and the 60 open issues |
