//! Pictures in `.xls` files ([MS-XLS] 2.4.168 MsoDrawing, 2.4.170 MsoDrawingGroup; [MS-ODRAW]):
//! the workbook globals' MsoDrawingGroup holds an OfficeArtDggContainer whose
//! OfficeArtBStoreContainer lists the pictures (OfficeArtFBSE entries, each embedding its blip);
//! a picture shape on a sheet names its entry by the 1-based `pib` property of its
//! OfficeArtFOPT. PNG and JPEG blips are kept as their encoded bytes; other blip types are not
//! imported.

use std::sync::Arc;

use gridcraft_model::Color;

use super::{Budget, MAX_PICTURE_BYTES};
use crate::ReadReport;

/// OfficeArt record types ([MS-ODRAW] 2.1.1, 2.2).
const DGG_CONTAINER: u16 = 0xF000;
const BSTORE_CONTAINER: u16 = 0xF001;
const FBSE: u16 = 0xF007;
const BLIP_JPEG: u16 = 0xF01D;
const BLIP_PNG: u16 = 0xF01E;
pub(super) const FSP: u16 = 0xF00A;
pub(super) const FOPT: u16 = 0xF00B;
pub(super) const TERTIARY_FOPT: u16 = 0xF122;

/// Property ids ([MS-ODRAW] 2.3): the BStore index, the shape's name and description, the group
/// shape boolean properties, the fill and line colours, the fill and line style boolean
/// properties and the line's arrowheads.
const PROP_PIB: u16 = 0x0104;
const PROP_NAME: u16 = 0x0380;
const PROP_DESCRIPTION: u16 = 0x0381;
const PROP_GROUP_BOOLEANS: u16 = 0x03BF;
const PROP_FILL_COLOR: u16 = 0x0181;
const PROP_FILL_BOOLEANS: u16 = 0x01BF;
const PROP_LINE_COLOR: u16 = 0x01C0;
const PROP_LINE_START_ARROWHEAD: u16 = 0x01D0;
const PROP_LINE_END_ARROWHEAD: u16 = 0x01D1;
const PROP_LINE_BOOLEANS: u16 = 0x01FF;
/// GroupShapeBooleanProperties: `fHidden` and `fUsefHidden`.
const HIDDEN: u32 = 0x0000_0002;
const USE_HIDDEN: u32 = 0x0002_0000;
/// FillStyleBooleanProperties: `fFilled` and `fUsefFilled`.
const FILLED: u32 = 0x0000_0010;
const USE_FILLED: u32 = 0x0010_0000;
/// LineStyleBooleanProperties: `fLine` and `fUsefLine`.
const LINE: u32 = 0x0000_0008;
const USE_LINE: u32 = 0x0008_0000;
/// OfficeArtCOLORREF ([MS-ODRAW] 2.2.22): the `fSchemeIndex` flag, whose red byte is then the
/// BIFF8 colour index.
const COLOR_SCHEME_INDEX: u32 = 0x0000_0008;

/// BStore entries read at most.
const MAX_ENTRIES: usize = 65_536;
/// Characters kept of a shape's name or description at most.
const MAX_CHARS: usize = 32_768;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16_at(b, at)?) | (u32::from(u16_at(b, at.checked_add(2)?)?) << 16))
}

/// An OfficeArt record header at `pos`: `(recVer, recInstance, recType, body)`; `None` when the
/// header or the body doesn't fit in `data`.
fn header(data: &[u8], pos: usize) -> Option<(u16, u16, u16, &[u8])> {
    let ver_inst = u16_at(data, pos)?;
    let ty = u16_at(data, pos.checked_add(2)?)?;
    let len = usize::try_from(u32_at(data, pos.checked_add(4)?)?).ok()?;
    let start = pos.checked_add(8)?;
    let body = data.get(start..start.checked_add(len)?)?;
    Some((ver_inst & 0x000F, ver_inst >> 4, ty, body))
}

/// One BStore entry.
#[derive(Clone)]
pub(super) enum Blip {
    Picture {
        data: Arc<Vec<u8>>,
        mime: &'static str,
    },
    /// A blip type that isn't imported, or an entry whose blip isn't embedded.
    Other,
}

/// The workbook's picture store.
#[derive(Default)]
pub(super) struct BStore {
    entries: Vec<Blip>,
    /// The store couldn't be read (a warning said so): its pictures are skipped silently.
    pub damaged: bool,
}

impl BStore {
    /// The entry a `pib` names (1-based).
    pub(super) fn get(&self, pib: u32) -> Option<&Blip> {
        self.entries.get(usize::try_from(pib).ok()?.checked_sub(1)?)
    }
}

/// Reads the BStore from the bytes of the globals' MsoDrawingGroup records. A damaged container
/// gives an empty store and one warning.
pub(super) fn bstore(data: &[u8], budget: &mut Budget, report: &mut ReadReport) -> BStore {
    if data.is_empty() {
        return BStore::default();
    }
    match read_bstore(data, budget, report) {
        Some(entries) => BStore { entries, damaged: false },
        None => {
            report.warn("The pictures of this .xls file are damaged and were not imported.");
            BStore { entries: vec![], damaged: true }
        }
    }
}

fn read_bstore(data: &[u8], budget: &mut Budget, report: &mut ReadReport) -> Option<Vec<Blip>> {
    let (_, _, ty, dgg) = header(data, 0)?;
    if ty != DGG_CONTAINER {
        return None;
    }
    let mut pos = 0usize;
    while pos < dgg.len() {
        let (_, _, ty, body) = header(dgg, pos)?;
        if ty == BSTORE_CONTAINER {
            return entries(body, budget, report);
        }
        pos = pos.checked_add(8)?.checked_add(body.len())?;
    }
    Some(vec![])
}

fn entries(store: &[u8], budget: &mut Budget, report: &mut ReadReport) -> Option<Vec<Blip>> {
    let mut out = vec![];
    let mut pos = 0usize;
    while pos < store.len() && out.len() < MAX_ENTRIES {
        let (_, inst, ty, body) = header(store, pos)?;
        let blip = match ty {
            // OfficeArtFBSE: btWin32, btMacOS, rgbUid (16), tag (2), size, cRef, foDelay (4
            // each), unused1, cbName, unused2, unused3, nameData, then the embedded blip.
            FBSE => {
                let name_len = usize::from(*body.get(33)?);
                let at = 36usize.checked_add(name_len)?;
                if at >= body.len() { Blip::Other } else { blip(body, at)? }
            }
            _ => blip_record(inst, ty, body),
        };
        if let Blip::Picture { data, .. } = &blip
            && !budget.picture(data.len())
        {
            report.warn(format!("Only the first {} MB of pictures were imported.", MAX_PICTURE_BYTES / (1024 * 1024)));
            out.push(Blip::Other);
        } else {
            out.push(blip);
        }
        pos = pos.checked_add(8)?.checked_add(body.len())?;
    }
    Some(out)
}

/// The blip record at `pos` in an FBSE body.
fn blip(data: &[u8], pos: usize) -> Option<Blip> {
    let (_, inst, ty, body) = header(data, pos)?;
    Some(blip_record(inst, ty, body))
}

/// A blip record body: `rgbUid1`, `rgbUid2` for the two-UID instances, `tag`, then the file.
fn blip_record(inst: u16, ty: u16, body: &[u8]) -> Blip {
    let (mime, uids) = match (ty, inst) {
        (BLIP_JPEG, 0x46A | 0x6E2) => ("image/jpeg", 1),
        (BLIP_JPEG, 0x46B | 0x6E3) => ("image/jpeg", 2),
        (BLIP_PNG, 0x6E0) => ("image/png", 1),
        (BLIP_PNG, 0x6E1) => ("image/png", 2),
        _ => return Blip::Other,
    };
    match body.get(16 * uids + 1..) {
        Some(file) if !file.is_empty() => Blip::Picture { data: Arc::new(file.to_vec()), mime },
        _ => Blip::Other,
    }
}

/// What a shape's property tables say about it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct ShapeProps {
    pub pib: Option<u32>,
    pub name: Option<String>,
    pub alt: Option<String>,
    pub hidden: bool,
    /// The OfficeArtFSP instance: the shape type ([MS-ODRAW] 2.2.40, msospt*).
    pub shape_type: u16,
    /// fillColor and lineColor as OfficeArtCOLORREFs, whatever their flags say.
    pub fill_color: Option<u32>,
    pub line_color: Option<u32>,
    /// FillStyleBooleanProperties: `fUsefFilled` set and `fFilled` clear.
    pub no_fill: bool,
    /// LineStyleBooleanProperties: `fUsefLine` set and `fLine` clear.
    pub no_line: bool,
    /// lineStartArrowhead and lineEndArrowhead are not `none` (0).
    pub arrow: bool,
    /// The Obj record's FtCmo `fPrint` is clear ("Print object" off): left out of print and PDF.
    pub no_print: bool,
}

/// An OfficeArtCOLORREF as a model colour: bytes red, green, blue, flags. With `fSchemeIndex` the
/// red byte is the BIFF8 colour index (icv) into the workbook's colour table; system colours
/// (icv 64 and up) and any other flag are automatic.
pub(super) fn art_color(op: Option<u32>, table: &[u32; 64]) -> Color {
    let Some(op) = op else { return Color::Auto };
    let (red, green, blue, flags) = (op & 0xFF, (op >> 8) & 0xFF, (op >> 16) & 0xFF, (op >> 24) & 0xFF);
    if flags == 0 {
        return Color::Rgb((red << 16) | (green << 8) | blue);
    }
    if flags & COLOR_SCHEME_INDEX != 0 {
        // icv 8..=63 is the palette; 64 and up are system colours.
        return match usize::try_from(red) {
            Ok(icv) if (8..64).contains(&icv) => table.get(icv).map_or(Color::Auto, |c| Color::Rgb(*c)),
            _ => Color::Auto,
        };
    }
    Color::Auto
}

/// Reads an OfficeArtFOPT (or tertiary FOPT) body with `count` properties into `props`: a table
/// of 6-byte properties (`opid` with `fBid` and `fComplex` bits, `op`), then the complex
/// properties' data in table order.
pub(super) fn fopt(count: u16, body: &[u8], props: &mut ShapeProps) {
    let table = usize::from(count).saturating_mul(6);
    let mut complex = table;
    for i in 0..usize::from(count) {
        let at = i * 6;
        let (Some(opid), Some(op)) = (u16_at(body, at), u32_at(body, at.saturating_add(2))) else { break };
        let id = opid & 0x3FFF;
        let is_complex = opid & 0x8000 != 0;
        let extra = if is_complex { usize::try_from(op).unwrap_or(usize::MAX) } else { 0 };
        let data = body.get(complex..complex.saturating_add(extra)).unwrap_or_default();
        match id {
            PROP_PIB if !is_complex => props.pib = Some(op),
            PROP_NAME if is_complex => props.name = utf16(data),
            PROP_DESCRIPTION if is_complex => props.alt = utf16(data),
            PROP_GROUP_BOOLEANS if op & USE_HIDDEN != 0 => props.hidden = op & HIDDEN != 0,
            PROP_FILL_COLOR if !is_complex => props.fill_color = Some(op),
            PROP_LINE_COLOR if !is_complex => props.line_color = Some(op),
            PROP_FILL_BOOLEANS if !is_complex => props.no_fill = op & USE_FILLED != 0 && op & FILLED == 0,
            PROP_LINE_BOOLEANS if !is_complex => props.no_line = op & USE_LINE != 0 && op & LINE == 0,
            PROP_LINE_START_ARROWHEAD | PROP_LINE_END_ARROWHEAD if !is_complex => props.arrow |= op != 0,
            _ => {}
        }
        complex = complex.saturating_add(extra);
    }
}

/// A null-terminated (or not) UTF-16LE string; `None` when empty.
fn utf16(data: &[u8]) -> Option<String> {
    let units: Vec<u16> = data.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&u| u != 0).take(MAX_CHARS).collect();
    let s = String::from_utf16_lossy(&units);
    (!s.is_empty()).then_some(s)
}
