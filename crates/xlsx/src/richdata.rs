//! Static in-cell PNG/JPEG images and cached rich error values (`#SPILL!`, `#CALC!`), from the
//! public MS-XLSX rich-data schemas.
//! `vm` and metadata type IDs are one-based; every subsequent table index is zero-based.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use gridcraft_core::CellError;
use gridcraft_model::CellPicture;

use crate::IoError;
use crate::package::{Rel, normalize};
use crate::read::Ctx;
use crate::write::{NS_MAIN, NS_REL, Out, Rels, XML_DECL};
use crate::xml::{self, El, esc};

const NS_RICH: &str = "http://schemas.microsoft.com/office/spreadsheetml/2017/richdata";
const NS_IMAGE_REL: &str = "http://schemas.microsoft.com/office/spreadsheetml/2022/richvaluerel";
// Lower case, as Excel writes it: Excel refuses a workbook whose XLRICHVALUE extension URI is in
// upper case (same class as #2339). The reader compares it case-insensitively.
/// The rich value types part ([MS-XLSX] 2.3.6): fixed key flags for the reserved keys, as Excel
/// writes them for a workbook with pictures in cells.
const RICH_VALUE_TYPES: &str = concat!(
    "<rvTypesInfo xmlns=\"http://schemas.microsoft.com/office/spreadsheetml/2017/richdata2\" ",
    "xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"x\" ",
    "xmlns:x=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><global><keyFlags>",
    "<key name=\"_Self\"><flag name=\"ExcludeFromFile\" value=\"1\"/><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_DisplayString\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_Flags\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_Format\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_SubLabel\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_Attribution\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_Icon\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_Display\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_CanonicalPropertyNames\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "<key name=\"_ClassificationId\"><flag name=\"ExcludeFromCalcComparison\" value=\"1\"/></key>",
    "</keyFlags></global></rvTypesInfo>"
);
const RICH_EXT: &str = "{3e2802c4-a4d2-4d8b-9148-e3be6c30e623}";
const MAX_IMAGE: u64 = 16 * 1024 * 1024;
const MAX_IMAGES_TOTAL: usize = 128 * 1024 * 1024;
const MAX_RECORDS: usize = 100_000;
const MAX_XML: u64 = 16 * 1024 * 1024;
const MAX_PIXELS: u64 = 16_000_000;

/// Header dimensions bound decode allocations in consumers. Full decoding is left to the UI.
fn image_type(data: &[u8]) -> Option<(&'static str, &'static str)> {
    let dimensions = if data.len() >= 33 && data.starts_with(b"\x89PNG\r\n\x1a\n") && data.get(8..16)? == b"\0\0\0\rIHDR" {
        let w = u32::from_be_bytes(data.get(16..20)?.try_into().ok()?);
        let h = u32::from_be_bytes(data.get(20..24)?.try_into().ok()?);
        Some((w, h, "image/png", "png"))
    } else if data.starts_with(&[0xff, 0xd8]) {
        jpeg_dimensions(data).map(|(w, h)| (w, h, "image/jpeg", "jpeg"))
    } else {
        None
    };
    let (w, h, mime, ext) = dimensions?;
    (w > 0 && h > 0 && w <= 8192 && h <= 8192 && u64::from(w) * u64::from(h) <= MAX_PIXELS).then_some((mime, ext))
}

fn jpeg_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let mut offset = 2usize;
    while offset < data.len() {
        if *data.get(offset)? != 0xff {
            return None;
        }
        while data.get(offset) == Some(&0xff) {
            offset += 1;
        }
        let marker = *data.get(offset)?;
        offset += 1;
        if matches!(marker, 0xd9 | 0xda) {
            return None;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let len = usize::from(u16::from_be_bytes(data.get(offset..offset.checked_add(2)?)?.try_into().ok()?));
        let end = offset.checked_add(len)?;
        if len < 2 || end > data.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) && len >= 8 {
            let h = u16::from_be_bytes(data.get(offset + 3..offset + 5)?.try_into().ok()?);
            let w = u16::from_be_bytes(data.get(offset + 5..offset + 7)?.try_into().ok()?);
            return Some((u32::from(w), u32::from(h)));
        }
        offset = end;
    }
    None
}

fn part(cx: &mut Ctx<'_>, rels: &[Rel], kind: &str) -> Option<String> {
    let mut target: Option<String> = None;
    for rel in rels.iter().filter(|rel| rel.kind == kind) {
        if rel.external {
            cx.warn(format!("external {kind} relationship is unsupported; cell pictures may be missing"));
            return None;
        }
        if target.as_ref().is_some_and(|old| normalize(old) != normalize(&rel.target)) {
            cx.warn(format!("conflicting {kind} relationships; cell pictures may be missing"));
            return None;
        }
        target = Some(rel.target.clone());
    }
    target
}

fn read_xml(cx: &mut Ctx<'_>, path: &str, root: &str) -> Result<Option<El>, IoError> {
    let bytes = match cx.pkg.read_limited(path, MAX_XML) {
        Ok(Some(bytes)) => bytes,
        Err(error @ IoError::TooLarge(_)) => return Err(error),
        other => {
            cx.warn(format!("missing or damaged cell-picture part {path}: {}", other.err().map(|e| e.to_string()).unwrap_or_default()));
            return Ok(None);
        }
    };
    match xml::parse(&bytes) {
        Ok(el) if el.name == root => Ok(Some(el)),
        _ => {
            cx.warn(format!("invalid cell-picture part {path}"));
            Ok(None)
        }
    }
}

fn entries<'a>(cx: &mut Ctx<'_>, el: &'a El, child: &'a str) -> Result<Vec<&'a El>, IoError> {
    let entries: Vec<_> = el.kids(child).take(MAX_RECORDS + 1).collect();
    if entries.len() > MAX_RECORDS {
        return Err(IoError::TooLarge("too many cell-picture metadata records".into()));
    }
    if el.attr("count").is_some() && el.attr_u32("count").map(|n| n as usize) != Some(entries.len()) {
        cx.warn(format!("cell-picture {} count does not match its records", el.name));
    }
    Ok(entries)
}

/// What a cell's value metadata resolves to.
#[derive(Clone, Debug)]
pub enum RichCell {
    /// A picture in the cell.
    Picture(Arc<CellPicture>),
    /// An error value (structure type `_error`). Excel stores errors it cannot write in `<v>`
    /// (`#SPILL!`, `#CALC!`) this way, with `#VALUE!` as the cell's fallback. `None` is an
    /// `errorType` this reader does not know: the fallback is kept.
    Error(Option<CellError>),
}

/// The error a rich value's `errorType` names, as measured in workbooks saved by Excel. Other
/// numbers are not guessed.
fn rich_error(error_type: u32) -> Option<CellError> {
    match error_type {
        8 => Some(CellError::Spill),
        13 => Some(CellError::Calc),
        _ => None,
    }
}

/// Resolves every value-metadata block without assuming the indices at each layer coincide.
pub fn read(cx: &mut Ctx<'_>, workbook_rels: &[Rel]) -> Result<Vec<Option<RichCell>>, IoError> {
    let Some(metadata_path) = part(cx, workbook_rels, "sheetMetadata") else { return Ok(vec![]) };
    let Some(metadata) = read_xml(cx, &metadata_path, "metadata")? else { return Ok(vec![]) };
    let Some(types) = metadata.child("metadataTypes") else { return Ok(vec![]) };
    let types = entries(cx, types, "metadataType")?;
    let Some(type_id) = types.iter().position(|el| el.attr("name") == Some("XLRICHVALUE")).map(|n| n + 1) else { return Ok(vec![]) };
    if types.iter().filter(|el| el.attr("name") == Some("XLRICHVALUE")).count() != 1 {
        cx.warn("duplicate XLRICHVALUE metadata types; cell pictures were not loaded");
        return Ok(vec![]);
    }
    // Support workbook-owned parts and the implicit owner chain described by MS-XLSX.
    let mut rels = workbook_rels.to_vec();
    rels.extend(cx.pkg.rels(&metadata_path)?);
    let Some(data_path) = part(cx, &rels, "rdRichValue") else {
        cx.warn("missing rich-value data relationship; cell pictures were not loaded");
        return Ok(vec![]);
    };
    rels.extend(cx.pkg.rels(&data_path)?);
    let Some(structure_path) = part(cx, &rels, "rdRichValueStructure") else {
        cx.warn("missing rich-value structure relationship; cell pictures were not loaded");
        return Ok(vec![]);
    };
    let Some(data) = read_xml(cx, &data_path, "rvData")? else { return Ok(vec![]) };
    let Some(structures) = read_xml(cx, &structure_path, "rvStructures")? else { return Ok(vec![]) };
    let structures = entries(cx, &structures, "s")?;
    // Each structure's key layout, worked out once: every value would otherwise walk its
    // structure's keys again. Only local-image and error structures matter here.
    let layouts: Vec<Option<Layout>> = structures.iter().map(|s| Layout::of(s)).collect();
    // Image relationships are needed only by pictures: a workbook whose rich values are all
    // errors has none.
    let image_rels_path = part(cx, &rels, "richValueRel");
    if image_rels_path.is_none() && layouts.iter().any(|l| matches!(l, Some(Layout { kind: Kind::Image { .. }, .. }))) {
        cx.warn("missing rich-value image relationships; cell pictures were not loaded");
    }
    let image_rel_list = match &image_rels_path {
        Some(path) => read_xml(cx, path, "richValueRels")?,
        None => None,
    };
    let image_rels = match &image_rels_path {
        Some(path) => cx.pkg.rels(path)?,
        None => vec![],
    };
    let image_rel_list = match &image_rel_list {
        Some(list) => entries(cx, list, "rel")?,
        None => vec![],
    };
    // Relationships by id, a repeated id recorded as invalid (`None`), so a value's lookup is
    // one probe rather than a scan of the whole table for duplicates.
    let mut rels_by_id: HashMap<&str, Option<&Rel>> = HashMap::new();
    for rel in &image_rels {
        rels_by_id.entry(rel.id.as_str()).and_modify(|r| *r = None).or_insert(Some(rel));
    }
    let values = entries(cx, &data, "rv")?;
    let mut pictures = Vec::with_capacity(values.len());
    let mut cache = HashMap::new();
    let mut total = 0usize;
    for value in values {
        let picture = read_value(cx, value, &layouts, &image_rel_list, &rels_by_id, &mut cache, &mut total)?;
        if picture.is_none() {
            cx.warn("some rich values are unsupported or malformed; their scalar fallbacks were retained (cell pictures may be missing)");
        }
        pictures.push(picture);
    }
    let Some(future) = metadata.kids("futureMetadata").find(|el| el.attr("name") == Some("XLRICHVALUE")) else {
        cx.warn("missing XLRICHVALUE future metadata; cell pictures were not loaded");
        return Ok(vec![]);
    };
    let future: Vec<_> = entries(cx, future, "bk")?
        .into_iter()
        .map(|block| {
            block
                .child("extLst")
                .and_then(|el| el.kids("ext").find(|ext| ext.attr("uri").is_some_and(|uri| uri.eq_ignore_ascii_case(RICH_EXT))))
                .and_then(|ext| ext.child("rvb"))
                .and_then(|rvb| rvb.attr_u32("i"))
                .map(|n| n as usize)
        })
        .collect();
    let Some(blocks) = metadata.child("valueMetadata") else { return Ok(vec![]) };
    let blocks = entries(cx, blocks, "bk")?;
    Ok(blocks
        .into_iter()
        .map(|block| {
            block
                .kids("rc")
                .find(|rc| rc.attr_u32("t").map(|n| n as usize) == Some(type_id))
                .and_then(|rc| rc.attr_u32("v"))
                .and_then(|n| future.get(n as usize))
                .copied()
                .flatten()
                .and_then(|n| pictures.get(n))
                .cloned()
                .flatten()
        })
        .collect())
}

/// Where a `_localImage` or `_error` structure keeps the values this reader needs.
struct Layout {
    keys: usize,
    kind: Kind,
}

enum Kind {
    Image {
        /// Index of the one `_rvRel:LocalImageIdentifier` key of type `i`, if exactly one.
        image: Option<usize>,
        /// Index of the one `Text` key of type `s`, if exactly one.
        text: Option<usize>,
    },
    Error {
        /// Index of the one `errorType` key of type `i`, if exactly one.
        error_type: Option<usize>,
    },
}

impl Layout {
    fn of(structure: &El) -> Option<Layout> {
        let keys: Vec<&El> = structure.kids("k").collect();
        let one = |name: &str, ty: &str| {
            let mut matches = keys.iter().enumerate().filter(|(_, k)| k.attr("n") == Some(name));
            let (i, key) = matches.next()?;
            (matches.next().is_none() && key.attr("t") == Some(ty)).then_some(i)
        };
        let kind = match structure.attr("t")? {
            "_localImage" => Kind::Image { image: one("_rvRel:LocalImageIdentifier", "i"), text: one("Text", "s") },
            "_error" => Kind::Error { error_type: one("errorType", "i") },
            _ => return None,
        };
        Some(Layout { keys: keys.len(), kind })
    }
}

fn read_value(
    cx: &mut Ctx<'_>,
    value: &El,
    layouts: &[Option<Layout>],
    image_rel_list: &[&El],
    rels_by_id: &HashMap<&str, Option<&Rel>>,
    cache: &mut HashMap<(String, String), Arc<CellPicture>>,
    total: &mut usize,
) -> Result<Option<RichCell>, IoError> {
    let Some(Some(layout)) = value.attr_u32("s").and_then(|n| layouts.get(n as usize)) else { return Ok(None) };
    let values: Vec<_> = value.kids("v").collect();
    if layout.keys != values.len() {
        return Ok(None);
    }
    let lookup = |slot: Option<usize>| slot.and_then(|i| values.get(i)).map(|v| v.text.as_str());
    let (image, text) = match layout.kind {
        Kind::Image { image, text } => (image, text),
        Kind::Error { error_type } => {
            return Ok(Some(RichCell::Error(lookup(error_type).and_then(|v| v.trim().parse::<u32>().ok()).and_then(rich_error))));
        }
    };
    let Some(rel_id) = lookup(image).and_then(|v| v.trim().parse::<usize>().ok()).and_then(|n| image_rel_list.get(n)).and_then(|rel| rel.attr("id"))
    else {
        return Ok(None);
    };
    let Some(found) = rels_by_id.get(rel_id) else { return Ok(None) };
    let Some(rel) = found.filter(|rel| rel.kind == "image" && !rel.external) else {
        cx.warn("a cell picture has an external or invalid image relationship; kept its scalar fallback");
        return Ok(None);
    };
    let alt = lookup(text).unwrap_or("").to_string();
    let key = (normalize(&rel.target), alt.clone());
    if let Some(picture) = cache.get(&key) {
        return Ok(Some(RichCell::Picture(picture.clone())));
    }
    let data = match cx.pkg.read_limited(&rel.target, MAX_IMAGE) {
        Ok(Some(data)) => data,
        other => {
            cx.warn(format!(
                "missing, damaged or oversized cell picture {}: {}; kept its scalar fallback",
                rel.target,
                other.err().map(|e| e.to_string()).unwrap_or_default()
            ));
            return Ok(None);
        }
    };
    let Some((mime, _)) = image_type(&data) else {
        cx.warn("a cell picture is not a supported PNG/JPEG or exceeds 8192 pixels per side / 16 million pixels; kept its scalar fallback");
        return Ok(None);
    };
    if total.saturating_add(data.len()) > MAX_IMAGES_TOTAL {
        cx.warn("cell pictures exceed the 128 MiB workbook limit; remaining pictures kept their scalar fallbacks");
        return Ok(None);
    }
    *total += data.len();
    let picture = Arc::new(CellPicture { data, mime: mime.to_string(), alt });
    cache.insert(key, picture.clone());
    Ok(Some(RichCell::Picture(picture)))
}

/// One rich value per shared model payload; copied cells can reuse a metadata block.
#[derive(Default)]
pub struct PicturesOut {
    pictures: Vec<Arc<CellPicture>>,
    indices: HashMap<*const CellPicture, u32>,
}

impl PicturesOut {
    pub fn is_empty(&self) -> bool {
        self.pictures.is_empty()
    }
    pub fn index(&mut self, picture: &Arc<CellPicture>) -> u32 {
        let key = Arc::as_ptr(picture);
        if let Some(index) = self.indices.get(&key) {
            return *index;
        }
        let index = self.pictures.len().saturating_add(1) as u32;
        self.pictures.push(picture.clone());
        self.indices.insert(key, index);
        index
    }
}

pub fn write(out: &mut Out, workbook_rels: &mut Rels) -> Result<(), IoError> {
    let pictures = std::mem::take(&mut out.cell_pictures).pictures;
    if pictures.len() > MAX_RECORDS {
        return Err(IoError::TooLarge("too many cell pictures".into()));
    }
    let count = pictures.len();
    let mut data = format!("{XML_DECL}<rvData xmlns=\"{NS_RICH}\" count=\"{count}\">");
    let mut rel_list = format!("{XML_DECL}<richValueRels xmlns=\"{NS_IMAGE_REL}\" xmlns:r=\"{NS_REL}\">");
    let mut image_rels = Rels::default();
    let mut total = 0usize;
    // Only the structures actually used are written: the one without `Text` first (index 0),
    // then the one with `Text`.
    let has_plain = pictures.iter().any(|picture| picture.alt.is_empty());
    let has_text = pictures.iter().any(|picture| !picture.alt.is_empty());
    let (plain, with_text) = (0usize, usize::from(has_plain));
    for (i, picture) in pictures.iter().enumerate() {
        total = total.saturating_add(picture.data.len());
        if picture.data.len() as u64 > MAX_IMAGE || total > MAX_IMAGES_TOTAL {
            return Err(IoError::TooLarge("cell pictures exceed the 16 MiB image or 128 MiB workbook limit".into()));
        }
        let (mime, ext) = image_type(&picture.data)
            .ok_or_else(|| IoError::Format("cell pictures must be PNG/JPEG with at most 8192 pixels per side and 16 million pixels".into()))?;
        let image_name = format!("cellImage{}.{}", i + 1, ext);
        out.default_type(ext, mime);
        out.part(&format!("xl/media/{image_name}"), None, picture.data.clone());
        let rid = image_rels.add("image", &format!("../media/{image_name}"));
        let _ = write!(rel_list, "<rel r:id=\"{rid}\"/>");
        // Keys in Excel's order: CalcOrigin, [Text], _rvRel:LocalImageIdentifier. A picture
        // without alt text uses the structure without `Text` (Excel never writes an empty `Text`).
        let s = if picture.alt.is_empty() { plain } else { with_text };
        if picture.alt.is_empty() {
            let _ = write!(data, "<rv s=\"{s}\"><v>5</v><v>{i}</v></rv>");
        } else {
            let _ = write!(data, "<rv s=\"{s}\"><v>5</v><v>{}</v><v>{i}</v></rv>", esc(&picture.alt));
        }
    }
    data.push_str("</rvData>");
    rel_list.push_str("</richValueRels>");
    let mut structures = format!("{XML_DECL}<rvStructures xmlns=\"{NS_RICH}\" count=\"{}\">", usize::from(has_plain) + usize::from(has_text));
    if has_plain {
        structures.push_str("<s t=\"_localImage\"><k n=\"CalcOrigin\" t=\"i\"/><k n=\"_rvRel:LocalImageIdentifier\" t=\"i\"/></s>");
    }
    if has_text {
        structures
            .push_str("<s t=\"_localImage\"><k n=\"CalcOrigin\" t=\"i\"/><k n=\"Text\" t=\"s\"/><k n=\"_rvRel:LocalImageIdentifier\" t=\"i\"/></s>");
    }
    structures.push_str("</rvStructures>");
    for (name, content_type, relation, bytes) in [
        (
            "rdrichvalue.xml",
            "application/vnd.ms-excel.rdrichvalue+xml",
            "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValue",
            data.into_bytes(),
        ),
        (
            "rdrichvaluestructure.xml",
            "application/vnd.ms-excel.rdrichvaluestructure+xml",
            "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueStructure",
            structures.into_bytes(),
        ),
        (
            "rdRichValueTypes.xml",
            "application/vnd.ms-excel.rdrichvaluetypes+xml",
            "http://schemas.microsoft.com/office/2017/06/relationships/rdRichValueTypes",
            format!("{XML_DECL}{RICH_VALUE_TYPES}").into_bytes(),
        ),
        (
            "richValueRel.xml",
            "application/vnd.ms-excel.richvaluerel+xml",
            "http://schemas.microsoft.com/office/2022/10/relationships/richValueRel",
            rel_list.into_bytes(),
        ),
    ] {
        out.part(&format!("xl/richData/{name}"), Some(content_type), bytes);
        workbook_rels.add_uri(relation, &format!("richData/{name}"));
    }
    out.part("xl/richData/_rels/richValueRel.xml.rels", None, image_rels.xml().into_bytes());
    out.part(
        "xl/metadata.xml",
        Some("application/vnd.openxmlformats-officedocument.spreadsheetml.sheetMetadata+xml"),
        match &out.mdx {
            Some(mdx) => crate::mdx_metadata::metadata(mdx, count, out.dynamic_arrays),
            None => metadata(count, out.dynamic_arrays),
        }
        .into_bytes(),
    );
    workbook_rels.add("sheetMetadata", "metadata.xml");
    Ok(())
}

fn metadata(count: usize, dynamic: bool) -> String {
    let type_id = if dynamic { 2 } else { 1 };
    let mut xml = format!(
        "{XML_DECL}<metadata xmlns=\"{NS_MAIN}\" xmlns:rv=\"{NS_RICH}\" xmlns:xda=\"http://schemas.microsoft.com/office/spreadsheetml/2017/dynamicarray\"><metadataTypes count=\"{type_id}\">"
    );
    if dynamic {
        xml.push_str("<metadataType name=\"XLDAPR\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\" cellMeta=\"1\"/>");
    }
    xml.push_str("<metadataType name=\"XLRICHVALUE\" minSupportedVersion=\"120000\" copy=\"1\" pasteAll=\"1\" pasteValues=\"1\" merge=\"1\" splitFirst=\"1\" rowColShift=\"1\" clearFormats=\"1\" clearComments=\"1\" assign=\"1\" coerce=\"1\"/></metadataTypes>");
    if dynamic {
        // Lower case, as the dynamic-array-only metadata in `write.rs` spells it: Excel refuses
        // a workbook whose XLDAPR extension URI is in upper case (#2339).
        xml.push_str("<futureMetadata name=\"XLDAPR\" count=\"1\"><bk><extLst><ext uri=\"{bdbb8cdc-fa1e-496e-a857-3c3f30c029c3}\"><xda:dynamicArrayProperties fDynamic=\"1\" fCollapsed=\"0\"/></ext></extLst></bk></futureMetadata>");
    }
    let _ = write!(xml, "<futureMetadata name=\"XLRICHVALUE\" count=\"{count}\">");
    for i in 0..count {
        let _ = write!(xml, "<bk><extLst><ext uri=\"{RICH_EXT}\"><rv:rvb i=\"{i}\"/></ext></extLst></bk>");
    }
    xml.push_str("</futureMetadata>");
    if dynamic {
        xml.push_str("<cellMetadata count=\"1\"><bk><rc t=\"1\" v=\"0\"/></bk></cellMetadata>");
    }
    let _ = write!(xml, "<valueMetadata count=\"{count}\">");
    for i in 0..count {
        let _ = write!(xml, "<bk><rc t=\"{type_id}\" v=\"{i}\"/></bk>");
    }
    xml.push_str("</valueMetadata></metadata>");
    xml
}
