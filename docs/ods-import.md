# Importing OpenDocument spreadsheets

Open an `.ods` file with File → Open, drag it onto GridCraft, or pass it to the CLI. GridCraft
imports worksheet data into a new workbook and shows a warning describing the conversion limits.

The import reads sheets, numbers, text, booleans, supported dates/times, repeated cells/rows,
merged cells, workbook-level named ranges and **formulas**. A formula (OpenFormula,
`of:=SUM([.A1:.B2];1)`) is translated to Excel syntax (`SUM(A1:B2,1)`: bracketed references,
quoted sheet names, 3-D ranges, `;` separators and `{1;2|3;4}` arrays, `COM.MICROSOFT.` and
`LEGACY.` function prefixes) and kept live, with the file's saved result as its cached value.

A formula falls back to its **last saved result** (as every formula did before) when it uses
something without an Excel equivalent: external links, the `~` union or `!` intersection
operators, LibreOffice-only functions (`ORG.…`), a function the calculator doesn't know, a name
the file didn't define, or a sheet that had to be renamed on import. A warning says so.
Unsupported or missing values retain available display text and produce a warning.

Checked on LibreOffice-written files (`gridcraft-cli convert` then `gridcraft-cli check`): every
kept formula recalculates to LibreOffice's saved result. Source formatting (beyond number formats),
charts, images and other workbook features are still not imported. Plunder Ledger does not write
ODS files.

Save opens Save As and suggests a new `.xlsx` file. The source ODS file is not used as the
workbook's save destination; requests to save to `.ods` return an error.

```sh
gridcraft-cli info source.ods
gridcraft-cli convert source.ods imported.xlsx
```

CLI import warnings go to stderr, leaving structured output on stdout available for scripts.
