//! `.xls` chart import: chart substreams ([MS-XLS] 2.2.3) embedded in a worksheet after their
//! Obj record (2.4.181) and MsoDrawing anchor (2.4.168), and chart sheets (BoundSheet8 `dt` = 2).

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{ChartKind, Color, LegendPos, Workbook};

use crate::{read_xls, read_xlsx, write_xlsx};

use super::xls_tests::compound;

fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &cat(&[&u16s(&[0x0600, dt, 0x0DBB, 0x07CC]), &[0; 8]]))
}

fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

fn begin() -> Vec<u8> {
    rec(0x1033, &[])
}

fn end() -> Vec<u8> {
    rec(0x1034, &[])
}

/// A workbook of `(name, BoundSheet8 dt, substream)` sheets; every formula's ixti 0 is the first sheet.
fn book(sheets: &[(&str, u8, Vec<u8>)]) -> Vec<u8> {
    let globals = |pos: &[u32]| {
        let mut g = bof(0x0005);
        for ((name, dt, _), p) in sheets.iter().zip(pos) {
            g.extend(rec(0x0085, &cat(&[&p.to_le_bytes(), &[0, *dt, name.len() as u8, 0], name.as_bytes()])));
        }
        g.extend(rec(0x01AE, &u16s(&[sheets.len() as u16, 0x0401])));
        g.extend(rec(0x0017, &cat(&[&u16s(&[2]), &u16s(&[0, 0, 0]), &u16s(&[0, 1, 1])])));
        g.extend(eof());
        g
    };
    let len = globals(&vec![0; sheets.len()]).len() as u32;
    let mut pos = vec![];
    let mut at = len;
    for (_, _, body) in sheets {
        pos.push(at);
        at += body.len() as u32;
    }
    let mut stream = globals(&pos);
    for (_, _, body) in sheets {
        stream.extend_from_slice(body);
    }
    compound("Workbook", &stream)
}

/// PtgArea3d through ixti 0 (Sheet1), absolute: rows and columns zero-based.
fn area(r1: u16, r2: u16, c1: u16, c2: u16) -> Vec<u8> {
    cat(&[&[0x3B], &u16s(&[0, r1, r2, c1, c2])])
}

/// PtgRef3d through ixti 0 (Sheet1), absolute.
fn cell3d(r: u16, c: u16) -> Vec<u8> {
    cat(&[&[0x3A], &u16s(&[0, r, c])])
}

/// BRAI: `id` (0 name, 1 values, 2 categories) referring to `rgce`.
fn brai(id: u8, rgce: &[u8]) -> Vec<u8> {
    let rt = if rgce.is_empty() { 1 } else { 2 };
    rec(0x1051, &cat(&[&[id, rt], &u16s(&[0, 0, rgce.len() as u16]), rgce]))
}

fn series_text(s: &str) -> Vec<u8> {
    rec(0x100D, &cat(&[&u16s(&[0]), &[s.len() as u8, 0], s.as_bytes()]))
}

/// A series with name, values and categories in column `col` (rows 2-5, names in row 1,
/// categories in column A), drawn by chart group 0, plus `extra` records in its block.
fn series(col: u16, extra: &[u8]) -> Vec<u8> {
    cat(&[
        &rec(0x1003, &u16s(&[1, 1, 4, 4, 1, 0])),
        &begin(),
        &brai(0, &cell3d(0, col)),
        &brai(1, &area(1, 4, col, col)),
        &brai(2, &area(1, 4, 0, 0)),
        &brai(3, &[]),
        extra,
        &rec(0x1045, &u16s(&[0])),
        &end(),
    ])
}

/// A whole-series DataFormat holding `formats`.
fn data_format(formats: &[u8]) -> Vec<u8> {
    cat(&[&rec(0x1006, &u16s(&[0xFFFF, 0, 0, 0])), &begin(), formats, &end()])
}

/// A Text block linked to `link` (1 chart title, 2 value axis, 3 category axis).
fn text(link: u16, s: &str) -> Vec<u8> {
    cat(&[&rec(0x1025, &[0; 32]), &begin(), &rec(0x1027, &u16s(&[link, 0, 0])), &series_text(s), &end()])
}

/// Legend at the bottom: horizontal, centred low in the chart area.
fn legend_bottom() -> Vec<u8> {
    rec(0x1015, &cat(&[&u32s(&[1500, 3600, 1000, 300]), &[0, 1], &u16s(&[0x0001])]))
}

fn bar(flags: u16) -> Vec<u8> {
    rec(0x1017, &u16s(&[0, 150, flags]))
}

/// A chart substream: the series, title, then one axis parent with a value axis and the group.
fn chart(series: &[u8], group: &[u8], extra: &[u8]) -> Vec<u8> {
    cat(&[
        &bof(0x0020),
        &rec(0x1002, &u32s(&[0, 0, 360 << 16, 216 << 16])),
        &begin(),
        series,
        extra,
        &rec(0x1041, &cat(&[&u16s(&[0]), &[0; 16]])),
        &begin(),
        &rec(0x101D, &cat(&[&u16s(&[0]), &[0; 16]])),
        &begin(),
        &end(),
        &rec(0x101D, &cat(&[&u16s(&[1]), &[0; 16]])),
        &begin(),
        // ValueRange: min 0, max 100, major 25, automatic minor and crossing.
        &rec(
            0x101F,
            &cat(&[&0f64.to_le_bytes(), &100f64.to_le_bytes(), &25f64.to_le_bytes(), &0f64.to_le_bytes(), &0f64.to_le_bytes(), &u16s(&[0x0018])]),
        ),
        &rec(0x1021, &u16s(&[1])),
        &end(),
        &rec(0x1014, &cat(&[&[0; 16], &u16s(&[0, 0])])),
        &begin(),
        group,
        &legend_bottom(),
        &end(),
        &end(),
        &end(),
        &eof(),
    ])
}

/// MsoDrawing holding one shape anchored from (col 4, row 1) to (col 10, row 15).
fn drawing() -> Vec<u8> {
    let mut atom = vec![0x00, 0x00, 0x10, 0xF0, 18, 0, 0, 0];
    atom.extend(u16s(&[0, 4, 0, 1, 0, 10, 0, 15, 0]));
    let mut sp = vec![0x0F, 0x00, 0x04, 0xF0];
    sp.extend((atom.len() as u32).to_le_bytes());
    sp.extend(atom);
    rec(0x00EC, &sp)
}

/// Obj with FtCmo `ot` and FtEnd.
fn obj(ot: u16) -> Vec<u8> {
    rec(0x005D, &cat(&[&u16s(&[0x15, 0x12, ot, 1, 0]), &[0; 12], &u16s(&[0, 0])]))
}

/// Sheet1 cells: A2:A5 labels as numbers 1-4, B1/C1 headers absent, B2:C5 numbers.
fn cells() -> Vec<u8> {
    let mut v = vec![];
    for r in 1..5u16 {
        for c in 0..3u16 {
            v.extend(rec(0x0203, &cat(&[&u16s(&[r, c, 0]), &f64::from(r * 10 + c).to_le_bytes()])));
        }
    }
    v
}

fn worksheet(charts: &[u8]) -> Vec<u8> {
    cat(&[&bof(0x0010), &cells(), charts, &eof()])
}

fn embedded(chart_stream: &[u8]) -> Vec<u8> {
    book(&[("Sheet1", 0, worksheet(&cat(&[&drawing(), &obj(5), chart_stream])))])
}

fn column_chart() -> Vec<u8> {
    let two = cat(&[&series(1, &[]), &series(2, &[])]);
    chart(&two, &bar(0), &text(1, "Quarterly sales"))
}

fn read(file: &[u8]) -> (Workbook, Vec<String>) {
    let (wb, report) = read_xls(file).unwrap();
    (wb, report.warnings)
}

#[test]
fn xls_embedded_column_chart_reads_series_title_legend_and_anchor() {
    let (wb, warnings) = read(&embedded(&column_chart()));
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.charts.len(), 1, "{warnings:?}");
    let ch = &sheet.charts[0];
    assert_eq!(ch.kind, ChartKind::ColumnClustered);
    assert_eq!(ch.title.as_deref(), Some("Quarterly sales"));
    assert_eq!(ch.legend, LegendPos::Bottom);
    assert_eq!(ch.anchor.cell, CellRef::new(1, 4));
    assert!(ch.anchor.width > 0.0 && ch.anchor.height > 0.0);
    assert_eq!(ch.series.len(), 2);
    assert_eq!(ch.series[0].name.as_deref(), Some("Sheet1!$B$1"));
    assert_eq!(ch.series[0].values, "Sheet1!$B$2:$B$5");
    assert_eq!(ch.series[0].categories.as_deref(), Some("Sheet1!$A$2:$A$5"));
    assert_eq!(ch.series[1].values, "Sheet1!$C$2:$C$5");
    assert_eq!(ch.scale.y.max, Some(100.0));
    assert_eq!(ch.scale.y.major_unit, Some(25.0));
    assert_eq!(ch.scale.y.minor_unit, None);
    assert!(ch.gridlines);
    assert!(ch.id > 0);
    assert!(!warnings.iter().any(|w| w.contains("skipped")), "{warnings:?}");
    // The cells are untouched.
    assert_eq!(sheet.cell(CellRef::new(1, 1)).map(|c| c.value.clone()), Some(Value::number(11.0)));
}

fn kind_of(group: &[u8], series_extra: &[u8]) -> ChartKind {
    let (wb, warnings) = read(&embedded(&chart(&series(1, series_extra), group, &[])));
    wb.sheets[0].charts.first().unwrap_or_else(|| panic!("no chart: {warnings:?}")).kind
}

#[test]
fn xls_chart_group_types_map_to_kinds() {
    assert_eq!(kind_of(&bar(0x0001), &[]), ChartKind::BarClustered);
    assert_eq!(kind_of(&bar(0x0003), &[]), ChartKind::BarStacked);
    assert_eq!(kind_of(&bar(0x0007), &[]), ChartKind::BarStacked100);
    assert_eq!(kind_of(&bar(0x0002), &[]), ChartKind::ColumnStacked);
    assert_eq!(kind_of(&bar(0x0006), &[]), ChartKind::ColumnStacked100);
    let line = rec(0x1018, &u16s(&[0]));
    assert_eq!(kind_of(&line, &[]), ChartKind::Line);
    // MarkerFormat: automatic markers.
    let markers = data_format(&rec(0x1009, &cat(&[&[0; 8], &u16s(&[2, 0x0001, 0, 0]), &[0; 4]])));
    assert_eq!(kind_of(&line, &markers), ChartKind::LineMarkers);
    assert_eq!(kind_of(&rec(0x1018, &u16s(&[1])), &[]), ChartKind::LineStacked);
    assert_eq!(kind_of(&rec(0x1019, &u16s(&[0, 0, 0])), &[]), ChartKind::Pie);
    assert_eq!(kind_of(&rec(0x1019, &u16s(&[0, 50, 0])), &[]), ChartKind::Doughnut);
    assert_eq!(kind_of(&rec(0x101A, &u16s(&[0])), &[]), ChartKind::Area);
    assert_eq!(kind_of(&rec(0x101A, &u16s(&[1])), &[]), ChartKind::AreaStacked);
    assert_eq!(kind_of(&rec(0x101A, &u16s(&[3])), &[]), ChartKind::AreaStacked100);
    let scatter = rec(0x101B, &u16s(&[100, 1, 0]));
    // LineFormat with no line (lns 5): markers only.
    let no_line = data_format(&rec(0x1007, &cat(&[&[0; 4], &u16s(&[5, 0, 0, 0])])));
    assert_eq!(kind_of(&scatter, &no_line), ChartKind::Scatter);
    let red_line = data_format(&rec(0x1007, &cat(&[&[0xFF, 0, 0, 0], &u16s(&[0, 0, 0, 0])])));
    assert_eq!(kind_of(&scatter, &red_line), ChartKind::ScatterLines);
    assert_eq!(kind_of(&rec(0x101B, &u16s(&[100, 1, 1])), &[]), ChartKind::Bubble);
    assert_eq!(kind_of(&rec(0x103E, &u16s(&[0, 0])), &[]), ChartKind::Radar);
}

#[test]
fn xls_chart_series_colour_that_is_not_automatic() {
    // AreaFormat: solid (fls 1) green fill, not automatic.
    let fill = data_format(&rec(0x100A, &cat(&[&[0, 0x80, 0, 0], &[0xFF; 4], &u16s(&[1, 0, 0, 0])])));
    let (wb, _) = read(&embedded(&chart(&series(1, &fill), &bar(0), &[])));
    assert_eq!(wb.sheets[0].charts[0].series[0].color, Some(Color::rgb(0, 0x80, 0)));
    // Automatic: no colour.
    let auto = data_format(&rec(0x100A, &cat(&[&[0, 0x80, 0, 0], &[0xFF; 4], &u16s(&[1, 1, 0, 0])])));
    let (wb, _) = read(&embedded(&chart(&series(1, &auto), &bar(0), &[])));
    assert_eq!(wb.sheets[0].charts[0].series[0].color, None);
}

#[test]
fn xls_unsupported_chart_type_is_skipped_with_a_warning() {
    let surface = rec(0x103F, &u16s(&[0]));
    let (wb, warnings) = read(&embedded(&chart(&series(1, &[]), &surface, &[])));
    assert!(wb.sheets[0].charts.is_empty());
    assert!(warnings.iter().any(|w| w.contains("surface")), "{warnings:?}");
}

#[test]
fn xls_chart_sheet_is_read_and_kept_when_saved_as_xlsx() {
    let body = column_chart();
    let file = book(&[("Sheet1", 0, worksheet(&[])), ("Chart1", 2, body)]);
    let (wb, warnings) = read(&file);
    assert_eq!(wb.sheets.len(), 1);
    assert_eq!(wb.chart_sheets.len(), 1, "{warnings:?}");
    let cs = &wb.chart_sheets[0];
    assert_eq!(cs.name, "Chart1");
    assert_eq!(cs.before, 1);
    assert_eq!(cs.chart.series.len(), 2);
    assert_eq!(cs.chart.series[1].values, "Sheet1!$C$2:$C$5");

    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert_eq!(back.chart_sheets.len(), 1);
    let cs = &back.chart_sheets[0];
    assert_eq!(cs.name, "Chart1");
    assert_eq!(cs.chart.kind, ChartKind::ColumnClustered);
    assert_eq!(cs.chart.title.as_deref(), Some("Quarterly sales"));
    assert_eq!(cs.chart.series[0].values, "Sheet1!$B$2:$B$5");
    assert_eq!(cs.chart.series[0].categories.as_deref(), Some("Sheet1!$A$2:$A$5"));
}

#[test]
fn xls_damaged_chart_is_skipped_and_the_cells_kept() {
    // A BRAI record cut short inside a series.
    let broken_series = cat(&[&rec(0x1003, &u16s(&[1, 1, 4, 4, 1, 0])), &begin(), &rec(0x1051, &[1, 2]), &end()]);
    let damaged = chart(&broken_series, &bar(0), &[]);
    // Garbage: no chart group at all, unbalanced End records and a stray truncated Chart record.
    let garbage = cat(&[&bof(0x0020), &rec(0x1002, &[1, 2, 3]), &end(), &end(), &rec(0x1003, &[]), &eof()]);
    for bad in [damaged, garbage] {
        let file = embedded(&cat(&[&bad, &drawing(), &obj(5), &column_chart()]));
        let (wb, warnings) = read(&file);
        let sheet = &wb.sheets[0];
        assert_eq!(sheet.charts.len(), 1, "the good chart after it is kept: {warnings:?}");
        assert!(warnings.iter().any(|w| w.contains("chart") && w.contains("skipped")), "{warnings:?}");
        assert_eq!(sheet.cell(CellRef::new(4, 2)).map(|c| c.value.clone()), Some(Value::number(42.0)));
    }
    // A chart substream cut off before its EOF at the end of the stream.
    let cut = column_chart();
    let file = book(&[("Sheet1", 0, cat(&[&bof(0x0010), &cells(), &drawing(), &obj(5), &cut[..cut.len() / 2]]))]);
    let (wb, warnings) = read(&file);
    assert!(warnings.iter().any(|w| w.contains("cut off")), "{warnings:?}");
    assert!(wb.sheets[0].charts.is_empty());
    assert_eq!(wb.sheets[0].cell(CellRef::new(4, 2)).map(|c| c.value.clone()), Some(Value::number(42.0)));
}

#[test]
fn xls_embedded_chart_survives_saving_as_xlsx() {
    let (wb, _) = read(&embedded(&chart(&series(1, &[]), &bar(0x0003), &[])));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    let ch = &back.sheets[0].charts[0];
    assert_eq!(ch.kind, ChartKind::BarStacked);
    assert_eq!(ch.series[0].values, "Sheet1!$B$2:$B$5");
    assert_eq!(ch.series[0].categories.as_deref(), Some("Sheet1!$A$2:$A$5"));
    assert_eq!(ch.series[0].name.as_deref(), Some("Sheet1!$B$1"));
    assert_eq!(ch.anchor.cell, CellRef::new(1, 4));
}

#[test]
fn xls_chart_literal_values_come_from_the_cache() {
    // A series with no references: its values are the cached numbers (SIIndex 1, column 0).
    let ser = cat(&[&rec(0x1003, &u16s(&[1, 1, 3, 3, 1, 0])), &begin(), &series_text("Fixed"), &end()]);
    let cache = cat(&[
        &rec(0x1065, &u16s(&[1])),
        &rec(0x0203, &cat(&[&u16s(&[1, 0, 0]), &2.5f64.to_le_bytes()])),
        &rec(0x0203, &cat(&[&u16s(&[0, 0, 0]), &1f64.to_le_bytes()])),
    ]);
    let (wb, warnings) = read(&embedded(&chart(&ser, &bar(0), &cache)));
    let ch = wb.sheets[0].charts.first().unwrap_or_else(|| panic!("{warnings:?}"));
    assert_eq!(ch.series[0].values, "{1,2.5}");
    assert_eq!(ch.series[0].name.as_deref(), Some("Fixed"));
}

/// AttachedLabel ([MS-XLS] 2.4.10) with `flags`.
fn attached_label(flags: u16) -> Vec<u8> {
    rec(0x100C, &u16s(&[flags]))
}

fn labelled(series_extra: &[u8]) -> (Workbook, Vec<String>) {
    read(&embedded(&chart(&series(1, series_extra), &bar(0), &[])))
}

#[test]
fn xls_chart_series_value_labels_are_read() {
    let (wb, warnings) = labelled(&data_format(&attached_label(0x0001)));
    assert!(wb.sheets[0].charts.first().unwrap_or_else(|| panic!("{warnings:?}")).data_labels);
    // Percent only: no value labels.
    let (wb, _) = labelled(&data_format(&attached_label(0x0002)));
    assert!(!wb.sheets[0].charts[0].data_labels);
    // A label on one data point (xi 0) only.
    let point = cat(&[&rec(0x1006, &u16s(&[0, 0, 0, 0])), &begin(), &attached_label(0x0001), &end()]);
    let (wb, _) = labelled(&point);
    assert!(!wb.sheets[0].charts[0].data_labels);
    // An empty payload: the chart is still read, without labels.
    let (wb, warnings) = labelled(&data_format(&rec(0x100C, &[])));
    assert!(!wb.sheets[0].charts.first().unwrap_or_else(|| panic!("{warnings:?}")).data_labels);
    // No AttachedLabel.
    let (wb, _) = read(&embedded(&column_chart()));
    assert!(!wb.sheets[0].charts[0].data_labels);
}

#[test]
fn xls_chart_value_labels_survive_saving_as_xlsx() {
    let (wb, _) = labelled(&data_format(&attached_label(0x0001)));
    let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
    assert!(back.sheets[0].charts[0].data_labels);
}
