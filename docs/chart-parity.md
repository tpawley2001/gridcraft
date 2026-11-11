# Chart parity

> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-11-11 · **Change:** minor (G0281: a date axis (`c:dateAx`) stays a date axis on open and save with its time units and bounds; earlier: G0256: a 3-D column or bar series keeps its own bar shape on open and save; earlier: G0246: 3-D column, bar, line, area and pie charts kept on open and save and drawn in 3-D; earlier: G0247 exploded pie slices; earlier: pie of pie and bar of pie; earlier: G0244 surface charts; earlier: Analysis ToolPak Regression Residual, Line Fit and Normal Probability plots; earlier: chart-data-table keys pass the series index to the shared combo plot-kind rule, restoring compilation after #1150; earlier: Analysis ToolPak chart output for Histogram, Moving Average and Exponential Smoothing; doughnut charts: every series is a concentric ring, the first series innermost; hole size and first slice angle kept; rotated slices cover their own sweep; legend names categories; earlier: charts resize from eight handles, chart data tables, legend keys and borders, native XLSX round-trip and print/PDF; earlier: selected chart source outlines stay beneath floating objects) · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4
> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-11-11 · **Change:** minor (G0276: filled radar charts open, draw filled and save as filled; earlier: G0256: a 3-D column or bar series keeps its own bar shape on open and save; earlier: G0246: 3-D column, bar, line, area and pie charts kept on open and save and drawn in 3-D; earlier: G0247 exploded pie slices; earlier: pie of pie and bar of pie; earlier: G0244 surface charts; earlier: Analysis ToolPak Regression Residual, Line Fit and Normal Probability plots; earlier: chart-data-table keys pass the series index to the shared combo plot-kind rule, restoring compilation after #1150; earlier: Analysis ToolPak chart output for Histogram, Moving Average and Exponential Smoothing; doughnut charts: every series is a concentric ring, the first series innermost; hole size and first slice angle kept; rotated slices cover their own sweep; legend names categories; earlier: charts resize from eight handles, chart data tables, legend keys and borders, native XLSX round-trip and print/PDF; earlier: selected chart source outlines stay beneath floating objects) · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4
> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-11-11 · **Change:** minor (G0275: 3-D columns and bars draw each series with its own shape; earlier: G0256: a 3-D column or bar series keeps its own bar shape on open and save; earlier: G0246: 3-D column, bar, line, area and pie charts kept on open and save and drawn in 3-D; earlier: G0247 exploded pie slices; earlier: pie of pie and bar of pie; earlier: G0244 surface charts; earlier: Analysis ToolPak Regression Residual, Line Fit and Normal Probability plots; earlier: chart-data-table keys pass the series index to the shared combo plot-kind rule, restoring compilation after #1150; earlier: Analysis ToolPak chart output for Histogram, Moving Average and Exponential Smoothing; doughnut charts: every series is a concentric ring, the first series innermost; hole size and first slice angle kept; rotated slices cover their own sweep; legend names categories; earlier: charts resize from eight handles, chart data tables, legend keys and borders, native XLSX round-trip and print/PDF; earlier: selected chart source outlines stay beneath floating objects) · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4
> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-11-10 · **Change:** minor (G0253: axis display units and crossing points are kept on open and save and drawn; earlier: G0247: exploded pie and doughnut slices are kept on open and save and drawn pulled out; earlier: Analysis ToolPak Regression Residual, Line Fit and Normal Probability plots; earlier: chart-data-table keys pass the series index to the shared combo plot-kind rule, restoring compilation after #1150; earlier: Analysis ToolPak chart output for Histogram, Moving Average and Exponential Smoothing; doughnut charts: every series is a concentric ring, the first series innermost; hole size and first slice angle kept; rotated slices cover their own sweep; legend names categories; earlier: charts resize from eight handles, chart data tables, legend keys and borders, native XLSX round-trip and print/PDF; earlier: selected chart source outlines stay beneath floating objects) · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4
> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-11-10 · **Change:** minor (pie of pie and bar of pie: opened, drawn, printed and saved as `c:ofPieChart` with their split settings; earlier: G0244: 3-D surface, wireframe 3-D surface, contour and wireframe contour open, draw, print and save; earlier: Analysis ToolPak Regression Residual, Line Fit and Normal Probability plots; earlier: chart-data-table keys pass the series index to the shared combo plot-kind rule, restoring compilation after #1150; earlier: Analysis ToolPak chart output for Histogram, Moving Average and Exponential Smoothing; doughnut charts: every series is a concentric ring, the first series innermost; hole size and first slice angle kept; rotated slices cover their own sweep; legend names categories; earlier: charts resize from eight handles, chart data tables, legend keys and borders, native XLSX round-trip and print/PDF; earlier: selected chart source outlines stay beneath floating objects) · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4
> **Last reviewed:** 2026-10-18 · **Last updated:** 2026-11-10 · **Change:** minor (G0258: 3-D column and bar charts draw cylinders, cones and pyramids; earlier: G0246: 3-D column, bar, line, area and pie charts kept on open and save and drawn in 3-D; earlier: G0247 exploded pie slices; earlier: pie of pie and bar of pie; earlier: G0244 surface charts; earlier: Analysis ToolPak Regression Residual, Line Fit and Normal Probability plots; earlier: chart-data-table keys pass the series index to the shared combo plot-kind rule, restoring compilation after #1150; earlier: Analysis ToolPak chart output for Histogram, Moving Average and Exponential Smoothing; doughnut charts: every series is a concentric ring, the first series innermost; hole size and first slice angle kept; rotated slices cover their own sweep; legend names categories; earlier: charts resize from eight handles, chart data tables, legend keys and borders, native XLSX round-trip and print/PDF; earlier: selected chart source outlines stay beneath floating objects) · **Target:** Microsoft Excel (Microsoft 365), Excel for Mac 16.113.4

Excel's chart types and chart elements against GridCraft. Part of
[target-app-parity.md](target-app-parity.md). **Breadth ~50%, ready ~30%, 40–60 h** (estimated).

Sources: `crates/model/src/features.rs` (`ChartKind`, `Chart`, `Series`), `crates/chart/src`
(layout and rendering), `crates/xlsx/src/chart.rs` (DrawingML chart parts), merged PRs #62, #116,
#158 and #161, and issue #190. Behaviour checked against Microsoft's
[Pareto](https://support.microsoft.com/en-us/excel/create-a-pareto-chart) and
[pie/doughnut](https://support.microsoft.com/en-us/excel/add-a-pie-chart) support pages.

## Chart types

| Excel type | Excel subtypes | GridCraft | Notes |
|---|---|---|---|
| Column | clustered, stacked, 100% stacked, 3-D clustered/stacked/100%, 3-D column | 3 of 7, + 4 3-D opened, drawn and saved | 3-D kept on open/save (`c:bar3DChart`, view angles, gap depth, group and per-series shape, series axis) and drawn as shaded boxes on a floor and walls; oblique projection, so perspective and `hPercent` are approximate; cylinder, cone and pyramid shapes drawn, the group shape (`c:bar3DChart/c:shape`) or a series' own (`c:ser/c:shape`) when it has one (per-series shape drawn); 3-D not yet in Insert ▸ Chart |
| Bar | clustered, stacked, 100%, 3-D ×3 | 3 of 6, + 3 3-D opened, drawn and saved | 3-D drawn and kept as for Column; not yet in Insert ▸ Chart |
| Line | line, stacked, 100% stacked, with markers ×3, 3-D line | 3 of 7 (line, with markers, stacked), + 3-D line opened, drawn and saved | no 100% stacked; smoothed lines landed (#62); 3-D line kept (`c:line3DChart` with its series axis) and drawn as ribbons, one row per series |
| Pie | pie, 3-D pie, pie of pie, bar of pie, doughnut | 4 of 5 (pie, pie of pie, bar of pie, doughnut), + 3-D pie opened, drawn and saved | 3-D pie kept (`c:pie3DChart`, view angles) and drawn as a tilted ellipse with a side band, exploded slices pulled out; pie of pie and bar of pie draw the second plot, combined slice and series lines, and keep gap width, split (auto, position, value, percent, custom) and second plot size on XLSX round-trip; exploded slices (series and single-slice `c:explosion`) are kept on open and save and drawn pulled out, the pie shrinking so they fit (doughnut: outer ring); doughnut draws every series as a concentric ring, first series innermost; its legend names categories, matching Excel's default vary-by-category colours |
| Area | area, stacked, 100%, 3-D ×3 | 3 of 6, + 3 3-D opened, drawn and saved | 3-D kept (`c:area3DChart`, series axis for standard grouping) and drawn as extruded slabs; not yet in Insert ▸ Chart; 100% stacked drawn as shares of each category's total and kept as `percentStacked` in XLSX |
| X Y (scatter) | markers, smooth lines (±markers), straight lines (±markers) | 2 of 5 | no smooth lines |
| Bubble | bubble, 3-D bubble | 1 of 2 | |
| Stock | high-low-close, open-high-low-close, volume-HLC, volume-OHLC | 1 kind | |
| Surface | 3-D surface, wireframe, contour, wireframe contour | 4 of 4 | read and written as `c:surface3DChart` / `c:surfaceChart` with series axis and view; contour bands and legend follow the value axis' major unit; the 3-D view is drawn at Excel's default angles (a file's own rotation is written back as the default) with one colour per cell, not split at band edges |
| Radar | radar, with markers, filled | 2 of 3 | Filled radar opens, draws each series as a filled shape and saves as `radarStyle val="filled"`; radar with markers draws without markers |
| Treemap, Sunburst | 1 each | 2 of 2 | |
| Histogram, Pareto | 2 | 1 of 2 | no Pareto |
| Box & Whisker | 1 | 1 of 1 | exclusive (default) and inclusive quartiles per series; XLSX round-trip; `chart.set {quartiles}` changes all series |
| Waterfall, Funnel | 2 | 2 of 2 | lose their kind in XLSX (PR #161) |
| Box & Whisker | 1 | 1 of 1 | |
| Waterfall, Funnel | 2 | 2 of 2 | |
| Filled map | 1 | 0 | needs geographic boundary data (licensing decision) |
| Combo | custom combinations, secondary axis | yes (line on secondary axis) | |
| **Total** | ~60 subtypes | **30 (~50%)**, + 12 3-D subtypes opened, drawn and saved | usage-weighted breadth higher (~65%): column, bar, line, pie and scatter dominate |

## Chart elements and formatting

| Element | Excel | GridCraft |
|---|---|---|
| Move and resize the chart object | yes | yes (eight handles, each moving only its own edges; Format Chart Area Size fields) (#190) |
| Selected chart source-data outline | visible on uncovered cells, beneath floating objects | painted above cells and below charts, shapes, pictures and slicers; retains pane clipping and right-to-left cell geometry (#2861) |
| Chart title, axis titles | text, linked to cells, full formatting | text only |
| Legend | position, overlay, formatting, per-entry delete | position only |
| Data labels | value, category, series, percentage, from cells, callouts, position | one on/off flag |
| Gridlines | major/minor per axis, formatting | one on/off flag |
| Axis options | min/max/units, log scale, reverse order, crossing point, display units, date axis, number format | display units (built-in and custom, with their label) and crossing points (automatic, minimum, maximum, a value or category) read, written and drawn: labels divided by the unit, axes and their labels moved to where they cross, columns and bars grow from the category axis; date axis (`c:dateAx`: auto, base/major/minor time units, major/minor units, bounds) read and written, not yet drawn as a time scale; others **missing** |
| Multi-level category axis (a category range two or more columns wide beside the values: year beside month) | the outer levels grouped under the labels with separators | read (`multiLvlStrRef`), drawn (rows under the labels; columns beside them in bar charts; up to three outer levels), written back with Excel's level caches |
| Doughnut hole size, first slice angle | 10–90% hole (Excel default 75%), 0–359° first slice | Format Chart pane sliders; read from and written to `c:holeSize` / `c:firstSliceAng`; documents saved before the setting existed keep their old 50% hole |
| Secondary axis | any series | line series in combo charts |
| Trendlines | linear, exponential, logarithmic, polynomial, power, moving average, equation and R² | all six types, several per series, name, forward/backward forecast, set intercept, equation and R² label, line colour/width/dash; read, written and drawn on 2-D column, bar, line, area, scatter and bubble series; `chart.addElement` `trendline` adds/removes them. No Format Trendline pane, label position/number format not kept |
| Error bars | fixed, %, standard deviation/error, custom | **missing** |
| Up/down bars, high-low lines, drop lines, series lines | yes | **missing** |
| Data table under the chart | yes | yes for column, bar, line, area and compatible combo charts: live formatted values, blank cells, grouped category headers, optional legend keys, horizontal/vertical borders and outline; native `c:dTable` XLSX round-trip; shared UI/print/PDF rendering |
| Per-series / per-point fill, line, marker, effects | Format pane for every element | series colour only; Format Chart pane covers chart-level options |
| Chart styles and colour palettes | ~16 styles × colour sets | style number stored, own palettes |
| Chart sheets | yes | **kept, not editable**: not shown, and written back unchanged in their place on save (#727); whole-workbook print and PDF export print each visible one as a page; left out when a sheet they plot is deleted or renamed. No F11 / Move Chart > New sheet |
| Chart templates (`.crtx`) | yes | **missing** |
| Recommended Charts | yes | dialog with recommendations from the data's shape (own heuristic) and an All Charts list, live previews (#215) |
| Switch Row/Column, Select Data | yes | yes (Switch on charts read from files landed in #116) |
| Recommended Charts | yes | partial (Insert ▸ Recommended) |
| Switch Row/Column, Select Data | yes | yes (Switch on charts read from files landed in #116); Select Data dialog: data range, remove series, Horizontal (Category) Axis Labels (#287). No series add/edit or hidden/empty cell options |
| Sparklines (line, column, win/loss, markers, axis) | yes | line, column, win/loss |
| PivotCharts | yes | Insert ▸ PivotChart (`insert.pivotChart`): bound to a PivotTable's report, rebuilt on refresh and layout changes, `c:pivotSource` round trip; no field buttons, innermost category level only |
| Animation on data change | yes (Windows) | no |

Data tables are controlled by `chart.addElement {element: "dataTable", on: true,
 dataTableKeys: true}` or `chart.set {dataTable: true}`. `chart.set` also accepts
`dataTableKeys`, `dataTableHorizontalBorder`, `dataTableVerticalBorder` and `dataTableOutline`.
Changing to an unsupported chart kind removes the table (Undo restores it). The chart legend
is independent. Bar charts keep their vertical category axis; other supported charts use the
table headers instead of repeating category-axis labels. Individual labels and values that
exceed a cell's width are shortened with an ellipsis. Tables are bounded to 128 categories,
32 series and half the available chart height; if they cannot fit, a “Data table needs more
space” notice replaces the table. Enlarge the chart or reduce its source range. Table-specific
font/fill styling and exact Excel layout fidelity are not implemented; Excel-side verification
has not been performed.

Format reference: [Microsoft's DrawingML data table documentation](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.charts.datatable?view=openxml-3.0.1).

Analysis ToolPak Chart Output creates embedded charts for Histogram (including Pareto and cumulative percentage), Moving Average and Exponential Smoothing, and Regression Residual Plots and Line Fit Plots (one XY scatter per X variable) and the Normal Probability Plot; report and charts share one Undo step.

## XLSX round-trip

Charts are read from and written to DrawingML chart parts, but only the fields the model holds
survive: anything else (axis scaling, label options, error bars, per-point formats, chartex
extensions) is lost when GridCraft saves a workbook it opened. That is the main reason charts are
**~30% ready**: a user who opens an Excel workbook with charts and saves it gets simpler charts
back.

Series data, categories and names may refer to defined names, as dynamic (OFFSET) ranges do:
Excel writes a workbook-scoped name as `[0]!Name` in `<c:f>`. GridCraft reads it as the bare
name and writes it back with `[0]!`; a sheet-scoped name is written `Sheet!Name`. Excel needs the
qualifier: with a bare name in `<c:f>` it draws the whole chart blank, without a repair prompt
(observed in Excel for Windows, Microsoft 365). External books (`[1]Sheet1!…`) are kept as they
are (#1686). Such series get no `numCache`/`strCache` on save (#1690).

## Work (40–60 h)

1. Preserve unmodelled chart XML on round-trip (keep the original part, patch only edited fields): 6–10 h.
2. Axis options, label options, gridlines per axis, legend formatting: 10–15 h.
3. Error bars; Format Trendline pane (trendlines landed): 4–6 h.
4. Per-element Format pane parity, smoothed lines, 100% variants: 8–12 h.
5. 3-D column/bar/line/area in Insert ▸ Chart (opening, drawing and saving landed), Pareto: 6–10 h.
6. Chart sheets, templates and PivotChart field buttons: 5–8 h. Filled map: owner decision on boundary data.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-11 | minor | G0281: a chart's primary category axis stored as `c:dateAx` is written back as `c:dateAx` (was `c:catAx`) with `auto`, base/major/minor time units, major/minor units, its scaling bounds and its axis ids, in CT_DateAx order and without `lblAlgn`/`noMultiLvlLbl`; invalid values are dropped. Points are still drawn evenly spaced. No readiness score change. |
| 2026-11-11 | minor | G0276: filled radar charts (`c:radarStyle val="filled"`, ST_RadarStyle) open as a filled radar, draw each series as a filled shape in its colour (later series over earlier ones, axis labels on top) on screen, print and PDF, and save back as `filled`; `insert.chart` takes `{"type":"radar","subtype":"filled"}`. Radar 2 of 3 subtypes. |
| 2026-11-11 | minor | G0275: 3-D columns and bars draw each series with its own shape (`c:ser/c:shape`), falling back to the group shape for series without one, like Excel. No readiness score change. |
| 2026-11-10 | minor | G0256: a 3-D column or bar series keeps its own bar shape (`c:ser/c:shape`, ST_Shape) on open and save, written after `c:val`; an unknown value is dropped and a flat bar chart never gets one. The renderer still draws boxes. No readiness score change. |
| 2026-11-10 | minor | G0253: value axes keep their display units (`c:dispUnits`: `c:builtInUnit` or `c:custUnit`, `c:dispUnitsLbl`) and every axis its crossing point (`c:crosses` automatic/min/max or `c:crossesAt`) through open and save. Drawn on screen, print and PDF: tick labels divided by the unit with Excel's unit label ("Thousands", "x 10000"…), the category axis at its value with its labels beside it and columns/bars growing from it, the value axis at the left edge of its category's slot, far-end crossings with their labels outside; scatter X/Y axes likewise. No readiness score change. |
| 2026-11-10 | minor | G0258: 3-D column and bar charts draw the `c:shape` bar shape on screen, print and PDF: cylinders, cones and pyramids over the box footprint, apex at the value end; `coneToMax` and `pyramidToMax` are slices of one solid whose apex is at the value axis end. No readiness score change. |
| 2026-11-10 | minor | 3-D column, bar, line, area and pie charts stay 3-D on open and save (`c:*3DChart`, `c:view3D` values as the file had them, floor and walls, gap depth, shape, series axis) and draw in 3-D on screen, print and PDF: shaded boxes, ribbons, extruded areas and a tilted pie. No readiness score change. |
| 2026-11-10 | minor | G0247: exploded pie and doughnut slices: series `c:explosion` and per-slice `c:dPt` explosion are read (clamped to 400%) and written in pie and doughnut series, and drawn pulled out along each slice's bisector on screen, print and PDF; the pie shrinks so the farthest slice fits. No readiness score change. |
| 2026-11-10 | minor | Pie of pie and bar of pie open as their own kinds instead of a plain pie, draw and print the second pie or stacked bar with the combined slice and series lines, save back as `c:ofPieChart` with gap width, split and second plot size, and are offered in Insert ▸ Pie and All Charts. Pie row 2 of 5 → 4 of 5. |
| 2026-11-10 | minor | G0244: the four surface chart types are chart kinds: read from and written to XLSX (`c:wireframe`, three axis ids, `c:serAx`, `c:view3D`), inserted from the ribbon, All Charts and Change Chart Type, and drawn on screen, in print and PDF: contour as value bands with a band legend, 3-D surface as a projected grid with floor, walls and three labelled axes. |
| 2026-11-09 | minor | Analysis ToolPak Regression adds Residual, Line Fit and Normal Probability plots as XY scatter charts stacked right of the report. No readiness score change. |
| 2026-11-07 | minor | Chart-data-table keys use the shared plot-kind helper with their series index after #1150. A focused regression checks the first combo series uses a column key and later series use line keys. No readiness score change. |
| 2026-11-06 | minor | G0131: Histogram, Moving Average and Exponential Smoothing offer Chart Output; report and chart share one Undo step. Regression plots remain. |
| 2026-11-02 | minor | Charts move and resize from eight handles, each moving only its own edges (#190); resizing leaves the work list (1–2 h, within the 40–60 h rounding) |
| 2026-11-01 | minor | Chart data tables for column, bar, line, area and compatible combo charts: command/UI controls, formatted source values, blanks, grouped category headers, legend keys, independent borders, shared print/PDF rendering and native c:dTable import/export. Dense tables show a space notice; no readiness score change. |
| 2026-11-01 | trivial | Whole-workbook print, preview and PDF export print each visible chart sheet as one page, its chart scaled to the page |
| 2026-10-29 | minor | Selected charts highlight their source cells beneath floating objects, so the blue outline no longer crosses the chart; uncovered source cells remain highlighted (#2861) |
| 2026-10-24 | minor | 100% Stacked Area: inserted from the ribbon and All Charts, drawn as shares of each category's total, read and written as `percentStacked` |
| 2026-10-18 | trivial | Stale notes: stock/combo, treemap/sunburst, box and waterfall kinds survive XLSX round-trip (PRs #158, #161) |
| 2026-10-18 | minor | Doughnut charts: every series is a ring (first series innermost), hole size and first slice angle modelled, editable and round-tripped; rotated slices split at 12 o'clock without overlap; legend names categories like Excel's default (#1823) |
| 2026-10-16 | trivial | Series that refer to defined names (`[0]!Name`, `Sheet!Name`) resolve instead of showing `#NAME?`, and are saved qualified, as Excel needs (#1686) |
| 2026-10-16 | minor | Box & whisker quartiles default to exclusive, honor each series on draw and XLSX round-trip; `chart.set {quartiles}` changes the method (#1453) |
| 2026-10-13 | minor | Multi-level category axes: read, drawn (outer levels under or beside the labels), written back with level caches |
| 2026-10-12 | trivial | chart sheets are kept on save, still not editable (#727) |
| 2026-10-11 | minor | Trendlines: all six types modelled, round-tripped through `c:trendline`, drawn with forecast and equation/R² labels; `chart.addElement` `trendline` |
| 2026-10-11 | minor | PivotCharts (`insert.pivotChart`) |
| 2026-10-11 | minor | Recommended Charts dialog with previews (#215) |
| 2026-10-11 | minor | Select Data dialog with category axis labels; a blank top-left cell makes the first column the categories (#287) |
| 2026-10-10 | minor | Smooth lines (#62) and Switch Row/Column on file charts (#116) landed; chart resize bug (#190) |
| 2026-10-10 | major | Created from a source audit: 25 kinds (25 of ~60 subtypes), element gaps, round-trip loss |
