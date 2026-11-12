//! Cube-formula (MDX) value metadata is kept on save: the `XLMDX` metadata type, `metadataStrings`
//! and `mdxMetadata` of `xl/metadata.xml` (ECMA-376 Part 1 §18.9), and each cell's `vm`, which must
//! still resolve to the MDX record it pointed at. Microsoft 365 Excel keeps all of it through its own
//! open + save. The chain is resolved here rather than compared by index: `vm` (one-based) ->
//! `valueMetadata/bk` -> `rc t` (one-based metadata type) and `rc v` (zero-based `mdx` record).

use std::collections::BTreeMap;

use gridcraft_core::{CellRef, Value};
use gridcraft_model::{Cell, Formula};

use crate::package::Package;
use crate::xml::El;
use crate::{read_xlsx, write_xlsx};

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const MDX_TYPE: &str = r#"<metadataType name="XLMDX" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1"/>"#;
const STRINGS: [&str; 5] = ["Sales cube", "[Measures].[Profit]", "[Store].[All Stores]", "{[Store].[All Stores].[USA]}", "Caption"];
/// The MDX records, in file order: member, member, value, set, property.
const RECORDS: [&str; 5] = [
    r#"<mdx n="0" f="m"><t c="1"><n x="1"/></t></mdx>"#,
    r#"<mdx n="0" f="m"><t c="1"><n x="2"/></t></mdx>"#,
    r#"<mdx n="0" f="v"><t c="2" ct="en-US"><n x="2"/><n x="1"/></t></mdx>"#,
    r#"<mdx n="0" f="s"><ms ns="3" c="0"/></mdx>"#,
    r#"<mdx n="0" f="p"><p n="2" np="4"/></mdx>"#,
];
/// `valueMetadata` blocks, deliberately not in record order: block i (zero-based) -> record.
const BLOCKS: [usize; 5] = [2, 0, 3, 1, 4];
/// Cell -> the record its `vm` resolves to in the source.
const CELLS: [(&str, usize); 5] = [("B1", 0), ("B2", 1), ("B3", 2), ("B4", 3), ("B5", 4)];
const RELS: &str = r#"<Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sheetMetadata" Target="metadata.xml"/>"#;
const SST: &str = r#"<?xml version="1.0"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="1" uniqueCount="1"><si><t>USA</t></si></sst>"#;

fn at(s: &str) -> CellRef {
    CellRef::parse(s).unwrap()
}

/// `vm` of the block that points at `record` (one-based).
fn vm(record: usize) -> usize {
    BLOCKS.iter().position(|r| *r == record).unwrap() + 1
}

/// The source workbook. With `dynamic`, the metadata also has an `XLDAPR` type AFTER `XLMDX` and
/// D1 is a dynamic-array formula (`cm`), so type numbers cannot be assumed.
fn source(dynamic: bool) -> Vec<u8> {
    let strings: String = STRINGS.iter().map(|s| format!(r#"<s v="{s}"/>"#)).collect();
    let blocks: String = BLOCKS.iter().map(|v| format!(r#"<bk><rc t="1" v="{v}"/></bk>"#)).collect();
    let (dynamic_type, dynamic_records, types) = if dynamic {
        (
            r#"<metadataType name="XLDAPR" minSupportedVersion="120000" copy="1" pasteAll="1" pasteValues="1" merge="1" splitFirst="1" rowColShift="1" clearFormats="1" clearComments="1" assign="1" coerce="1" cellMeta="1"/>"#,
            r#"<futureMetadata name="XLDAPR" count="1"><bk><extLst><ext uri="{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}"><xda:dynamicArrayProperties fDynamic="1" fCollapsed="0"/></ext></extLst></bk></futureMetadata><cellMetadata count="1"><bk><rc t="2" v="0"/></bk></cellMetadata>"#,
            2,
        )
    } else {
        ("", "", 1)
    };
    let metadata = format!(
        r#"<?xml version="1.0"?><metadata xmlns="{NS}" xmlns:xda="http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray"><metadataTypes count="{types}">{MDX_TYPE}{dynamic_type}</metadataTypes><metadataStrings count="{}">{strings}</metadataStrings><mdxMetadata count="{}">{}</mdxMetadata>{dynamic_records}<valueMetadata count="{}">{blocks}</valueMetadata></metadata>"#,
        STRINGS.len(),
        RECORDS.len(),
        RECORDS.concat(),
        BLOCKS.len(),
    );
    let d1 = if dynamic { r#"<c r="D1" cm="1"><f t="array" ref="D1">SUMPRODUCT(C1:C3*C1:C3)</f><v>49</v></c>"# } else { "" };
    let body = format!(
        r#"<sheetData><row r="1"><c r="B1" t="str" vm="{}"><f>CUBEMEMBER("Sales cube","[Measures].[Profit]")</f><v>Profit</v></c><c r="C1"><v>7</v></c>{d1}</row><row r="2"><c r="B2" t="str" vm="{}"><f>CUBEMEMBER("Sales cube","[Store].[All Stores]")</f><v>All Stores</v></c></row><row r="3"><c r="B3" vm="{}"><f>CUBEVALUE("Sales cube",B2,B1)</f><v>1234.5</v></c></row><row r="4"><c r="B4" t="s" vm="{}"><v>0</v></c></row><row r="5"><c r="B5" t="str" vm="{}"><f>CUBEMEMBERPROPERTY("Sales cube",B2,"Caption")</f><v>All</v></c></row></sheetData>"#,
        vm(0),
        vm(1),
        vm(2),
        vm(3),
        vm(4),
    );
    super::minimal(&body, &[("xl/sharedStrings.xml", SST), ("xl/metadata.xml", &metadata)], RELS, "")
}

/// An element as text: name, attributes sorted by name, children in order.
fn dump(el: &El) -> String {
    let mut attrs: Vec<String> = el.attrs.iter().filter(|(k, _)| !k.starts_with("xmlns")).map(|(k, v)| format!("{k}={v}")).collect();
    attrs.sort();
    let children: Vec<String> = el.children.iter().map(dump).collect();
    format!("<{} {}>[{}]", el.name, attrs.join(" "), children.join(""))
}

fn record(n: usize) -> String {
    dump(&crate::xml::parse(RECORDS[n].as_bytes()).unwrap())
}

/// What a saved package says about its metadata.
struct Saved {
    metadata: Option<El>,
    /// Cell address -> (its `vm`, whether it has `<f>`), for every cell written.
    cells: BTreeMap<String, (Option<usize>, bool)>,
    /// Cell address -> its `cm`.
    cm: BTreeMap<String, usize>,
}

fn saved(bytes: &[u8]) -> Saved {
    let mut pkg = Package::open(bytes).unwrap();
    let sheet = pkg.read_xml("xl/worksheets/sheet1.xml").unwrap().unwrap();
    let mut cells = BTreeMap::new();
    let mut cm = BTreeMap::new();
    for c in sheet.child("sheetData").unwrap().kids("row").flat_map(|r| r.kids("c")) {
        let address = c.attr("r").unwrap().to_string();
        if let Some(n) = c.attr("cm") {
            cm.insert(address.clone(), n.parse().unwrap());
        }
        cells.insert(address, (c.attr("vm").map(|n| n.parse().unwrap()), c.child("f").is_some()));
    }
    let metadata = pkg.read_xml("xl/metadata.xml").unwrap();
    if metadata.is_some() {
        assert!(pkg.rels("xl/workbook.xml").unwrap().iter().any(|r| r.kind == "sheetMetadata" && r.target == "xl/metadata.xml"));
        let types = pkg.read_xml("[Content_Types].xml").unwrap().unwrap();
        assert!(types.kids("Override").any(|e| e.attr("PartName") == Some("/xl/metadata.xml")
            && e.attr("ContentType") == Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml")));
    }
    Saved { metadata, cells, cm }
}

impl Saved {
    fn metadata(&self) -> &El {
        self.metadata.as_ref().expect("xl/metadata.xml is written")
    }

    fn type_names(&self) -> Vec<&str> {
        self.metadata().child("metadataTypes").unwrap().kids("metadataType").map(|t| t.attr("name").unwrap()).collect()
    }

    /// The MDX record the cell's `vm` resolves to, as [`dump`] text. `None`: the cell has no `vm`.
    fn mdx_record(&self, address: &str) -> Option<String> {
        let vm = self.cells.get(address).unwrap_or_else(|| panic!("{address} is written")).0?;
        let metadata = self.metadata();
        let block = metadata.child("valueMetadata").unwrap().kids("bk").nth(vm.checked_sub(1).expect("vm is one-based"));
        let rc = block.unwrap_or_else(|| panic!("{address}: vm {vm} is past the value metadata blocks")).child("rc").unwrap();
        let type_name = self.type_names()[rc.attr_u32("t").unwrap() as usize - 1];
        assert_eq!(type_name, "XLMDX", "{address}: vm {vm} points at another metadata type");
        let n = rc.attr_u32("v").unwrap() as usize;
        Some(dump(metadata.child("mdxMetadata").unwrap().kids("mdx").nth(n).unwrap_or_else(|| panic!("{address}: no mdx record {n}"))))
    }

    /// Counts match their children, children follow the schema order, every index is in range.
    fn assert_consistent(&self) {
        let metadata = self.metadata();
        const ORDER: [&str; 7] = ["metadataTypes", "metadataStrings", "mdxMetadata", "futureMetadata", "cellMetadata", "valueMetadata", "extLst"];
        let positions: Vec<usize> =
            metadata.children.iter().map(|c| ORDER.iter().position(|n| *n == c.name).unwrap_or_else(|| panic!("unexpected <{}>", c.name))).collect();
        assert!(positions.windows(2).all(|w| w[0] <= w[1]), "children of <metadata> out of schema order: {positions:?}");
        for (list, item) in
            [("metadataTypes", "metadataType"), ("metadataStrings", "s"), ("mdxMetadata", "mdx"), ("cellMetadata", "bk"), ("valueMetadata", "bk")]
        {
            if let Some(el) = metadata.child(list) {
                assert_eq!(el.attr_u32("count"), Some(el.kids(item).count() as u32), "{list} count");
            }
        }
        assert_eq!(self.type_names().iter().filter(|n| **n == "XLMDX").count(), 1);
        let types = self.type_names().len() as u32;
        let records = metadata.child("mdxMetadata").unwrap().kids("mdx").count() as u32;
        for list in ["cellMetadata", "valueMetadata"] {
            for rc in metadata.child(list).into_iter().flat_map(|l| l.kids("bk")).flat_map(|b| b.kids("rc")) {
                let t = rc.attr_u32("t").unwrap();
                assert!((1..=types).contains(&t), "{list} rc t={t}");
                if self.type_names()[t as usize - 1] == "XLMDX" {
                    assert!(rc.attr_u32("v").unwrap() < records, "{list} rc v past the mdx records");
                }
            }
        }
    }

    fn assert_mdx_kept(&self) {
        self.assert_consistent();
        let metadata = self.metadata();
        let mdx_type = metadata.child("metadataTypes").unwrap().kids("metadataType").find(|t| t.attr("name") == Some("XLMDX")).unwrap();
        assert_eq!(dump(mdx_type), dump(&crate::xml::parse(MDX_TYPE.as_bytes()).unwrap()));
        let strings: Vec<&str> = metadata.child("metadataStrings").expect("metadataStrings kept").kids("s").map(|s| s.attr("v").unwrap()).collect();
        assert_eq!(strings, STRINGS);
        let records: Vec<String> = metadata.child("mdxMetadata").expect("mdxMetadata kept").kids("mdx").map(dump).collect();
        assert_eq!(records, (0..RECORDS.len()).map(record).collect::<Vec<_>>());
    }
}

#[test]
fn mdx_metadata_and_every_cell_link_survive_save() {
    let (wb, report) = read_xlsx(&source(false)).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("metadata")), "{:?}", report.warnings);
    let bytes = write_xlsx(&wb).unwrap();
    let out = saved(&bytes);
    out.assert_mdx_kept();
    for (address, n) in CELLS {
        assert_eq!(out.mdx_record(address), Some(record(n)), "{address}");
        assert_eq!(out.cells[address].1, address != "B4", "{address}: formula presence changed");
    }
    assert_eq!(out.cells["C1"].0, None, "a plain cell gets no value metadata");
    // The file GridCraft wrote reads back and saves the same way.
    let (back, report) = read_xlsx(&bytes).unwrap();
    assert!(!report.warnings.iter().any(|w| w.contains("metadata")), "{:?}", report.warnings);
    let again = saved(&write_xlsx(&back).unwrap());
    again.assert_mdx_kept();
    for (address, n) in CELLS {
        assert_eq!(again.mdx_record(address), Some(record(n)), "second save, {address}");
    }
}

#[test]
fn a_cell_whose_content_is_replaced_loses_its_link_and_the_others_keep_theirs() {
    let (mut wb, _) = read_xlsx(&source(false)).unwrap();
    let sh = wb.sheet_mut(0).unwrap();
    sh.set_cell(at("B1"), Cell::formula(Formula::new("=1+1")));
    sh.set_value(at("B4"), Value::Number(3.0));
    let out = saved(&write_xlsx(&wb).unwrap());
    out.assert_mdx_kept();
    assert_eq!(out.cells["B1"].0, None, "B1 is no longer a cube formula");
    assert_eq!(out.cells["B4"].0, None, "B4 is no longer a cube value");
    for (address, n) in [("B2", 1), ("B3", 2), ("B5", 4)] {
        assert_eq!(out.mdx_record(address), Some(record(n)), "{address}");
    }
}

#[test]
fn mdx_metadata_is_written_next_to_dynamic_array_metadata() {
    let (wb, _) = read_xlsx(&source(true)).unwrap();
    assert!(wb.sheet(0).unwrap().cell(at("D1")).unwrap().formula.as_ref().unwrap().dynamic);
    let out = saved(&write_xlsx(&wb).unwrap());
    out.assert_mdx_kept();
    for (address, n) in CELLS {
        assert_eq!(out.mdx_record(address), Some(record(n)), "{address}");
    }
    // D1's cm still resolves to the dynamic-array type, whatever number it has now.
    let cm = *out.cm.get("D1").expect("D1 keeps its cm");
    let rc = out.metadata().child("cellMetadata").unwrap().kids("bk").nth(cm - 1).unwrap().child("rc").unwrap();
    assert_eq!(out.type_names()[rc.attr_u32("t").unwrap() as usize - 1], "XLDAPR");
    assert_eq!(out.cells["D1"].0, None);
    assert!(out.metadata().kids("futureMetadata").any(|f| f.attr("name") == Some("XLDAPR")));
}

#[test]
fn a_broken_chain_opens_and_keeps_nothing_for_that_cell() {
    // B1's vm is past the blocks; B2's block names a record that does not exist.
    let metadata = format!(
        r#"<?xml version="1.0"?><metadata xmlns="{NS}"><metadataTypes count="1">{MDX_TYPE}</metadataTypes><metadataStrings count="2"><s v="Sales cube"/><s v="[Measures].[Profit]"/></metadataStrings><mdxMetadata count="1">{}</mdxMetadata><valueMetadata count="2"><bk><rc t="1" v="0"/></bk><bk><rc t="1" v="9"/></bk></valueMetadata></metadata>"#,
        RECORDS[0]
    );
    let body = r#"<sheetData><row r="1"><c r="A1" vm="1"><v>1</v></c><c r="B1" vm="7"><v>2</v></c></row><row r="2"><c r="B2" vm="2"><v>3</v></c></row></sheetData>"#;
    let (wb, _) = read_xlsx(&super::minimal(body, &[("xl/metadata.xml", &metadata)], RELS.split_once("/>").unwrap().1, "")).unwrap();
    let out = saved(&write_xlsx(&wb).unwrap());
    out.assert_consistent();
    assert_eq!(out.mdx_record("A1"), Some(record(0)));
    assert_eq!(out.cells["B1"].0, None);
    assert_eq!(out.cells["B2"].0, None);
}

#[test]
fn a_workbook_without_mdx_metadata_writes_none() {
    let (wb, _) = read_xlsx(&super::minimal(r#"<sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData>"#, &[], "", "")).unwrap();
    let out = saved(&write_xlsx(&wb).unwrap());
    assert!(out.metadata.is_none(), "no metadata part for a plain workbook");
}
