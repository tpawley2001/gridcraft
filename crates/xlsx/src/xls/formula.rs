//! BIFF8 parsed formulas ([MS-XLS] 2.2.2, 2.5.198): reverse-Polish `Ptg` tokens back to an
//! expression tree. Anything outside the common token set is reported as unsupported, and the
//! caller keeps the cell's cached value instead. XLSB ([MS-XLSB] 2.2.2, 2.5.97) cell formulas
//! use the same token ids with wider cell coordinates; see [`Grid`]. XLSB structured references
//! (`PtgList`) resolve their table and columns through [`Links::lists`].

use std::sync::Arc;

use gridcraft_core::{CellError, CellRef, RangeRef};
use gridcraft_formula::{Anchor, BinOp, Expr, RefKind, Reference, SheetSel, StructItem, StructRef, UnOp};

use super::cetab::cetab;
use super::ftab::FTAB;

/// Formula token bytes decoded at most (Excel's limit is 1,800 per formula).
const MAX_RGCE: usize = 16 * 1024;
/// Values in one array constant at most.
const MAX_ARRAY: usize = 65_536;

/// Why a formula could not be turned back into text.
#[derive(Debug, PartialEq)]
pub(crate) struct Unsupported(pub &'static str);

/// The coordinate layout of a file format's references.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Grid {
    /// BIFF8 (.xls): u16 rows, 8-bit columns (65,536 × 256).
    Biff8,
    /// BIFF5 (Excel 5.0/95 .xls): 14-bit rows whose top two bits are the column and row
    /// relative flags, then an 8-bit column (16,384 × 256).
    Biff5,
    /// BIFF12 (.xlsb): u32 rows, 14-bit columns (1,048,576 × 16,384), `RgceLoc` / `RgceArea`.
    Biff12,
}

impl Grid {
    fn last_row(self) -> u32 {
        match self {
            Grid::Biff8 => 0xFFFF,
            Grid::Biff5 => 0x3FFF,
            Grid::Biff12 => 0xF_FFFF,
        }
    }
    fn last_col(self) -> u32 {
        match self {
            Grid::Biff8 | Grid::Biff5 => 0xFF,
            Grid::Biff12 => 0x3FFF,
        }
    }
    fn row(self, t: &mut Bytes<'_>) -> Result<u32, Unsupported> {
        match self {
            Grid::Biff8 | Grid::Biff5 => t.u16().map(u32::from),
            Grid::Biff12 => t.u32(),
        }
    }
    /// A cell: row then column. BIFF5's relative flags move into a BIFF8-style column field.
    fn loc(self, t: &mut Bytes<'_>) -> Result<(u32, u16), Unsupported> {
        if self == Grid::Biff5 {
            let rw = t.u16()?;
            let col = t.u8()?;
            return Ok((u32::from(rw & 0x3FFF), u16::from(col) | (rw & 0xC000)));
        }
        Ok((self.row(t)?, t.u16()?))
    }
    /// An area: first and last row, then first and last column.
    fn area(self, t: &mut Bytes<'_>) -> Result<(u32, u32, u16, u16), Unsupported> {
        if self == Grid::Biff5 {
            let (r0, r1) = (t.u16()?, t.u16()?);
            let (c0, c1) = (t.u8()?, t.u8()?);
            return Ok((u32::from(r0 & 0x3FFF), u32::from(r1 & 0x3FFF), u16::from(c0) | (r0 & 0xC000), u16::from(c1) | (r1 & 0xC000)));
        }
        Ok((self.row(t)?, self.row(t)?, t.u16()?, t.u16()?))
    }
    /// Bytes of a `loc` (`area` is twice that).
    fn loc_len(self) -> usize {
        match self {
            Grid::Biff8 => 4,
            Grid::Biff5 => 3,
            Grid::Biff12 => 6,
        }
    }
}

/// Where the formula lives: refs in cell formulas store coordinates; shared formulas store
/// row/column offsets for relative parts; names store offsets for 3-D refs too.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Mode {
    Cell,
    Shared,
    Name,
    /// .xls NAME records: shared-formula offset tokens become #N/A, as in Excel.
    XlsName,
}

impl Mode {
    fn is_name(self) -> bool {
        matches!(self, Self::Name | Self::XlsName)
    }
}

/// A supporting link ([MS-XLS] 2.4.271): only links to this workbook (and add-in function
/// names) are resolved; external workbooks are never followed.
#[derive(Default)]
pub(crate) struct SupBook {
    pub internal: bool,
    pub addin: bool,
    pub names: Vec<String>,
    /// XLSB BrtSupBookSrc: the relationship id of the external link part.
    pub rel: Option<String>,
    /// The external workbook as its link was read; `None` until then (or when links can't be
    /// kept), so references into it stay unsupported.
    pub external: Option<ExternalBook>,
}

/// An external workbook a formula can name as `[number]Sheet!A1`.
pub(crate) struct ExternalBook {
    /// 1-based position among the workbook's links to other workbooks (the `.xlsx`
    /// `<externalReferences>` order).
    pub number: usize,
    /// The linked workbook's sheet names (BrtSupTabs).
    pub sheets: Vec<String>,
    /// Its defined names (`.xls` EXTERNNAME), index-aligned with the records; `None` for an
    /// entry that can't be used.
    pub names: Vec<Option<crate::external_link::LinkName>>,
}

/// XTI ([MS-XLS] 2.5.344, [MS-XLSB] 2.5.172): a supporting link and a range of its sheets.
/// BIFF8 stores 16-bit fields, BIFF12 32-bit ones.
#[derive(Clone, Copy)]
pub(crate) struct Xti {
    pub book: u32,
    pub first: i32,
    pub last: i32,
}

/// Workbook-level tables formulas refer to.
#[derive(Default)]
pub(crate) struct Links {
    /// Every sheet's name in BoundSheet8 order (including skipped chart/macro sheets).
    pub sheets: Vec<String>,
    /// Defined names (Lbl records), in order; `None` for names that can't be referenced.
    pub names: Vec<String>,
    /// Per defined name: whether the import skips it (its formula can't be decoded).
    pub skipped: Vec<bool>,
    pub books: Vec<SupBook>,
    pub xti: Vec<Xti>,
    /// XLSB tables (ListObjects) structured references can name.
    pub lists: Vec<List>,
    /// For a calculated PivotTable field's formula: the names of the cache fields its
    /// `PtgSxName` tokens refer to, in `BrtBeginPName` order (empty: none can be referred to).
    pub pivot_fields: Vec<String>,
}

/// An XLSB table as `PtgList` names it: by `idList`, and its columns by index.
pub(crate) struct List {
    pub id: u32,
    pub name: String,
    pub columns: Vec<String>,
    /// The sheet holding the table, and its range there.
    pub sheet: String,
    pub range: RangeRef,
}

/// Where a formula lives, including XLSB structured references that omit their table name.
#[derive(Clone, Copy, Default)]
pub(crate) struct Site<'a> {
    /// The sheet holding the formula (empty for defined names).
    pub sheet: &'a str,
    /// The table (`idList`) whose calculated column or totals row formula this is.
    pub table: Option<u32>,
    /// Whether references into other workbooks read (see [`ExternalBook`]) decode to `[n]Sheet`.
    pub external: bool,
    /// Whether cell formulas belong to an Excel 4.0 macro sheet. Commands in ordinary
    /// sheets must keep their cached values to avoid invalid worksheet formulas on save.
    pub commands: bool,
}

/// What a BIFF5 EXTERNSHEET entry points at.
#[derive(Clone, PartialEq)]
pub(super) enum ExternKind {
    /// A sheet of this workbook, by name.
    Sheet(String),
    /// This workbook (3-D refs give the sheets by index; names are its own).
    OwnBook,
    /// Add-in functions.
    AddIn,
    /// Another workbook.
    Other,
}

/// A BIFF5 EXTERNSHEET entry and the EXTERNNAME records after it.
pub(super) struct Extern {
    pub kind: ExternKind,
    pub names: Vec<String>,
}

/// BIFF5 (Excel 5.0/95) workbook tables: the code page of its 8-bit text, and the
/// EXTERNSHEET entries that 3-D references and external names go through (BIFF8 uses XTI).
#[derive(Default)]
pub(super) struct Biff5 {
    pub codepage: u16,
    pub externs: Vec<Extern>,
}

impl Biff5 {
    /// 8-bit text in the workbook's code page.
    pub fn text(&self, bytes: &[u8]) -> String {
        crate::csv::decode_codepage(bytes, self.codepage)
    }

    /// What an EXTERNSHEET entry's encoded target (after its 1-byte count) points at.
    pub fn extern_kind(&self, raw: &[u8]) -> ExternKind {
        match raw.split_first() {
            Some((0x03, name)) => ExternKind::Sheet(self.text(name)),
            Some((0x02 | 0x04, _)) => ExternKind::OwnBook,
            Some((b':', [])) => ExternKind::AddIn,
            _ => ExternKind::Other,
        }
    }

    fn entry(&self, ixals: i16) -> Option<&Extern> {
        usize::from(ixals.unsigned_abs()).checked_sub(1).and_then(|i| self.externs.get(i))
    }

    /// The sheets of a BIFF5 3-D reference: a negative `ixals` is a reference into this
    /// workbook by sheet index; a positive one names its sheet through an EXTERNSHEET entry.
    fn sheet_sel(&self, links: &Links, ixals: i16, first: i16, last: i16) -> Result<Option<SheetSel>, Unsupported> {
        let entry = self.entry(ixals);
        if ixals > 0 {
            match entry.map(|e| &e.kind) {
                Some(ExternKind::Sheet(name)) => {
                    let sheet = links.sheets.iter().find(|s| s.eq_ignore_ascii_case(name)).ok_or(Unsupported("reference to a missing sheet link"))?;
                    return Ok(Some(SheetSel::Named(sheet.clone())));
                }
                Some(ExternKind::OwnBook) => {}
                Some(_) => return Err(Unsupported("references to other workbooks (external links)")),
                None => return Err(Unsupported("reference to a missing sheet link")),
            }
        }
        let name = |i: i16| usize::try_from(i).ok().and_then(|i| links.sheets.get(i)).cloned();
        Ok(match (name(first), name(last)) {
            (Some(a), Some(_)) if first == last => Some(SheetSel::Named(a)),
            (Some(a), Some(b)) => Some(SheetSel::Span(a, b)),
            // A deleted sheet: the reference is #REF!.
            _ => None,
        })
    }

    fn extern_name(&self, ixals: i16, index: u16) -> Result<String, Unsupported> {
        let entry = self.entry(ixals).ok_or(Unsupported("reference to a missing name link"))?;
        if !matches!(entry.kind, ExternKind::OwnBook | ExternKind::AddIn) {
            return Err(Unsupported("names in other workbooks (external links)"));
        }
        let i = usize::from(index).checked_sub(1).ok_or(Unsupported("invalid name index"))?;
        entry.names.get(i).cloned().ok_or(Unsupported("reference to a missing name"))
    }
}

pub(super) fn function(iftab: u16) -> Option<(&'static str, i8)> {
    FTAB.binary_search_by_key(&iftab, |(i, _, _)| *i).ok().and_then(|i| FTAB.get(i)).map(|(_, n, a)| (*n, *a))
}

pub(super) fn error_code(code: u8) -> Option<CellError> {
    Some(match code {
        0x00 => CellError::Null,
        0x07 => CellError::Div0,
        0x0F => CellError::Value,
        0x17 => CellError::Ref,
        0x1D => CellError::Name,
        0x24 => CellError::Num,
        0x2A => CellError::NA,
        0x2B => CellError::GettingData,
        _ => return None,
    })
}

pub(super) fn cell_error(code: u8) -> CellError {
    error_code(code).unwrap_or(CellError::NA)
}

struct Bytes<'a> {
    b: &'a [u8],
    pos: usize,
}

const TRUNC: Unsupported = Unsupported("truncated formula");

impl Bytes<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], Unsupported> {
        let s = self.b.get(self.pos..self.pos.checked_add(n).ok_or(TRUNC)?).ok_or(TRUNC)?;
        self.pos += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, Unsupported> {
        self.take(1)?.first().copied().ok_or(TRUNC)
    }
    fn u16(&mut self) -> Result<u16, Unsupported> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([*s.first().ok_or(TRUNC)?, *s.get(1).ok_or(TRUNC)?]))
    }
    fn u32(&mut self) -> Result<u32, Unsupported> {
        Ok(u32::from(self.u16()?) | (u32::from(self.u16()?) << 16))
    }
    fn f64(&mut self) -> Result<f64, Unsupported> {
        let a: [u8; 8] = self.take(8)?.try_into().map_err(|_| TRUNC)?;
        Ok(f64::from_le_bytes(a))
    }
    fn chars(&mut self, cch: usize, high: bool) -> Result<String, Unsupported> {
        let units: Vec<u16> = if high {
            self.take(cch.checked_mul(2).ok_or(TRUNC)?)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()
        } else {
            self.take(cch)?.iter().map(|&b| u16::from(b)).collect()
        };
        Ok(String::from_utf16_lossy(&units))
    }
}

/// A column field (ColRelU / ColRelNegU): the 14-bit column and the two relative flags.
fn col_field(v: u16) -> (u16, bool, bool) {
    (v & 0x3FFF, v & 0x4000 != 0, v & 0x8000 != 0)
}

/// One end of a reference. In `relative` (RgceLocRel) form the relative parts are offsets
/// from `base`, wrapped into the grid (65,536 × 256 for BIFF8).
fn anchor(row: u32, col: u16, base: CellRef, relative: bool, grid: Grid) -> Result<Anchor, Unsupported> {
    let (c, col_rel, row_rel) = col_field(col);
    let (last_row, last_col) = (grid.last_row(), grid.last_col());
    let (r, c) = if relative {
        let r = if row_rel { row.wrapping_add(base.row) & last_row } else { row };
        // A 14-bit signed offset; adding modulo the column count gives the same column either way.
        let c = if col_rel { (u32::from(c).wrapping_add(base.col)) & last_col } else { u32::from(c) & last_col };
        (r, c)
    } else {
        (row, u32::from(c) & last_col)
    };
    if r > last_row {
        return Err(Unsupported("reference outside the sheet"));
    }
    Ok(Anchor { row: r, col: c, row_abs: !row_rel, col_abs: !col_rel })
}

/// An area; a range spanning every row (or column) of the grid becomes a whole-column (row) ref.
fn area(r0: u32, r1: u32, c0: u16, c1: u16, base: CellRef, relative: bool, grid: Grid) -> Result<RefKind, Unsupported> {
    let a = anchor(r0, c0, base, relative, grid)?;
    let b = anchor(r1, c1, base, relative, grid)?;
    if a.row == 0 && b.row == grid.last_row() {
        return Ok(RefKind::Cols(a.col, a.col_abs, b.col, b.col_abs));
    }
    if a.col == 0 && b.col == grid.last_col() {
        return Ok(RefKind::Rows(a.row, a.row_abs, b.row, b.row_abs));
    }
    Ok(RefKind::Range(a, b))
}

impl Links {
    /// The sheets XTI `ixti` names; with `external`, a read external workbook's sheets as
    /// `[n]Sheet` (a span as `[n]First` to `Last`), as the `.xlsx` formula text holds them.
    fn sheet_sel(&self, ixti: u16, external: bool) -> Result<Option<SheetSel>, Unsupported> {
        let x = self.xti.get(usize::from(ixti)).ok_or(Unsupported("reference to a missing sheet link"))?;
        let book = usize::try_from(x.book).ok().and_then(|b| self.books.get(b)).ok_or(Unsupported("reference to a missing workbook link"))?;
        if !book.internal {
            let ext = book.external.as_ref().filter(|_| external).ok_or(Unsupported("references to other workbooks (external links)"))?;
            let name = |i: i32| usize::try_from(i).ok().and_then(|i| ext.sheets.get(i)).cloned();
            return Ok(match (name(x.first), name(x.last)) {
                (Some(a), Some(_)) if x.first == x.last => Some(SheetSel::Named(format!("[{}]{a}", ext.number))),
                (Some(a), Some(b)) => Some(SheetSel::Span(format!("[{}]{a}", ext.number), b)),
                _ => None,
            });
        }
        let name = |i: i32| usize::try_from(i).ok().and_then(|i| self.sheets.get(i)).cloned();
        Ok(match (name(x.first), name(x.last)) {
            (Some(a), Some(_)) if x.first == x.last => Some(SheetSel::Named(a)),
            (Some(a), Some(b)) => Some(SheetSel::Span(a, b)),
            // A deleted sheet: the reference is #REF!.
            _ => None,
        })
    }

    fn extern_name(&self, ixti: u16, index: u32) -> Result<String, Unsupported> {
        let x = self.xti.get(usize::from(ixti)).ok_or(Unsupported("reference to a missing name link"))?;
        let book = usize::try_from(x.book).ok().and_then(|b| self.books.get(b)).ok_or(Unsupported("reference to a missing workbook link"))?;
        let i = usize::try_from(index).ok().and_then(|i| i.checked_sub(1)).ok_or(Unsupported("invalid name index"))?;
        if let Some(ext) = &book.external {
            // A name of a linked workbook: `[1]!Rate`, or `[1]Totals!Limit` when sheet-level.
            let n = ext.names.get(i).and_then(Option::as_ref).ok_or(Unsupported("a name in another workbook that can't be read"))?;
            return match n.sheet {
                None => Ok(format!("[{}]!{}", ext.number, n.name)),
                Some(s) => {
                    let sheet = ext.sheets.get(s).ok_or(Unsupported("a name in another workbook that can't be read"))?;
                    Ok(format!("[{}]{sheet}!{}", ext.number, n.name))
                }
            };
        }
        if !book.internal && !book.addin {
            return Err(Unsupported("names in other workbooks (external links)"));
        }
        book.names.get(i).cloned().ok_or(Unsupported("reference to a missing name"))
    }
}

/// Strips the prefixes newer functions carry in files (`_xlfn.IFERROR`).
fn function_name(name: &str) -> String {
    let n = name.strip_prefix("_xlfn.").or_else(|| name.strip_prefix("_xlws.")).unwrap_or(name);
    n.to_ascii_uppercase()
}

/// Expression nesting at most, as in the formula parser: deeper trees would overflow the stack
/// when printed, evaluated or dropped (a hostile file can chain thousands of PtgParen tokens).
const MAX_DEPTH: usize = 256;

/// Operand stack entries: an expression and its nesting depth.
type Stack = Vec<(Expr, usize)>;

/// Pops an operand, recording its depth in `kids` (the deepest operand this token consumed).
fn pop(stack: &mut Stack, kids: &mut usize) -> Result<Expr, Unsupported> {
    let (e, d) = stack.pop().ok_or(Unsupported("malformed formula (operand missing)"))?;
    *kids = (*kids).max(d);
    Ok(e)
}

fn pop_n(stack: &mut Stack, n: usize, kids: &mut usize) -> Result<Vec<Expr>, Unsupported> {
    let at = stack.len().checked_sub(n).ok_or(Unsupported("malformed formula (argument missing)"))?;
    let popped = stack.split_off(at);
    *kids = popped.iter().map(|(_, d)| *d).fold(*kids, usize::max);
    Ok(popped.into_iter().map(|(e, _)| e).collect())
}

/// Pushes the result of a token one level above the operands it consumed.
fn push(stack: &mut Stack, kids: usize, e: Expr) -> Result<(), Unsupported> {
    let depth = kids.saturating_add(1);
    if depth > MAX_DEPTH {
        return Err(Unsupported("formula nested too deeply"));
    }
    stack.push((e, depth));
    Ok(())
}

/// Decodes `rgce` (tokens) with `rgcb` (the extra data for array constants) evaluated at
/// `base`. Returns the expression or why it can't be represented.
pub(super) fn decode(rgce: &[u8], rgcb: &[u8], base: CellRef, mode: Mode, links: &Links) -> Result<Expr, Unsupported> {
    decode_in(rgce, rgcb, base, mode, links, Grid::Biff8, Site::default(), None)
}

/// [`decode`] for a BIFF8 workbook (`biff5` is `None`) or a BIFF5 one.
pub(super) fn decode_version(rgce: &[u8], rgcb: &[u8], base: CellRef, mode: Mode, links: &Links, biff5: Option<&Biff5>) -> Result<Expr, Unsupported> {
    let grid = if biff5.is_some() { Grid::Biff5 } else { Grid::Biff8 };
    decode_in(rgce, rgcb, base, mode, links, grid, Site::default(), biff5)
}

/// [`decode_version`] for a cell formula: references into the other workbooks read (see
/// [`ExternalBook`]) decode to `[n]Sheet`.
pub(super) fn decode_cell(
    rgce: &[u8],
    rgcb: &[u8],
    base: CellRef,
    mode: Mode,
    links: &Links,
    biff5: Option<&Biff5>,
    commands: bool,
) -> Result<Expr, Unsupported> {
    let grid = if biff5.is_some() { Grid::Biff5 } else { Grid::Biff8 };
    decode_in(rgce, rgcb, base, mode, links, grid, Site { external: true, commands, ..Site::default() }, biff5)
}

/// Decodes an XLSB cell, shared, array or name formula's `rgce` and `rgcb` at `base`. Shared
/// formulas' `PtgRefN` / `PtgAreaN` hold offsets from it; defined names and other sheets resolve
/// through `links`. `PtgArray` and `PtgMemArea` consume their corresponding extra data.
/// A `PtgExp` (resolved by the reader) and names in other workbooks (`PtgNameX`) are refused.
pub(crate) fn decode_xlsb(rgce: &[u8], rgcb: &[u8], base: CellRef, mode: Mode, links: &Links) -> Result<Expr, Unsupported> {
    decode_xlsb_at(rgce, rgcb, base, mode, links, Site::default())
}

/// [`decode_xlsb`] for a formula at `site`: structured references from inside their own table
/// are written without the table name (`[@Qty]`), as the .xlsx reader gives them.
pub(crate) fn decode_xlsb_at(rgce: &[u8], rgcb: &[u8], base: CellRef, mode: Mode, links: &Links, site: Site<'_>) -> Result<Expr, Unsupported> {
    decode_in(rgce, rgcb, base, mode, links, Grid::Biff12, site, None)
}

/// Formula text printed from an expression with external sheets (`[1]Data`, see
/// [`ExternalBook`]) written the way `.xlsx` files hold them: the printer quotes every such name
/// (`'[1]Data'!B2`); the quotes are left only when the sheet name needs them
/// (`'[1]My Data'!B2`). Also returns whether the text refers to another workbook.
pub(crate) fn external_sheet_text(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut external = false;
    let mut in_string = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if in_string || c != '\'' {
            if c == '"' {
                in_string = !in_string;
            }
            out.push(c);
            continue;
        }
        // A quoted sheet name: up to the closing quote, `''` standing for one quote.
        let mut inner = String::new();
        let mut end = text.len();
        while let Some((j, d)) = chars.next() {
            if d == '\'' {
                if chars.peek().is_some_and(|&(_, e)| e == '\'') {
                    chars.next();
                    inner.push('\'');
                    continue;
                }
                end = j + 1;
                break;
            }
            inner.push(d);
        }
        let quoted = text.get(i..end).unwrap_or_default();
        let book = inner.strip_prefix('[').and_then(|r| r.split_once(']')).filter(|(n, _)| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
        match book {
            Some((n, rest)) if text.get(end..).is_some_and(|t| t.starts_with('!')) => {
                external = true;
                if !rest.is_empty() && rest.split(':').all(|s| !gridcraft_core::sheet_needs_quotes(s)) {
                    out.push('[');
                    out.push_str(n);
                    out.push(']');
                    out.push_str(rest);
                } else {
                    out.push_str(quoted);
                }
            }
            _ => out.push_str(quoted),
        }
    }
    (out, external)
}

/// `PtgList` ([MS-XLSB] 2.5.98, after its first byte): eptg 0x19, ixti, flags (columns: 2 bits,
/// rowType: 5 bits, spacing and data type bits, then invalid and nonresident), listIndex,
/// colFirst and colLast. A table or column the workbook doesn't have is refused.
fn list_ref(t: &mut Bytes<'_>, base: CellRef, links: &Links, site: Site<'_>) -> Result<Expr, Unsupported> {
    if t.u8()? != 0x19 {
        return Err(Unsupported("XLSB extended formula token"));
    }
    let ixti = t.u16()?;
    let flags = t.u16()?;
    let id = t.u32()?;
    let (c0, c1) = (t.u16()?, t.u16()?);
    if flags & 0x3000 != 0 {
        return Err(Unsupported("invalid structured references or ones to other workbooks"));
    }
    let book = links.xti.get(usize::from(ixti)).and_then(|x| usize::try_from(x.book).ok()).and_then(|b| links.books.get(b));
    if book.is_some_and(|b| !b.internal) {
        return Err(Unsupported("references to other workbooks (external links)"));
    }
    let list = links.lists.iter().find(|l| l.id == id).ok_or(Unsupported("reference to a missing table"))?;
    let specifiers = match (flags >> 2) & 0x1F {
        0x00 => vec![],
        0x01 => vec![StructItem::All],
        0x02 => vec![StructItem::Headers],
        0x04 => vec![StructItem::Data],
        0x06 => vec![StructItem::Headers, StructItem::Data],
        0x08 => vec![StructItem::Totals],
        0x0C => vec![StructItem::Data, StructItem::Totals],
        0x10 => vec![StructItem::ThisRow],
        _ => return Err(Unsupported("unknown structured reference rows")),
    };
    let col = |c: u16| list.columns.get(usize::from(c)).cloned().ok_or(Unsupported("reference to a missing table column"));
    let (col_start, col_end) = match flags & 0x3 {
        0 => (None, None),
        1 | 2 => {
            let (a, b) = (col(c0)?, col(c1)?);
            if flags & 0x3 == 1 || c0 == c1 { (Some(a), None) } else { (Some(a), Some(b)) }
        }
        _ => return Err(Unsupported("unknown structured reference columns")),
    };
    let own = site.table == Some(id) || (specifiers == [StructItem::ThisRow] && list.sheet == site.sheet && list.range.contains(base));
    let table = if own { String::new() } else { list.name.clone() };
    Ok(Expr::Struct(StructRef { table, specifiers, col_start, col_end }))
}

/// `PtgSxName` ([MS-XLSB] 2.5.98.16: first byte 0x18, eptg 0x1D, then a 4-byte `sxIndex`): a
/// field in a calculated PivotTable field's formula, by its index in the field's `PNAMES`
/// collection. Written as the field name, in single quotes when a sheet name would need them
/// (`'Unit Price'`).
fn sx_name(t: &mut Bytes<'_>, links: &Links) -> Result<Expr, Unsupported> {
    let i = usize::try_from(t.u32()?).map_err(|_| TRUNC)?;
    let name = links.pivot_fields.get(i).filter(|n| !n.is_empty()).ok_or(Unsupported("reference to a missing PivotTable field"))?;
    if name.contains('!') {
        return Err(Unsupported("PivotTable field names with '!'"));
    }
    Ok(Expr::Name(gridcraft_core::quote_sheet(name)))
}

/// BIFF5 3-D tokens: `ixals`, 8 reserved bytes, then the first and last sheet index.
const BIFF5_3D_HEADER: usize = 14;

#[allow(clippy::too_many_arguments)]
fn decode_in(
    rgce: &[u8],
    rgcb: &[u8],
    base: CellRef,
    mode: Mode,
    links: &Links,
    grid: Grid,
    site: Site<'_>,
    biff5: Option<&Biff5>,
) -> Result<Expr, Unsupported> {
    let wide = grid == Grid::Biff12;
    if rgce.len() > MAX_RGCE {
        return Err(Unsupported("formula too long"));
    }
    let mut t = Bytes { b: rgce, pos: 0 };
    let mut extra = Bytes { b: rgcb, pos: 0 };
    let mut stack: Stack = Vec::new();
    while t.pos < rgce.len() {
        let ptg = t.u8()?;
        let mut kids = 0usize;
        if wide && ptg & 0x1F == 0x18 && rgce.get(t.pos) == Some(&0x1D) {
            t.u8()?;
            push(&mut stack, kids, sx_name(&mut t, links)?)?;
            continue;
        }
        if wide && ptg & 0x1F == 0x18 {
            // PtgList, whichever operand class its first byte carries.
            push(&mut stack, kids, list_ref(&mut t, base, links, site)?)?;
            continue;
        }
        // Operand classes (reference / value / array) share one token number.
        let id = if ptg & 0x60 != 0 { 0x20 | (ptg & 0x1F) } else { ptg };
        if wide && id == 0x39 {
            // External workbook names are not resolved by this import.
            return Err(Unsupported("XLSB formula token"));
        }
        // Offset tokens have no base cell in an .xls NAME record. Keep the name as
        // #N/A like Excel; emitting wrapped A1 references can require repair on XLSX save.
        if mode == Mode::XlsName && matches!(id, 0x2C | 0x2D) {
            return Ok(Expr::Error(CellError::NA));
        }
        match id {
            0x03..=0x11 => {
                let op = match id {
                    0x03 => BinOp::Add,
                    0x04 => BinOp::Sub,
                    0x05 => BinOp::Mul,
                    0x06 => BinOp::Div,
                    0x07 => BinOp::Pow,
                    0x08 => BinOp::Concat,
                    0x09 => BinOp::Lt,
                    0x0A => BinOp::Le,
                    0x0B => BinOp::Eq,
                    0x0C => BinOp::Ge,
                    0x0D => BinOp::Gt,
                    0x0E => BinOp::Ne,
                    0x0F => BinOp::Intersect,
                    0x10 => BinOp::Union,
                    _ => BinOp::Range,
                };
                let b = pop(&mut stack, &mut kids)?;
                let a = pop(&mut stack, &mut kids)?;
                push(&mut stack, kids, Expr::Binary(op, Box::new(a), Box::new(b)))?;
            }
            0x12..=0x14 => {
                let op = match id {
                    0x12 => UnOp::Plus,
                    0x13 => UnOp::Neg,
                    _ => UnOp::Percent,
                };
                let x = pop(&mut stack, &mut kids)?;
                push(&mut stack, kids, Expr::Unary(op, Box::new(x)))?;
            }
            0x15 => {
                let x = pop(&mut stack, &mut kids)?;
                push(&mut stack, kids, Expr::Paren(Box::new(x)))?;
            }
            0x16 => push(&mut stack, kids, Expr::Missing)?,
            0x17 if biff5.is_some() => {
                // BIFF5: a count and 8-bit characters in the workbook's code page.
                let cch = usize::from(t.u8()?);
                let bytes = t.take(cch)?;
                let text = biff5.map(|b5| b5.text(bytes)).unwrap_or_default();
                push(&mut stack, kids, Expr::Text(Arc::from(text)))?;
            }
            0x17 => {
                let (cch, high) = if wide {
                    let cch = usize::from(t.u16()?);
                    if cch > 255 {
                        return Err(Unsupported("XLSB text constant too long"));
                    }
                    (cch, true)
                } else {
                    (usize::from(t.u8()?), t.u8()? & 1 != 0)
                };
                push(&mut stack, kids, Expr::Text(Arc::from(t.chars(cch, high)?)))?;
            }
            0x19 => {
                let kind = t.u8()?;
                if kind & 0x04 != 0 {
                    // PtgAttrChoose: jump table of cOffset + 1 entries.
                    let n = usize::from(t.u16()?) + 1;
                    t.take(n * 2)?;
                } else {
                    t.take(2)?;
                }
                if kind & 0x10 != 0 {
                    // PtgAttrSum: SUM of the single operand.
                    let x = pop(&mut stack, &mut kids)?;
                    push(&mut stack, kids, Expr::Call("SUM".into(), vec![x]))?;
                }
                // Semi (volatile), If, Goto, Space, Baxcel: no effect on the expression.
            }
            0x1C => {
                let e = t.u8()?;
                push(&mut stack, kids, Expr::Error(error_code(e).ok_or(Unsupported("unknown error constant"))?))?;
            }
            0x1D => push(&mut stack, kids, Expr::Bool(t.u8()? != 0))?,
            0x1E => push(&mut stack, kids, Expr::Number(f64::from(t.u16()?)))?,
            0x1F => {
                let n = t.f64()?;
                if !n.is_finite() {
                    return Err(Unsupported("invalid number constant"));
                }
                push(&mut stack, kids, Expr::Number(n))?;
            }
            0x20 => {
                t.take(if wide { 14 } else { 7 })?;
                let array = if wide { array_constant_xlsb(&mut extra)? } else { array_constant(&mut extra, biff5.is_some())? };
                push(&mut stack, kids, array)?;
            }
            0x21 => {
                let iftab = t.u16()?;
                let (name, args) = function(iftab).ok_or(Unsupported("unknown function"))?;
                let n = usize::try_from(args).map_err(|_| Unsupported("unknown function argument count"))?;
                let args = pop_n(&mut stack, n, &mut kids)?;
                push(&mut stack, kids, Expr::Call(name.to_string(), args))?;
            }
            0x22 => {
                // cparams (7 bits) and fPrompt (bit 7): the dialog-box form of a command.
                let cparams = t.u8()?;
                let n = usize::from(cparams & 0x7F);
                let tab = t.u16()?;
                if tab & 0x8000 != 0 {
                    // fCeFunc: the low 15 bits are a Cetab value, a macro sheet command.
                    if !site.commands && mode != Mode::Name {
                        return Err(Unsupported("macro sheet commands"));
                    }
                    let name = cetab(tab & 0x7FFF).ok_or(Unsupported("macro sheet commands"))?;
                    let args = pop_n(&mut stack, n, &mut kids)?;
                    let name = if cparams & 0x80 != 0 { format!("{name}?") } else { name.to_string() };
                    push(&mut stack, kids, Expr::Call(name, args))?;
                    continue;
                }
                let mut args = pop_n(&mut stack, n, &mut kids)?;
                if tab & 0x7FFF == 0x00FF {
                    // A function called by name (add-in or newer function): the name comes first.
                    if args.is_empty() {
                        return Err(Unsupported("malformed function call"));
                    }
                    let name = match args.remove(0) {
                        Expr::Name(n) => function_name(&n),
                        _ => return Err(Unsupported("function call through an expression")),
                    };
                    push(&mut stack, kids, Expr::Call(name, args))?;
                } else {
                    let (name, _) = function(tab & 0x7FFF).ok_or(Unsupported("unknown function"))?;
                    push(&mut stack, kids, Expr::Call(name.to_string(), args))?;
                }
            }
            0x23 => {
                let index = if biff5.is_some() {
                    // BIFF5: a 16-bit index and 12 reserved bytes.
                    let i = t.u16()?;
                    t.take(12)?;
                    u32::from(i)
                } else {
                    t.u32()?
                };
                let i = usize::try_from(index).ok().and_then(|i| i.checked_sub(1));
                let name = i.and_then(|i| links.names.get(i)).filter(|n| !n.is_empty()).ok_or(Unsupported("reference to a missing defined name"))?;
                // A cell using a name the import drops would calculate to #NAME?: keep its
                // saved result instead, as for references to other workbooks.
                if !mode.is_name() && i.and_then(|i| links.skipped.get(i)) == Some(&true) {
                    return Err(Unsupported("defined names that were skipped"));
                }
                push(&mut stack, kids, Expr::Name(name.clone()))?;
            }
            0x24 | 0x2C => {
                let (r, c) = grid.loc(&mut t)?;
                let a = anchor(r, c, base, id == 0x2C, grid)?;
                push(&mut stack, kids, Expr::Ref(Reference { sheet: SheetSel::Current, kind: RefKind::Cell(a) }))?;
            }
            0x25 | 0x2D => {
                let (r0, r1, c0, c1) = grid.area(&mut t)?;
                let kind = area(r0, r1, c0, c1, base, id == 0x2D, grid)?;
                push(&mut stack, kids, Expr::Ref(Reference { sheet: SheetSel::Current, kind }))?;
            }
            0x26 => {
                // PtgMemArea: a cache of the sub-expression that follows; skip its extra data.
                t.take(6)?;
                let n = if wide { usize::try_from(extra.u32()?).map_err(|_| TRUNC)? } else { usize::from(extra.u16()?) };
                extra.take(n.checked_mul(if wide { 16 } else { 8 }).ok_or(TRUNC)?)?;
            }
            0x27 | 0x28 => {
                t.take(6)?;
            }
            0x29 => {
                t.take(2)?;
            }
            0x2A => {
                t.take(grid.loc_len())?;
                push(&mut stack, kids, Expr::Error(CellError::Ref))?;
            }
            0x2B => {
                t.take(grid.loc_len() * 2)?;
                push(&mut stack, kids, Expr::Error(CellError::Ref))?;
            }
            0x39 if biff5.is_some() => {
                // BIFF5 PtgNameX: ixals, 8 reserved bytes, the name's index, 12 reserved bytes.
                let ixals = t.u16()? as i16;
                t.take(8)?;
                let i = t.u16()?;
                t.take(12)?;
                let name = biff5.ok_or(Unsupported("malformed formula"))?.extern_name(ixals, i)?;
                push(&mut stack, kids, Expr::Name(name))?;
            }
            0x3A | 0x3B if biff5.is_some() => {
                let b5 = biff5.ok_or(Unsupported("malformed formula"))?;
                let ixals = t.u16()? as i16;
                t.take(8)?;
                let (first, last) = (t.u16()? as i16, t.u16()? as i16);
                let kind = if id == 0x3A {
                    let (r, c) = grid.loc(&mut t)?;
                    RefKind::Cell(anchor(r, c, base, mode.is_name(), grid)?)
                } else {
                    let (r0, r1, c0, c1) = grid.area(&mut t)?;
                    area(r0, r1, c0, c1, base, mode.is_name(), grid)?
                };
                push(
                    &mut stack,
                    kids,
                    match b5.sheet_sel(links, ixals, first, last)? {
                        Some(sheet) => Expr::Ref(Reference { sheet, kind }),
                        None => Expr::Error(CellError::Ref),
                    },
                )?;
            }
            0x39 => {
                let ixti = t.u16()?;
                let i = t.u32()?;
                push(&mut stack, kids, Expr::Name(links.extern_name(ixti, i)?))?;
            }
            0x3A => {
                let ixti = t.u16()?;
                let (r, c) = grid.loc(&mut t)?;
                let a = anchor(r, c, base, mode.is_name(), grid)?;
                push(
                    &mut stack,
                    kids,
                    match links.sheet_sel(ixti, site.external)? {
                        Some(sheet) => Expr::Ref(Reference { sheet, kind: RefKind::Cell(a) }),
                        None => Expr::Error(CellError::Ref),
                    },
                )?;
            }
            0x3B => {
                let ixti = t.u16()?;
                let (r0, r1, c0, c1) = grid.area(&mut t)?;
                let kind = area(r0, r1, c0, c1, base, mode.is_name(), grid)?;
                push(
                    &mut stack,
                    kids,
                    match links.sheet_sel(ixti, site.external)? {
                        Some(sheet) => Expr::Ref(Reference { sheet, kind }),
                        None => Expr::Error(CellError::Ref),
                    },
                )?;
            }
            0x3C => {
                t.take(if biff5.is_some() { BIFF5_3D_HEADER } else { 2 } + grid.loc_len())?;
                push(&mut stack, kids, Expr::Error(CellError::Ref))?;
            }
            0x3D => {
                t.take(if biff5.is_some() { BIFF5_3D_HEADER } else { 2 } + grid.loc_len() * 2)?;
                push(&mut stack, kids, Expr::Error(CellError::Ref))?;
            }
            0x01 => return Err(Unsupported("nested shared or array formula reference")),
            0x02 => return Err(Unsupported("what-if data tables")),
            0x18 => return Err(Unsupported("natural-language formulas")),
            _ => return Err(Unsupported("unknown formula token")),
        }
        if stack.len() > MAX_RGCE {
            return Err(Unsupported("formula too complex"));
        }
    }
    let (e, _) = stack.pop().ok_or(Unsupported("malformed formula (operand missing)"))?;
    if !stack.is_empty() {
        return Err(Unsupported("malformed formula (unused operands)"));
    }
    Ok(e)
}

/// PtgExtraArray ([MS-XLS] 2.5.198.59): an array constant's values in row-major order.
/// BIFF5 text values (`biff5`) are refused: their layout isn't read.
fn array_constant(b: &mut Bytes<'_>, biff5: bool) -> Result<Expr, Unsupported> {
    let cols = usize::from(b.u8()?) + 1;
    let rows = usize::from(b.u16()?) + 1;
    if cols.saturating_mul(rows) > MAX_ARRAY {
        return Err(Unsupported("array constant too large"));
    }
    let mut out = Vec::with_capacity(rows);
    for _ in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for _ in 0..cols {
            let kind = b.u8()?;
            row.push(match kind {
                0x00 => {
                    b.take(8)?;
                    Expr::Missing
                }
                0x01 => {
                    let n = b.f64()?;
                    if !n.is_finite() {
                        return Err(Unsupported("invalid number constant"));
                    }
                    Expr::Number(n)
                }
                0x02 if biff5 => return Err(Unsupported("text in Excel 5.0/95 array constants")),
                0x02 => {
                    let cch = usize::from(b.u16()?);
                    let high = b.u8()? & 1 != 0;
                    Expr::Text(Arc::from(b.chars(cch, high)?))
                }
                0x04 => {
                    let v = b.u8()?;
                    b.take(7)?;
                    Expr::Bool(v != 0)
                }
                0x10 => {
                    let e = b.u8()?;
                    b.take(7)?;
                    Expr::Error(error_code(e).ok_or(Unsupported("unknown error constant"))?)
                }
                _ => return Err(Unsupported("unknown array constant value")),
            });
        }
        out.push(row);
    }
    Ok(Expr::Array(out))
}

/// BIFF12 PtgExtraArray and SerAr ([MS-XLSB] 2.5.98.41, 2.5.98.93–97).
fn array_constant_xlsb(b: &mut Bytes<'_>) -> Result<Expr, Unsupported> {
    let rows = usize::try_from(b.u32()?).map_err(|_| TRUNC)?;
    let cols = usize::try_from(b.u32()?).map_err(|_| TRUNC)?;
    if rows == 0 || cols == 0 || rows > 1_048_576 || cols > 16_384 || rows.saturating_mul(cols) > MAX_ARRAY {
        return Err(Unsupported("invalid array constant dimensions"));
    }
    let mut out = Vec::with_capacity(rows);
    for _ in 0..rows {
        let mut row = Vec::with_capacity(cols);
        for _ in 0..cols {
            row.push(match b.u8()? {
                0x00 => {
                    let n = b.f64()?;
                    if !n.is_finite() {
                        return Err(Unsupported("invalid number constant"));
                    }
                    Expr::Number(n)
                }
                0x01 => {
                    let cch = usize::from(b.u16()?);
                    if cch >= 256 {
                        return Err(Unsupported("array string too long"));
                    }
                    Expr::Text(Arc::from(b.chars(cch, true)?))
                }
                0x02 => match b.u8()? {
                    0 => Expr::Bool(false),
                    1 => Expr::Bool(true),
                    _ => return Err(Unsupported("invalid array boolean")),
                },
                0x04 => {
                    let e = b.u8()?;
                    b.take(3)?;
                    Expr::Error(cell_error(e))
                }
                _ => return Err(Unsupported("unknown array constant value")),
            });
        }
        out.push(row);
    }
    Ok(Expr::Array(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn links() -> Links {
        Links {
            sheets: vec!["Sheet1".into(), "My Data".into()],
            names: vec!["Rate".into()],
            skipped: vec![false],
            books: vec![
                SupBook { internal: true, ..SupBook::default() },
                SupBook { addin: true, names: vec!["_xlfn.IFERROR".into()], ..SupBook::default() },
                SupBook::default(),
            ],
            xti: vec![
                Xti { book: 0, first: 0, last: 1 },
                Xti { book: 1, first: -2, last: -2 },
                Xti { book: 2, first: 0, last: 0 },
                Xti { book: 0, first: -1, last: -1 },
            ],
            lists: vec![],
            pivot_fields: vec![],
        }
    }

    fn text(rgce: &[u8], rgcb: &[u8]) -> Result<String, Unsupported> {
        decode(rgce, rgcb, CellRef::new(0, 0), Mode::Cell, &links()).map(|e| gridcraft_formula::print(&e))
    }

    #[test]
    fn only_xls_names_replace_offset_tokens_with_na() {
        for ptg in [0x2C, 0x4C, 0x6C, 0x2D, 0x4D, 0x6D] {
            let area = ptg & 0x1F == 0x0D;
            let mut biff8 = vec![ptg];
            biff8.resize(if area { 9 } else { 5 }, 0);
            let l = Links::default();
            let base = CellRef::new(0, 0);
            let name = decode_version(&biff8, &[], base, Mode::XlsName, &l, None).unwrap();
            assert_eq!(name, Expr::Error(CellError::NA));
            for mode in [Mode::Cell, Mode::Shared, Mode::Name] {
                assert!(matches!(decode(&biff8, &[], base, mode, &l).unwrap(), Expr::Ref(_)));
            }
            let mut biff12 = vec![ptg];
            biff12.resize(if area { 13 } else { 7 }, 0);
            assert!(matches!(decode_xlsb(&biff12, &[], base, Mode::Name, &l).unwrap(), Expr::Ref(_)));
        }
        // Token-looking bytes inside a constant are data, not relative references.
        let number = decode_version(&[0x1E, 0x2C, 0x6D], &[], CellRef::new(0, 0), Mode::XlsName, &Links::default(), None).unwrap();
        assert_eq!(number, Expr::Number(f64::from(0x6D2C)));
    }

    #[test]
    fn decodes_common_tokens() {
        // {1,"a";TRUE,#N/A} via PtgArray + PtgExtraArray.
        let extra =
            [&[1u8, 1, 0][..], &[1], &1f64.to_le_bytes(), &[2, 1, 0, 0, b'a'], &[4, 1, 0, 0, 0, 0, 0, 0, 0], &[0x10, 0x2A, 0, 0, 0, 0, 0, 0, 0]]
                .concat();
        assert_eq!(text(&[0x60, 0, 0, 0, 0, 0, 0, 0], &extra).unwrap(), "{1,\"a\";TRUE,#N/A}");
        // SUM via PtgAttrSum over a 3-D span, whole column.
        let area3d = [&[0x3Bu8][..], &0u16.to_le_bytes(), &[0, 0, 0xFF, 0xFF, 1, 0, 1, 0]].concat();
        assert_eq!(text(&[area3d.as_slice(), &[0x19, 0x10, 0, 0]].concat(), &[]).unwrap(), "SUM('Sheet1:My Data'!$B:$B)");
        // IFERROR (a newer function) called by name through PtgNameX, with a missing argument.
        let namex = [&[0x39u8][..], &1u16.to_le_bytes(), &1u32.to_le_bytes()].concat();
        let call = [namex.as_slice(), &[0x1E, 5, 0, 0x16, 0x22, 3, 0xFF, 0]].concat();
        assert_eq!(text(&call, &[]).unwrap(), "IFERROR(5,)");
        // A deleted sheet becomes #REF!; the CHOOSE jump table is skipped.
        let ref_deleted = [&[0x3Au8][..], &3u16.to_le_bytes(), &[0, 0, 0, 0]].concat();
        let choose = [&[0x1Eu8, 1, 0][..], &[0x19, 0x04, 1, 0, 0, 0, 0, 0], ref_deleted.as_slice(), &[0x1E, 2, 0, 0x22, 3, 100, 0]].concat();
        assert_eq!(text(&choose, &[]).unwrap(), "CHOOSE(1,#REF!,2)");
        // External links, data tables and unbalanced stacks are refused, never guessed.
        let external = [&[0x3Au8][..], &2u16.to_le_bytes(), &[0, 0, 0, 0]].concat();
        assert!(text(&external, &[]).is_err());
        assert!(text(&[0x02, 0, 0, 0, 0], &[]).is_err());
        assert!(text(&[0x1E, 1, 0, 0x1E, 2, 0], &[]).is_err());
        assert!(text(&[0x03], &[]).is_err());
        assert!(text(&[0x21, 0xFF, 0x7F], &[]).is_err());
        assert!(text(&[0x60, 0, 0, 0, 0, 0, 0, 0], &[0xFF, 0xFF, 0xFF]).is_err());
    }

    fn xlsb(rgce: &[u8], base: CellRef) -> Result<String, Unsupported> {
        decode_xlsb(rgce, &[], base, Mode::Cell, &Links::default()).map(|e| gridcraft_formula::print(&e))
    }

    fn xlsb_linked(rgce: &[u8], mode: Mode) -> Result<String, Unsupported> {
        decode_xlsb(rgce, &[], CellRef::new(0, 0), mode, &links()).map(|e| gridcraft_formula::print(&e))
    }

    #[test]
    fn decodes_xlsb_names_and_other_sheets() {
        let loc = |row: u32, col: u16| [&row.to_le_bytes()[..], &col.to_le_bytes()].concat();
        // PtgRef3d: ixti u16 + RgceLoc (u32 row, u16 column) = 8 bytes.
        let r = [&[0x3Au8][..], &2u16.to_le_bytes(), &loc(1, 1)].concat();
        assert_eq!(r.len(), 9);
        let mut l = links();
        l.xti[2] = Xti { book: 0, first: 1, last: 1 };
        let one = |rgce: &[u8]| decode_xlsb(rgce, &[], CellRef::new(0, 0), Mode::Cell, &l).map(|e| gridcraft_formula::print(&e));
        assert_eq!(one(&r).unwrap(), "'My Data'!$B$2");
        // PtgArea3d: ixti u16 + RgceArea (two u32 rows, two u16 columns) = 14 bytes; a 3-D span.
        let a = [&[0x3Bu8][..], &0u16.to_le_bytes(), &0u32.to_le_bytes(), &9u32.to_le_bytes(), &0xC000u16.to_le_bytes(), &0xC000u16.to_le_bytes()]
            .concat();
        assert_eq!(a.len(), 15);
        assert_eq!(one(&a).unwrap(), "'Sheet1:My Data'!A1:A10");
        // A row only BIFF12 can address.
        let far = [&[0x3Au8][..], &2u16.to_le_bytes(), &loc(0xF_FFFF, 0x3FFF)].concat();
        assert_eq!(one(&far).unwrap(), "'My Data'!$XFD$1048576");
        // PtgName: a u32 one-based index into the defined names = 5 bytes.
        assert_eq!(xlsb_linked(&[0x43, 1, 0, 0, 0, 0x1E, 2, 0, 0x05], Mode::Cell).unwrap(), "Rate*2");
        // PtgRefErr3d (2 + 6 bytes) and PtgAreaErr3d (2 + 12 bytes) become #REF!.
        let err = [&[0x3Cu8][..], &[0; 8], &[0x3D], &[0; 14], &[0x10]].concat();
        assert_eq!(xlsb_linked(&err, Mode::Cell).unwrap(), "#REF!,#REF!");
        // Name formulas: a relative 3-D reference is an offset; a deleted sheet is #REF!.
        let rel = [&[0x3Au8][..], &0u16.to_le_bytes(), &loc(2, 0xC001)].concat();
        assert_eq!(xlsb_linked(&rel, Mode::Name).unwrap(), "'Sheet1:My Data'!B3");
        let gone = [&[0x3Au8][..], &3u16.to_le_bytes(), &loc(0, 0)].concat();
        assert_eq!(xlsb_linked(&gone, Mode::Cell).unwrap(), "#REF!");
        // External books, a missing XTI, name index 0 or past the end, truncated payloads.
        let external = [&[0x3Au8][..], &2u16.to_le_bytes(), &loc(0, 0)].concat();
        assert!(xlsb_linked(&external, Mode::Cell).is_err());
        let missing = [&[0x3Au8][..], &9u16.to_le_bytes(), &loc(0, 0)].concat();
        assert!(xlsb_linked(&missing, Mode::Cell).is_err());
        assert!(xlsb_linked(&[0x23, 0, 0, 0, 0], Mode::Cell).is_err());
        assert!(xlsb_linked(&[0x23, 2, 0, 0, 0], Mode::Cell).is_err());
        assert_eq!(xlsb_linked(&r[..8], Mode::Cell), Err(TRUNC));
        assert_eq!(xlsb_linked(&a[..14], Mode::Cell), Err(TRUNC));
        // PtgNameX stays refused in XLSB.
        assert!(xlsb_linked(&[0x39, 1, 0, 1, 0, 0, 0], Mode::Cell).is_err());
    }

    #[test]
    fn decodes_xlsb_wide_coordinates() {
        let loc = |row: u32, col: u16| [&row.to_le_bytes()[..], &col.to_le_bytes()].concat();
        // XFD1048576 (absolute), beyond the BIFF8 grid.
        let r = [&[0x24u8][..], &loc(0xF_FFFF, 0x3FFF)].concat();
        assert_eq!(xlsb(&r, CellRef::new(0, 0)).unwrap(), "$XFD$1048576");
        // Relative AA70000 in a value-class PtgRef, with PtgAttrSpace before PtgUminus.
        let r = [&[0x44u8][..], &loc(69_999, 26 | 0xC000), &[0x19, 0x40, 0, 1, 0x13]].concat();
        assert_eq!(xlsb(&r, CellRef::new(5, 5)).unwrap(), "-AA70000");
        // Whole columns and whole rows use the XLSB grid's extent.
        let cols =
            [&[0x25u8][..], &0u32.to_le_bytes(), &0xF_FFFFu32.to_le_bytes(), &(2u16 | 0xC000).to_le_bytes(), &(3u16 | 0xC000).to_le_bytes()].concat();
        assert_eq!(xlsb(&[cols.as_slice(), &[0x19, 0x10, 0, 0]].concat(), CellRef::new(0, 0)).unwrap(), "SUM(C:D)");
        let rows = [&[0x65u8][..], &4u32.to_le_bytes(), &4u32.to_le_bytes(), &0u16.to_le_bytes(), &0x3FFFu16.to_le_bytes()].concat();
        assert_eq!(xlsb(&[rows.as_slice(), &[0x42, 1, 4, 0]].concat(), CellRef::new(0, 0)).unwrap(), "SUM($5:$5)");
        // PtgRefErr / PtgAreaErr carry 6 / 12 unused bytes.
        let err = [&[0x2Au8][..], &[0; 6], &[0x2B], &[0; 12], &[0x10]].concat();
        assert_eq!(xlsb(&err, CellRef::new(0, 0)).unwrap(), "#REF!,#REF!");
    }

    #[test]
    fn decodes_xlsb_relative_ptg_ref_n_and_area_n() {
        // RgceLocRel / RgceAreaRel: a signed 32-bit row offset and a 14-bit column offset with
        // the relative flags, resolved against the formula's cell and wrapped into the grid.
        let rel = |row: i32, col: i16| [&row.to_le_bytes()[..], &((col as u16 & 0x3FFF) | 0xC000).to_le_bytes()].concat();
        // Reference, value and array classes: one row up, one column left of B2 is A1.
        for ptg in [0x2Cu8, 0x4C, 0x6C] {
            assert_eq!(xlsb(&[&[ptg][..], &rel(-1, -1)].concat(), CellRef::new(1, 1)).unwrap(), "A1");
        }
        // Negative offsets past the edge wrap like Excel: one row above row 1 is the last row.
        assert_eq!(xlsb(&[&[0x2Cu8][..], &rel(-1, -1)].concat(), CellRef::new(0, 0)).unwrap(), "XFD1048576");
        // Mixed: absolute row 5 ($ kept), relative column +2.
        let mixed = [&[0x2Cu8][..], &4u32.to_le_bytes(), &(2u16 | 0x4000).to_le_bytes()].concat();
        assert_eq!(xlsb(&mixed, CellRef::new(0, 3)).unwrap(), "F$5");
        // PtgAreaN: rows 0..+1 and the column one left of C3: B3:B4.
        let area = [&[0x2Du8][..], &0i32.to_le_bytes(), &1i32.to_le_bytes(), &rel(0, -1)[4..], &rel(0, -1)[4..]].concat();
        assert_eq!(xlsb(&[area.as_slice(), &[0x19, 0x10, 0, 0]].concat(), CellRef::new(2, 2)).unwrap(), "SUM(B3:B4)");
        assert_eq!(xlsb(&[0x2D, 0, 0, 0], CellRef::new(0, 0)), Err(TRUNC));
    }

    #[test]
    fn xlsb_refuses_what_it_cannot_resolve() {
        let base = CellRef::new(0, 0);
        // A row past the sheet, a truncated RgceLoc, names, 3-D refs, shared formulas, strings.
        assert!(xlsb(&[0x24, 0, 0, 0x10, 0, 0, 0], base).is_err());
        assert_eq!(xlsb(&[0x24, 0, 0, 0, 0], base), Err(TRUNC));
        assert!(xlsb(&[0x23, 1, 0, 0, 0], base).is_err());
        assert!(xlsb(&[0x3A, 0, 0, 0, 0, 0, 0, 0, 0], base).is_err());
        assert!(xlsb(&[0x01, 0, 0, 0, 0, 0, 0], base).is_err());
        // A truncated PtgStr, and one over Excel's 255-character limit.
        assert!(xlsb(&[0x17, 2, 0, b'a', 0], base).is_err());
        assert!(xlsb(&[&[0x17u8, 0, 1][..], &[b'a', 0].repeat(256)].concat(), base).is_err());
        assert!(xlsb(&[0x60, 0, 0, 0, 0, 0, 0, 0], base).is_err());
    }

    #[test]
    fn decodes_xlsb_text_constants() {
        // BIFF12 PtgStr: u16 character count, then UTF-16LE, no high-byte flag.
        let base = CellRef::new(0, 0);
        assert_eq!(xlsb(&[0x17, 1, 0, b'a', 0], base).unwrap(), "\"a\"");
        assert_eq!(xlsb(&[0x17, 0, 0], base).unwrap(), "\"\"");
        let euro = [&[0x17u8, 2, 0][..], &0x20ACu16.to_le_bytes(), &[b'5', 0]].concat();
        assert_eq!(xlsb(&euro, base).unwrap(), "\"\u{20AC}5\"");
    }

    #[test]
    fn xlsb_text_constant_utf16_and_length_boundaries() {
        let base = CellRef::new(0, 0);
        // A surrogate pair is one character but consumes two UTF-16 code units.
        assert_eq!(xlsb(&[0x17, 2, 0, 0x3D, 0xD8, 0, 0xDE], base).unwrap(), "\"😀\"");
        for unit in [0xD800u16, 0xDC00] {
            let rgce = [&[0x17, 1, 0][..], &unit.to_le_bytes()].concat();
            assert_eq!(xlsb(&rgce, base).unwrap(), "\"\u{FFFD}\"");
        }
        let rgce = [&[0x17, 255, 0][..], &[b'x', 0].repeat(255)].concat();
        assert_eq!(xlsb(&rgce, base).unwrap(), format!("\"{}\"", "x".repeat(255)));
        assert!(xlsb(&[0x17, 1, 0, b'x'], base).is_err());
        assert!(xlsb(&[0x17, 1], base).is_err());
    }

    #[test]
    fn decodes_xlsb_array_constant_value_types() {
        let text = |units: &[u16]| {
            let units = units.iter().flat_map(|u| u.to_le_bytes()).collect::<Vec<_>>();
            [&[1][..], &((units.len() / 2) as u16).to_le_bytes(), &units].concat()
        };
        let values =
            [[&[0][..], &(-2.5f64).to_le_bytes()].concat(), text(&[0xD83D, 0xDE00]), text(&[]), vec![2, 0], vec![2, 1], vec![4, 0x2A, 0, 0, 0]];
        let extra = [&3u32.to_le_bytes()[..], &2u32.to_le_bytes(), &values.concat()].concat();
        let decode = |tokens: &[u8], extra: &[u8]| {
            decode_xlsb(tokens, extra, CellRef::new(0, 0), Mode::Cell, &Links::default()).map(|e| gridcraft_formula::print(&e))
        };
        assert_eq!(decode(&[&[0x60][..], &[0; 14]].concat(), &extra).unwrap(), "{-2.5,\"😀\";\"\",FALSE;TRUE,#N/A}");
        let long = [&[1][..], &256u16.to_le_bytes(), &[b'x', 0].repeat(256)].concat();
        let long = [&1u32.to_le_bytes()[..], &1u32.to_le_bytes(), &long].concat();
        assert!(decode(&[&[0x60][..], &[0; 14]].concat(), &long).is_err());
    }

    #[test]
    fn xlsb_arrays_and_memory_extras_are_consumed_in_token_order() {
        let array = [&1u32.to_le_bytes()[..], &2u32.to_le_bytes(), &[0], &1.0f64.to_le_bytes(), &[2, 1]].concat();
        let mem = [&1u32.to_le_bytes()[..], &[0; 16]].concat();
        for class in [0x20, 0x40, 0x60] {
            let tokens = [&[class | 6][..], &[0; 6], &[class], &[0; 14]].concat();
            let extra = [&mem[..], &array].concat();
            let decode = |t: &[u8], e: &[u8]| decode_xlsb(t, e, CellRef::new(0, 0), Mode::Cell, &Links::default());
            assert_eq!(gridcraft_formula::print(&decode(&tokens, &extra).unwrap()), "{1,TRUE}");
            for len in 0..extra.len() {
                assert!(decode(&tokens, &extra[..len]).is_err(), "extra length {len}");
            }
            for len in 0..tokens.len() {
                assert!(decode(&tokens[..len], &extra).is_err(), "token length {len}");
            }
        }
        let mut tokens = vec![0x60];
        tokens.extend([0; 14]);
        tokens.push(0x60);
        tokens.extend([0; 14]);
        tokens.push(0x03);
        let extras = [&array[..], &array].concat();
        let expr = decode_xlsb(&tokens, &extras, CellRef::new(0, 0), Mode::Cell, &Links::default()).unwrap();
        assert_eq!(gridcraft_formula::print(&expr), "{1,TRUE}+{1,TRUE}");

        for ptg in [0x26, 0x46, 0x66] {
            let tokens = [&[ptg][..], &[0; 6], &[0x1E, 1, 0]].concat();
            let decode = |extra: &[u8]| decode_xlsb(&tokens, extra, CellRef::new(0, 0), Mode::Cell, &Links::default());
            for extra in [vec![], 1u32.to_le_bytes().to_vec(), [&1u32.to_le_bytes()[..], &[0; 15]].concat(), u32::MAX.to_le_bytes().to_vec()] {
                assert!(decode(&extra).is_err());
            }
        }
    }

    #[test]
    fn deep_nesting_is_refused_not_a_stack_overflow() {
        // 1 followed by thousands of PtgParen / PtgUminus tokens: a hostile file used to build a
        // 16K-deep tree that overflowed the stack when printed or dropped.
        for op in [0x15u8, 0x13] {
            let mut rgce = vec![0x1E, 1, 0];
            rgce.resize(MAX_RGCE, op);
            assert_eq!(text(&rgce, &[]), Err(Unsupported("formula nested too deeply")));
        }
        // Binary chains nest too: 1+1+1+... left-associated.
        let mut chain = vec![0x1E, 1, 0];
        while chain.len() + 4 <= MAX_RGCE {
            chain.extend([0x1E, 1, 0, 0x03]);
        }
        assert!(text(&chain, &[]).is_err());
        // Nesting up to the limit still decodes.
        let mut ok = vec![0x1E, 1, 0];
        ok.resize(3 + MAX_DEPTH - 1, 0x15);
        assert!(text(&ok, &[]).is_ok());
    }
}

#[cfg(test)]
#[path = "formula_command_tests.rs"]
mod command_tests;
