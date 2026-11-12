//! Cube-formula (MDX) value metadata of `xl/metadata.xml` (ECMA-376 Part 1, 18.9.8 `CT_Metadata`,
//! MDX records 18.9.10-18.9.12), kept through open + save: the `XLMDX` metadata type,
//! `metadataStrings`, `mdxMetadata`, and each cell's `vm` link to its record. Nothing in the
//! records is interpreted.
//!
//! `vm` and `rc t` are one-based; `rc v` and the record index are zero-based.

use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_model::MdxMetadata;

use crate::package::Rel;
use crate::read::Ctx;
use crate::write::{NS_MAIN, XML_DECL};
use crate::xml::{self, El, esc_attr};

const TYPE: &str = "XLMDX";
const MAX_XML: u64 = 16 * 1024 * 1024;
const MAX_RECORDS: usize = 100_000;
/// Nesting kept for one record (`mdx` > `t` > `n` is three levels).
const MAX_DEPTH: usize = 16;
const CONTENT_TYPE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml";

/// What a `valueMetadata` block says about cubes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Block {
    /// Not an `XLMDX` block (or one whose record does not exist).
    Other,
    /// The zero-based MDX record the block names.
    Record(u32),
}

/// The workbook's MDX metadata and, by zero-based `valueMetadata` block, what each block names.
#[derive(Default)]
pub struct MdxIn {
    pub metadata: Option<Arc<MdxMetadata>>,
    pub blocks: Vec<Block>,
}

impl MdxIn {
    /// The MDX record a cell's one-based `vm` resolves to, if any.
    pub fn record(&self, vm: &str) -> Option<u32> {
        let n = vm.trim().parse::<usize>().ok()?.checked_sub(1)?;
        match self.blocks.get(n)? {
            Block::Record(r) => Some(*r),
            Block::Other => None,
        }
    }
}

/// Reads the MDX metadata. A missing, damaged or oversized part keeps nothing and adds no warning:
/// the rich-data reader already reports on the same part.
pub fn read(cx: &mut Ctx<'_>, workbook_rels: &[Rel]) -> MdxIn {
    let none = MdxIn::default();
    let Some(path) = workbook_rels.iter().find(|r| r.kind == "sheetMetadata" && !r.external).map(|r| r.target.clone()) else { return none };
    let Ok(Some(bytes)) = cx.pkg.read_limited(&path, MAX_XML) else { return none };
    let Ok(metadata) = xml::parse(&bytes) else { return none };
    if metadata.name != "metadata" {
        return none;
    }
    let Some(types) = metadata.child("metadataTypes") else { return none };
    let types: Vec<&El> = types.kids("metadataType").take(MAX_RECORDS + 1).collect();
    let mut named = types.iter().enumerate().filter(|(_, t)| t.attr("name") == Some(TYPE));
    let (Some((position, mdx_type)), None) = (named.next(), named.next()) else { return none };
    if types.len() > MAX_RECORDS {
        return none;
    }
    let type_id = position.saturating_add(1);
    let strings: Vec<String> = metadata
        .child("metadataStrings")
        .into_iter()
        .flat_map(|s| s.kids("s"))
        .map(|s| s.attr("v").unwrap_or_default().to_string())
        .take(MAX_RECORDS + 1)
        .collect();
    let records: Vec<Option<String>> = metadata
        .child("mdxMetadata")
        .into_iter()
        .flat_map(|m| m.kids("mdx"))
        .take(MAX_RECORDS + 1)
        .map(|el| {
            let mut out = String::new();
            push_el(&mut out, el, 0).then_some(out)
        })
        .collect();
    if strings.len() > MAX_RECORDS || records.len() > MAX_RECORDS || records.is_empty() || records.iter().any(Option::is_none) {
        return none;
    }
    let records: Vec<String> = records.into_iter().flatten().collect();
    let blocks = metadata
        .child("valueMetadata")
        .into_iter()
        .flat_map(|v| v.kids("bk"))
        .take(MAX_RECORDS)
        .map(|bk| {
            bk.kids("rc")
                .find(|rc| rc.attr_u32("t").map(|t| t as usize) == Some(type_id))
                .and_then(|rc| rc.attr_u32("v"))
                .filter(|v| (*v as usize) < records.len())
                .map_or(Block::Other, Block::Record)
        })
        .collect();
    let metadata = MdxMetadata { type_attrs: mdx_type.attrs.clone(), strings, records };
    MdxIn { metadata: Some(Arc::new(metadata)), blocks }
}

/// Writes `el` as XML; `false` when it nests deeper than [`MAX_DEPTH`].
fn push_el(out: &mut String, el: &El, depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false;
    }
    out.push('<');
    out.push_str(&el.name);
    push_attrs(out, &el.attrs);
    if el.children.is_empty() && el.text.is_empty() {
        out.push_str("/>");
        return true;
    }
    out.push('>');
    xml::push_esc(out, &el.text);
    for child in &el.children {
        if !push_el(out, child, depth + 1) {
            return false;
        }
    }
    let _ = write!(out, "</{}>", el.name);
    true
}

fn push_attrs(out: &mut String, attrs: &[(String, String)]) {
    for (k, v) in attrs {
        let _ = write!(out, " {k}=\"{}\"", esc_attr(v));
    }
}

/// The `XLDAPR` type, as the dynamic-array-only metadata in `write.rs` writes it.
const DYNAMIC_TYPE: &str = "<metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/>";
const RICH_TYPE: &str = "<metadataType name=\"XLRICHVALUE\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\"/>";
// Lower case, as Excel writes them (#2339).
const DYNAMIC_EXT: &str = "{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}";
const RICH_EXT: &str = "{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}";

/// The whole `xl/metadata.xml` of a workbook with MDX metadata, holding every type in use:
/// `XLDAPR` when `dynamic`, `XLRICHVALUE` for `pictures` rich values, then `XLMDX`. The value
/// metadata lists one block per MDX record first (a cell's `vm` is its record index + 1, known
/// before any sheet is written), then one per rich value (`vm` = record count + rich value index
/// + 1, see [`crate::richdata::PicturesOut`]). Children follow the schema order (18.9.8).
pub fn metadata(mdx: &MdxMetadata, pictures: usize, dynamic: bool) -> String {
    let rich = pictures > 0;
    let dynamic_id = 1usize;
    let rich_id = 1 + usize::from(dynamic);
    let mdx_id = rich_id + usize::from(rich);
    let mut xml = format!("{XML_DECL}<metadata xmlns=\"{NS_MAIN}\"");
    if rich {
        xml.push_str(" xmlns:rv=\"http://schemas.microsoft.com/office/spreadsheetml/2017/richdata\"");
    }
    if dynamic {
        xml.push_str(" xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\"");
    }
    let _ = write!(xml, "><metadataTypes count=\"{mdx_id}\">");
    if dynamic {
        xml.push_str(DYNAMIC_TYPE);
    }
    if rich {
        xml.push_str(RICH_TYPE);
    }
    xml.push_str("<metadataType");
    push_attrs(&mut xml, &mdx.type_attrs);
    xml.push_str("/></metadataTypes>");
    if !mdx.strings.is_empty() {
        let _ = write!(xml, "<metadataStrings count=\"{}\">", mdx.strings.len());
        for s in &mdx.strings {
            let _ = write!(xml, "<s v=\"{}\"/>", esc_attr(s));
        }
        xml.push_str("</metadataStrings>");
    }
    let _ = write!(xml, "<mdxMetadata count=\"{}\">", mdx.records.len());
    for record in &mdx.records {
        xml.push_str(record);
    }
    xml.push_str("</mdxMetadata>");
    if dynamic {
        let _ = write!(
            xml,
            "<futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{DYNAMIC_EXT}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata>"
        );
    }
    if rich {
        let _ = write!(xml, "<futureMetadata name=\"XLRICHVALUE\" count=\"{pictures}\">");
        for i in 0..pictures {
            let _ = write!(xml, "<bk><extLst><ext uri=\"{RICH_EXT}\"><rv:rvb i=\"{i}\"/></ext></extLst></bk>");
        }
        xml.push_str("</futureMetadata>");
    }
    if dynamic {
        let _ = write!(xml, "<cellMetadata count=\"1\"><bk><rc t=\"{dynamic_id}\" v=\"0\"/></bk></cellMetadata>");
    }
    let _ = write!(xml, "<valueMetadata count=\"{}\">", mdx.records.len().saturating_add(pictures));
    for i in 0..mdx.records.len() {
        let _ = write!(xml, "<bk><rc t=\"{mdx_id}\" v=\"{i}\"/></bk>");
    }
    for i in 0..pictures {
        let _ = write!(xml, "<bk><rc t=\"{rich_id}\" v=\"{i}\"/></bk>");
    }
    xml.push_str("</valueMetadata></metadata>");
    xml
}

/// Adds `xl/metadata.xml` for a workbook with MDX metadata and no cell pictures.
pub fn write(out: &mut crate::write::Out, mdx: &MdxMetadata, workbook_rels: &mut crate::write::Rels) {
    let xml = metadata(mdx, 0, out.dynamic_arrays);
    out.part("xl/metadata.xml", Some(CONTENT_TYPE), xml.into_bytes());
    workbook_rels.add("sheetMetadata", "metadata.xml");
}
