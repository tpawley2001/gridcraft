//! Synthetic BIFF12 arrays and memory caches from [MS-XLSB] 2.5.98.
use super::xlsb_tests::{
    RELS, ROOT, TYPES, book, cell, file, ftext, package, ptg_area, ptg_exp, ptg_int, ptg_ref_n, record, rfx, row, sst, wide, worksheet,
};
use crate::{read_xlsb, read_xlsx, write_xlsx};
use gridcraft_core::{CellRef, Value};

fn array_ptg() -> Vec<u8> {
    [vec![0x60], vec![0; 14]].concat()
}
fn array(rows: u32, cols: u32, values: &[Vec<u8>]) -> Vec<u8> {
    [rows.to_le_bytes().to_vec(), cols.to_le_bytes().to_vec(), values.concat()].concat()
}
fn num(n: f64) -> Vec<u8> {
    [vec![0], n.to_le_bytes().to_vec()].concat()
}
fn string(s: &str) -> Vec<u8> {
    let mut out = vec![1];
    out.extend((s.encode_utf16().count() as u16).to_le_bytes());
    for u in s.encode_utf16() {
        out.extend(u.to_le_bytes());
    }
    out
}
fn parsed(rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    [(rgce.len() as u32).to_le_bytes().to_vec(), rgce.to_vec(), (rgcb.len() as u32).to_le_bytes().to_vec(), rgcb.to_vec()].concat()
}
fn fmla(col: u32, rgce: &[u8], rgcb: &[u8]) -> Vec<u8> {
    record(9, &[col.to_le_bytes().to_vec(), vec![0; 4], 99.0f64.to_le_bytes().to_vec(), vec![0; 2], parsed(rgce, rgcb)].concat())
}
fn sum_array() -> Vec<u8> {
    [array_ptg(), vec![0x22, 1, 4, 0]].concat()
}

#[test]
fn xlsb_array_constants_keep_all_value_types_and_formula_text() {
    let cases = [
        (sum_array(), array(1, 3, &[num(1.0), num(2.0), num(3.0)]), "SUM({1,2,3})"),
        ([array_ptg(), ptg_int(2), vec![0x22, 2, 29, 0]].concat(), array(1, 3, &[num(10.0), num(20.0), num(30.0)]), "INDEX({10,20,30},2)"),
        (
            [vec![0x17, 1, 0, b'b', 0], array_ptg(), ptg_int(2), vec![0x1D, 0, 0x22, 4, 102, 0]].concat(),
            array(2, 2, &[string("a"), num(1.0), string("b"), num(2.0)]),
            "VLOOKUP(\"b\",{\"a\",1;\"b\",2},2,FALSE)",
        ),
        ([array_ptg(), vec![0x22, 1, 36, 0]].concat(), array(1, 2, &[vec![2, 1], vec![2, 0]]), "AND({TRUE,FALSE})"),
        ([array_ptg(), ptg_int(1), vec![0x22, 2, 29, 0, 0x21, 2, 0]].concat(), array(1, 1, &[vec![4, 0x2A, 0, 0, 0]]), "ISNA(INDEX({#N/A},1))"),
    ];
    for (rgce, rgcb, text) in cases {
        let data = [row(0), fmla(0, &rgce, &rgcb)].concat();
        let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
        assert_eq!(ftext(&wb, "A1").as_deref(), Some(text));
        assert!(!report.warnings.iter().any(|w| w.contains("kept as their saved results")));
        let (back, _) = read_xlsx(&write_xlsx(&wb).unwrap()).unwrap();
        assert_eq!(ftext(&back, "A1").as_deref(), Some(text));
    }
}

#[test]
fn xlsb_shared_and_array_groups_carry_extra_data() {
    for is_array in [false, true] {
        let rgce = if is_array { sum_array() } else { [ptg_ref_n(0, 1, true, true), array_ptg(), vec![0x22, 2, 4, 0]].concat() };
        let rgcb = array(1, 2, &[num(1.0), num(2.0)]);
        let mut group = rfx(0, 2, 0, 0);
        if is_array {
            group.push(0);
        }
        group.extend(parsed(&rgce, &rgcb));
        let mut data = vec![];
        for r in 0..3 {
            data.extend(row(r));
            data.extend(fmla(0, &ptg_exp(0), &0u32.to_le_bytes()));
            if r == 0 {
                data.extend(record(if is_array { 426 } else { 427 }, &group));
            }
            data.extend(cell(5, 1, &f64::from(r + 1).to_le_bytes()));
        }
        let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
        for r in 1..=3 {
            let expected = if is_array { "SUM({1,2})".to_owned() } else { format!("SUM(B{r},{{1,2}})") };
            assert_eq!(ftext(&wb, &format!("A{r}")), Some(expected));
        }
        assert!(!report.warnings.iter().any(|w| w.contains("kept as their saved results")));
    }
}

#[test]
fn xlsb_defined_array_name_stays_live() {
    let name = [
        vec![0; 5],
        u32::MAX.to_le_bytes().to_vec(),
        wide("Consts"),
        parsed(&array_ptg(), &array(1, 3, &[num(1.0), num(2.0), num(3.0)])),
        u32::MAX.to_le_bytes().to_vec(),
    ]
    .concat();
    let mut workbook = book(false);
    workbook.splice(workbook.len() - record(132, &[]).len().., [record(39, &name), record(132, &[])].concat());
    let data = [row(0), fmla(0, &[0x43, 1, 0, 0, 0, 0x22, 1, 4, 0], &[])].concat();
    let (wb, report) = read_xlsb(&package(&workbook, &worksheet(&data, &[]), &sst(), ROOT, RELS, TYPES)).unwrap();
    assert_eq!(wb.names.iter().find(|n| n.name == "Consts").unwrap().formula, "{1,2,3}");
    assert_eq!(ftext(&wb, "A1").as_deref(), Some("SUM(Consts)"));
    assert!(!report.warnings.iter().any(|w| w.contains("kept as their saved results")));
}

#[test]
fn xlsb_mem_area_classes_skip_their_biff12_cache() {
    for ptg in [0x26, 0x46, 0x66] {
        let rgce = [vec![ptg, 0, 0, 0, 0, 27, 0], ptg_area(0, 1, 0, 1), ptg_area(1, 2, 1, 2), vec![0x0F, 0x22, 1, 4, 0]].concat();
        let rgcb = [1u32.to_le_bytes().to_vec(), rfx(1, 1, 1, 1)].concat();
        let data = [row(0), fmla(0, &rgce, &rgcb), row(1), cell(5, 1, &7.0f64.to_le_bytes())].concat();
        let (wb, _) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
        assert_eq!(ftext(&wb, "A1").as_deref(), Some("SUM(A1:B2 B2:C3)"));
    }
}

#[test]
fn xlsb_malformed_arrays_keep_caches_and_warn_once() {
    let bad = [
        array(0, 1, &[]),
        array(1, 0, &[]),
        array(1_048_577, 1, &[]),
        array(1, 16_385, &[]),
        array(256, 257, &[]),
        array(u32::MAX, u32::MAX, &[]),
        array(1, 1, &[vec![0, 1]]),
        array(1, 1, &[vec![3]]),
        array(1, 1, &[num(f64::INFINITY)]),
        array(1, 1, &[vec![4, 0x2A]]),
    ];
    let mut data = row(0);
    for (col, rgcb) in bad.iter().enumerate() {
        data.extend(fmla(col as u32, &sum_array(), rgcb));
    }
    let (wb, report) = read_xlsb(&file(&worksheet(&data, &[]))).unwrap();
    for col in 0..bad.len() {
        let cell = wb.sheets[0].cell(CellRef::new(0, col as u32)).unwrap();
        assert!(cell.formula.is_none());
        assert_eq!(cell.value, Value::Number(99.0));
    }
    assert_eq!(report.warnings.iter().filter(|w| w.contains("kept as their saved results")).count(), 1);
}
