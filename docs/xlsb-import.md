# Importing Excel binary workbooks

Last updated: 2026-11-01.
> **Last updated:** 2026-11-01 · **Change:** minor (workbook themes and theme tab colours imported)

Open an `.xlsb` file with File → Open, drag it onto GridCraft, or pass it to the CLI.
GridCraft imports worksheet data into a new workbook and shows the conversion limits.

The import reads worksheet names, visibility, numbers, text, booleans, errors, merged
cells, hyperlinks (web pages, files and places in the workbook, with their tooltips) and notes
(author and plain text). A note's box size, position and whether it is shown come from a
drawing part that is not imported: notes open hidden in the default box. Hyperlinks or notes
that can't be read are skipped with one warning each. Formula cells become their **last saved results**, not live formulas. Those results
The import reads worksheet names, visibility, numbers, text, booleans, errors and merged
cells. It also keeps the sheet layout: column widths, row heights, hidden and outlined
rows and columns, default column width and row height, frozen panes, gridlines, headings,
zero display, right-to-left, zoom and RGB or indexed tab colours (theme tab colours and split
panes are not imported). Supported cell, shared, array and defined-name formulas stay live,
including array constants (numbers, strings, booleans and errors) and range intersections
preceded by a memory cache. Their saved results are retained until recalculation.
Unsupported or malformed formulas keep only their last saved results with a warning; these
zero display, right-to-left, zoom and RGB, indexed or theme tab colours, including theme tints
(split panes are not imported). Formula cells become their **last saved results**, not live formulas. Those results
may be out of date if the source application did not recalculate before saving.
Links to other workbooks, external names and table formula tokens remain unsupported.

Built-in and custom number formats, fonts, pattern fills, borders, alignment and protection
are imported. Gradient fills, cell style inheritance and differential formats are omitted.
The workbook's date system is retained. Defined names retain their scope and hidden flag.
Charts, images and macros are not imported. GridCraft does not write XLSB files.
Each sheet's print settings are imported, so a sheet prints as it did in Excel: page
orientation, paper size, print scale or fit-to-pages, the six page margins, gridlines and
headings, horizontal and vertical centring, the header and footer code strings (`&L&C&R`,
`&P`, `&N`, `&"font"` … kept verbatim, including the different-odd/even and different-first
page variants) and manual row and column page breaks. A damaged print record is skipped with
one warning and the rest of the sheet's settings are kept. Print area and print titles are
defined names, which are not imported yet, so a sheet with a saved print area prints its used
range.
The workbook theme's colours and major/minor fonts are imported. Without a theme relationship,
the default Office theme is used. A missing, unreadable, oversized or malformed theme part
uses the default theme with one warning; worksheet data still imports. Theme cell colours
and tab colours retain their theme indices and tints through XLSX saves.

Source formatting is omitted: dates, times, percentages and currencies initially display as
their underlying numbers. The workbook's date system is retained.

Data validation (drop-down lists, input messages and error alerts) and conditional
formatting (cell-value and formula rules, text, blank and error tests, duplicates, top/bottom,
above/below average, dates, colour scales, data bars and icon sets) are imported with the
fill, font, border and number format each rule applies. A rule or validation whose formula
uses something the import can't read yet is left out, with one warning per sheet. Charts, images, names,
macros and other workbook features are not imported. GridCraft does not write XLSB files.

Tables keep their name, range, header and totals rows, column names, totals functions and
labels, table style and its options (first/last column, banded rows/columns) and their
AutoFilter; the sheet AutoFilter is imported too (value lists, custom comparisons, top 10,
above/below average). Calculated column formulas and custom totals row formulas are not
imported yet, nor are date, colour and icon filters; each gives one warning. A table whose
part is missing or damaged, whose range doesn't fit its columns, or whose name repeats
another table's is skipped with one warning per kind.

Save opens Save As and suggests a new `.xlsx` file. The source XLSB file is not used as the
workbook's save destination; requests to save to `.xlsb` return an error.

```sh
gridcraft-cli info source.xlsb
gridcraft-cli convert source.xlsb imported.xlsx
```

CLI import warnings go to stderr, leaving structured output on stdout available for scripts.

Sheet protection and workbook structure protection are imported, including legacy password
verifiers and ISO iterated password hashes. Calculation mode (automatic, manual or automatic
except data tables), iterative calculation, maximum iterations, maximum change and precision
as displayed are retained. These settings survive saving the imported workbook as XLSX.
Malformed protection or calculation records are skipped with a warning.

Record layouts and identifiers follow Microsoft's public [MS-XLSB] specification,
sections 2.3.2, 2.4.311–312, 2.4.318, 2.4.792–793 and 2.5.80. The ISO protection record
identifiers are 677 (workbook) and 678 (sheet).

## Revision history

| Date | Change |
|---|---|
| 2026-11-01 | Array constants and PtgMemArea caches decode in cell, shared, array and defined-name formulas; malformed payloads retain saved results. Layouts follow [MS-XLSB] PtgArray, PtgExtraArray, SerAr and PtgExtraMem. |
| --- | --- |
| 2026-11-01 | Import sheet and workbook structure protection and calculation settings. |
## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-11-01 | minor | XLSB imports workbook theme colours and major/minor fonts, plus theme tab colours with tints; unreadable theme parts keep defaults with one warning. |
