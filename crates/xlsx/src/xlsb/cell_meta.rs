//! Cell metadata of the XLSB metadata part ([MS-XLSB] 2.1.7.34): which cell-metadata blocks mark
//! a formula as a dynamic array. A worksheet's `BrtCellMeta` gives the one-based index (`icmb`) of
//! a `BrtMdb` among those after a `BrtBeginEsmdb` with fCellMeta set; its `Mdir` entries name a
//! metadata type (`BrtMdtinfo`) and, for the dynamic-array type "XLDAPR", the zero-based future
//! metadata record (`BrtBeginFmd`) whose `BrtEndDynamicArrayPr` carries fDynamic. This is the
//! binary form of `cm`, `<cellMetadata>` and `<futureMetadata name="XLDAPR">` in .xlsx.

use super::{Bytes, Package, Records, Rel, invalid};
use crate::{IoError, ReadReport};

/// The metadata type of dynamic-array formulas.
const XLDAPR: &str = "XLDAPR";
/// Most metadata types, future metadata records and cell-metadata blocks one part may declare.
const MAX_BLOCKS: usize = 100_000;
/// Largest metadata part read: room for `MAX_BLOCKS` blocks of a few `Mdir` entries each.
const MAX_PART: u64 = 16 * 1024 * 1024;

/// The workbook's cell-metadata blocks, by `BrtCellMeta` index.
pub(super) enum CellMeta {
    /// No readable metadata part: any cell metadata on a formula marks it dynamic, as `cm` does
    /// in the .xlsx reader.
    Unknown,
    /// Whether each cell-metadata block (`icmb` 1, 2, …) marks a dynamic array.
    Blocks(Vec<bool>),
}

impl CellMeta {
    /// Whether the cell-metadata block `icmb` marks a dynamic-array formula. An index the part
    /// doesn't declare falls back to the `cm` rule.
    pub(super) fn dynamic(&self, icmb: u32) -> bool {
        match self {
            Self::Unknown => true,
            Self::Blocks(blocks) => usize::try_from(icmb).ok().and_then(|i| i.checked_sub(1)).and_then(|i| blocks.get(i)).copied().unwrap_or(true),
        }
    }
}

/// Reads the part the workbook's `sheetMetadata` relationship points at. A missing part is
/// `Unknown` without a warning; a damaged one is `Unknown` with one.
pub(super) fn read(package: &mut Package<'_>, rels: &[Rel], report: &mut ReadReport) -> CellMeta {
    let Some(rel) = rels.iter().find(|r| r.kind == "sheetMetadata") else { return CellMeta::Unknown };
    let data = if rel.external { Err(invalid("external metadata relationship")) } else { package.read_limited(&rel.target, MAX_PART) };
    let parsed = match data {
        Ok(None) => return CellMeta::Unknown,
        Ok(Some(data)) => parse(&data),
        Err(e) => Err(e),
    };
    match parsed {
        Ok(blocks) => CellMeta::Blocks(blocks),
        Err(e) => {
            report.warn(format!("The damaged XLSB cell metadata part was skipped; formulas with cell metadata open as dynamic arrays: {e}"));
            CellMeta::Unknown
        }
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<bool>, IoError> {
    // Future records (BrtFRTBegin … BrtFRTEnd) hold the dynamic-array properties: read them.
    let mut records = Records::raw(bytes);
    let mut quiet = ReadReport::default();
    records.first(332, &mut quiet)?;
    let mut types: Vec<String> = vec![];
    // `Some(name is XLDAPR)` between BrtBeginEsfmd and BrtEndEsfmd.
    let mut esfmd: Option<bool> = None;
    // fDynamic of each XLDAPR future metadata record.
    let mut fmds: Vec<bool> = vec![];
    // `Some(fCellMeta)` between BrtBeginEsmdb and BrtEndEsmdb.
    let mut esmdb: Option<bool> = None;
    // For each cell-metadata block, the XLDAPR record it refers to.
    let mut blocks: Vec<Option<u32>> = vec![];
    let full = || IoError::TooLarge("XLSB metadata part declares too many records".into());
    while let Some((id, payload)) = records.next(&mut quiet)? {
        let mut b = Bytes::new(payload);
        match id {
            333 => {
                records.end(payload, &mut quiet)?;
                let dynamic = |mdd: u32| usize::try_from(mdd).ok().and_then(|i| fmds.get(i)).copied().unwrap_or(false);
                return Ok(blocks.iter().map(|b| b.is_some_and(dynamic)).collect());
            }
            // BrtMdtinfo: grbit, metadataID, stName.
            335 => {
                if types.len() >= MAX_BLOCKS {
                    return Err(full());
                }
                b.u32()?;
                b.u32()?;
                types.push(b.text(65535)?);
            }
            // BrtBeginEsfmd: cFmd, stName.
            339 => {
                b.u32()?;
                esfmd = Some(b.text(65535)? == XLDAPR);
            }
            340 => esfmd = None,
            // BrtBeginFmd: one future metadata record of the open collection.
            52 if esfmd == Some(true) => {
                if fmds.len() >= MAX_BLOCKS {
                    return Err(full());
                }
                fmds.push(false);
            }
            // BrtEndDynamicArrayPr: fDynamic, fCollapsed (one byte each).
            4097 if esfmd == Some(true) => {
                let flag = b.u8()?;
                b.u8()?;
                let last = fmds.last_mut().ok_or_else(|| invalid("dynamic-array properties outside a metadata record"))?;
                *last = flag != 0;
            }
            // BrtBeginEsmdb: cMdb, fCellMeta.
            337 => {
                b.u32()?;
                esmdb = Some(b.u32()? == 1);
            }
            338 => esmdb = None,
            // BrtMdb: cMdir, then Mdir entries of iMdt (one-based type index) and mdd.
            51 if esmdb == Some(true) => {
                if blocks.len() >= MAX_BLOCKS {
                    return Err(full());
                }
                let count = usize::try_from(b.u32()?).unwrap_or(usize::MAX);
                if count > payload.len() / 8 {
                    return Err(invalid("metadata block runs past its record"));
                }
                let mut xldapr = None;
                for _ in 0..count {
                    let imdt = b.u32()?;
                    let mdd = b.u32()?;
                    let name = usize::try_from(imdt).ok().and_then(|i| i.checked_sub(1)).and_then(|i| types.get(i));
                    if name.is_some_and(|n| n == XLDAPR) {
                        xldapr = Some(mdd);
                    }
                }
                blocks.push(xldapr);
            }
            _ => {}
        }
    }
    Err(invalid("missing metadata end record"))
}
