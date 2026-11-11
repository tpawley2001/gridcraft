//! OPC package access: reading zip parts with decompression limits, relationships and part-name
//! resolution; writing a zip package deterministically.

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use crate::IoError;
use crate::xml::{self, El};

/// Largest decompressed part accepted.
pub const MAX_PART: u64 = 512 * 1024 * 1024;
/// Largest total decompressed size read from one package.
pub const MAX_TOTAL: u64 = 1024 * 1024 * 1024;
/// Most entries accepted in the zip directory.
pub const MAX_ENTRIES: usize = 200_000;

pub struct Package<'a> {
    source: Source<'a>,
    /// Lower-case normalized part name → zip index (or index into the recovered parts).
    names: HashMap<String, usize>,
    total: u64,
    /// Parts kept although their ZIP CRC-32 didn't match their data, in read order.
    checksum_failures: Vec<String>,
    /// The error of the failed read that ended the budget, when that (not an oversized part) is
    /// why `total` is exhausted: later reads report it instead of a misleading package size.
    failed: Option<String>,
}

enum Source<'a> {
    Zip(zip::ZipArchive<Cursor<&'a [u8]>>),
    /// Parts read from the local file headers of a package that lost its central directory.
    Recovered(Vec<Vec<u8>>),
}

/// A relationship.
#[derive(Clone, Debug)]
pub struct Rel {
    pub id: String,
    /// Last path segment of the relationship type (`worksheet`, `styles`, …): the same for the
    /// transitional and strict namespaces.
    pub kind: String,
    /// Resolved part name (no leading `/`), or the raw target for external relationships.
    pub target: String,
    pub external: bool,
}

pub fn normalize(name: &str) -> String {
    name.replace('\\', "/").trim_start_matches('/').to_ascii_lowercase()
}

impl<'a> Package<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Package<'a>, IoError> {
        let zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| IoError::Zip(e.to_string()))?;
        if zip.len() > MAX_ENTRIES {
            return Err(IoError::TooLarge(format!("{} zip entries", zip.len())));
        }
        let mut names = HashMap::new();
        for i in 0..zip.len() {
            if let Some(n) = zip.name_for_index(i) {
                names.entry(normalize(n)).or_insert(i);
            }
        }
        Ok(Package { source: Source::Zip(zip), names, total: 0, checksum_failures: Vec::new(), failed: None })
    }

    /// [`Package::open`], and when the zip has no readable central directory (a truncated
    /// download), the parts that can be read in order from their local file headers (APPNOTE
    /// 4.3.7), as Excel's repair does. Recovery stops at the first incomplete or damaged entry
    /// and keeps the complete ones before it. It is used only when the content types and the
    /// workbook part survived; otherwise the original error is returned.
    pub fn open_repairing(bytes: &'a [u8]) -> Result<Package<'a>, IoError> {
        match Package::open(bytes) {
            Err(IoError::Zip(e)) => Package::recover(bytes).ok_or(IoError::Zip(e)),
            other => other,
        }
    }

    /// Parts whose data was kept although their ZIP CRC-32 didn't match, in read order.
    pub fn checksum_failures(&self) -> &[String] {
        &self.checksum_failures
    }

    /// How many parts were recovered from local headers, when the package was repaired.
    pub fn recovered_parts(&self) -> Option<usize> {
        match &self.source {
            Source::Zip(_) => None,
            Source::Recovered(parts) => Some(parts.len()),
        }
    }

    fn recover(bytes: &'a [u8]) -> Option<Package<'a>> {
        if !bytes.starts_with(b"PK\x03\x04") {
            return None;
        }
        let mut cursor = Cursor::new(bytes);
        let mut parts = Vec::new();
        let mut names = HashMap::new();
        let mut total = 0u64;
        while parts.len() < MAX_ENTRIES {
            let Some(entry) = recover_entry(&mut cursor, MAX_TOTAL.saturating_sub(total)) else { break };
            if let Some((name, data)) = entry {
                total = total.saturating_add(data.len() as u64);
                names.entry(normalize(&name)).or_insert(parts.len());
                parts.push(data);
            }
        }
        let mut pkg = Package { source: Source::Recovered(parts), names, total: 0, checksum_failures: Vec::new(), failed: None };
        // The essential parts: the content types and the workbook part the package names.
        let workbook = pkg.rels("").ok()?.into_iter().find(|r| r.kind == "officeDocument" && !r.external).map(|r| r.target);
        let workbook = workbook.unwrap_or_else(|| "xl/workbook.xml".into());
        (pkg.has("[Content_Types].xml") && pkg.has(&workbook)).then(|| {
            pkg.total = 0;
            pkg
        })
    }

    /// Every part's name as the package spells it (the first of names that differ only in case),
    /// sorted. Lookups ([`Package::read`]) ignore case either way.
    #[cfg(test)]
    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .names
            .iter()
            .map(|(key, &i)| match &self.source {
                Source::Zip(zip) => zip.name_for_index(i).map_or_else(|| key.clone(), |n| n.trim_start_matches('/').to_string()),
                // Recovered parts keep only their normalized names.
                Source::Recovered(_) => key.clone(),
            })
            .collect();
        v.sort();
        v
    }

    pub fn has(&self, part: &str) -> bool {
        self.names.contains_key(&normalize(part))
    }

    /// Reads a part's bytes; `Ok(None)` when it doesn't exist.
    pub fn read(&mut self, part: &str) -> Result<Option<Vec<u8>>, IoError> {
        self.read_limited(part, MAX_PART)
    }

    /// Reads a part the reader can do without (the caller warns and goes on when it fails). A read
    /// that fails partway is charged this part's whole allowance (`maximum`, and the byte that
    /// detects an overrun) instead of ending the package's: the decompression work stays bounded,
    /// and the parts read after it still open.
    pub fn read_optional_limited(&mut self, part: &str, maximum: u64) -> Result<Option<Vec<u8>>, IoError> {
        let before = self.total;
        let read = self.read_limited(part, maximum);
        if let Err(IoError::Zip(_)) = &read {
            self.total = before.saturating_add(maximum.min(MAX_PART).saturating_add(1)).min(MAX_TOTAL);
            if before < MAX_TOTAL {
                // The budget ends at this part's allowance, not at the failed read.
                self.failed = None;
            }
        }
        read
    }

    /// Reads a small metadata part without allowing it to expand to the general part limit.
    pub fn read_limited(&mut self, part: &str, maximum: u64) -> Result<Option<Vec<u8>>, IoError> {
        let maximum = maximum.min(MAX_PART);
        let Some(&idx) = self.names.get(&normalize(part)) else {
            return Ok(None);
        };
        let zip = match &mut self.source {
            Source::Zip(zip) => zip,
            Source::Recovered(parts) => {
                let data = parts.get(idx).ok_or_else(|| IoError::Zip(format!("{part}: missing")))?;
                let size = data.len() as u64;
                if size > maximum {
                    return Err(IoError::TooLarge(format!("{part} expands to {size} bytes")));
                }
                if size > MAX_TOTAL.saturating_sub(self.total) {
                    self.total = MAX_TOTAL;
                    return Err(IoError::TooLarge("package expands to more than 1 GB".into()));
                }
                self.total += size;
                return Ok(Some(data.clone()));
            }
        };
        if self.total >= MAX_TOTAL
            && let Some(failed) = &self.failed
        {
            // Nothing is decompressed: the budget went to a part whose data is broken.
            return Err(IoError::Zip(format!("{part}: not read after a damaged part ({failed})")));
        }
        let mut f = zip.by_index(idx).map_err(|e| IoError::Zip(format!("{part}: {e}")))?;
        if f.size() > maximum {
            return Err(IoError::TooLarge(format!("{part} expands to {} bytes", f.size())));
        }
        let remaining = MAX_TOTAL.saturating_sub(self.total);
        let limit = maximum.min(remaining);
        let mut out = Vec::with_capacity(f.size().min(16 * 1024 * 1024) as usize);
        let read = (&mut f).take(limit + 1).read_to_end(&mut out);
        // The zip reader checks the CRC-32 only once the whole stated size has been inflated. When
        // that check is the only failure, the data is complete and its content is kept (Excel
        // opens such a part too); the caller reports the damage from `checksum_failures`.
        let read = match read {
            Err(e) if is_checksum_error(&e) && out.len() as u64 == f.size() && out.len() as u64 <= limit => {
                if !self.checksum_failures.iter().any(|p| p == part) {
                    self.checksum_failures.push(part.to_string());
                }
                Ok(out.len())
            }
            other => other,
        };
        let read = read.map_err(|e| {
            // A read that fails partway has already spent decompression work and buffered data
            // the package budget never sees: count the allowance as spent, so a loop over many
            // broken parts can't decompress it over and over.
            self.total = MAX_TOTAL;
            let message = format!("{part}: {e}");
            self.failed.get_or_insert_with(|| message.clone());
            IoError::Zip(message)
        });
        if out.len() as u64 > limit {
            // The allowance was spent getting here; make the exhaustion terminal instead of
            // letting later reads pay the same decompression cost again.
            self.total = MAX_TOTAL;
            self.failed = None;
            return Err(IoError::TooLarge(if remaining < maximum {
                "package expands to more than 1 GB".into()
            } else {
                format!("{part} expands to more than {maximum} bytes")
            }));
        }
        read?;
        self.total += out.len() as u64;
        Ok(Some(out))
    }

    pub fn read_xml(&mut self, part: &str) -> Result<Option<El>, IoError> {
        match self.read(part)? {
            Some(b) => xml::parse(&b).map(Some).map_err(|e| prefix(part, e)),
            None => Ok(None),
        }
    }

    /// Reads a legacy VML drawing with HTML-style caption line breaks.
    pub fn read_vml(&mut self, part: &str) -> Result<Option<El>, IoError> {
        match self.read(part)? {
            Some(b) => xml::parse_vml(&b).map(Some).map_err(|e| prefix(part, e)),
            None => Ok(None),
        }
    }

    /// Relationships of a part (`xl/workbook.xml` → `xl/_rels/workbook.xml.rels`). Missing or
    /// broken rels parts give an empty list.
    pub fn rels(&mut self, part: &str) -> Result<Vec<Rel>, IoError> {
        let (dir, file) = split_dir(part);
        let rels_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let root = match self.read(&rels_name)? {
            Some(b) => match xml::parse(&b) {
                Ok(r) => r,
                Err(_) => return Ok(vec![]),
            },
            None => return Ok(vec![]),
        };
        Ok(root
            .kids("Relationship")
            .filter_map(|r| {
                let id = r.attr("Id")?.to_string();
                let ty = r.attr("Type").unwrap_or("");
                let kind = ty.rsplit('/').next().unwrap_or("").to_string();
                let target_raw = r.attr("Target")?;
                let external = r.attr("TargetMode").is_some_and(|m| m.eq_ignore_ascii_case("External"));
                let target =
                    if external || (kind == "hyperlink" && target_raw.starts_with('#')) { target_raw.to_string() } else { resolve(dir, target_raw) };
                Some(Rel { id, kind, target, external })
            })
            .collect())
    }
}

/// The load warning for a package opened by [`Package::open_repairing`] from `n` recovered parts.
pub fn damaged_warning(n: usize) -> String {
    format!("The file was damaged (incomplete ZIP archive); {n} parts were recovered.")
}

/// The load warning for a part whose data was kept although its ZIP CRC-32 didn't match.
pub fn checksum_warning(part: &str) -> String {
    format!("The file was damaged: part '{part}' failed its checksum; its content was kept.")
}

/// Whether a read error is the zip reader's CRC-32 mismatch, reported after the data was read.
fn is_checksum_error(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::InvalidData && e.to_string().contains("Invalid checksum")
}

/// The next entry at the cursor: `Some(Some((name, data)))` for a complete file,
/// `Some(None)` for a directory, `None` at the central directory or at the first entry that is
/// truncated, damaged, over the size limits, or written with a data descriptor (its size isn't
/// known before its data).
fn recover_entry(cursor: &mut Cursor<&[u8]>, remaining: u64) -> Option<Option<(String, Vec<u8>)>> {
    let mut f = zip::read::read_zipfile_from_stream(cursor).ok()??;
    let size = f.size();
    if size > MAX_PART || size > remaining {
        return None;
    }
    let name = f.name().to_string();
    if f.is_dir() {
        return Some(None);
    }
    let mut out = Vec::with_capacity(size.min(16 * 1024 * 1024) as usize);
    (&mut f).take(size.saturating_add(1)).read_to_end(&mut out).ok()?;
    (out.len() as u64 == size).then_some(Some((name, out)))
}

/// The first part whose declared unpacked size is over [`MAX_PART`], with that size.
pub fn oversized_part(bytes: &[u8]) -> Option<(String, u64)> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    (0..zip.len().min(MAX_ENTRIES)).find_map(|i| {
        let f = zip.by_index_raw(i).ok()?;
        (f.size() > MAX_PART).then(|| (f.name().to_string(), f.size()))
    })
}

fn prefix(part: &str, e: IoError) -> IoError {
    match e {
        IoError::Xml(m) => IoError::Xml(format!("{part}: {m}")),
        o => o,
    }
}

/// `xl/worksheets/sheet1.xml` → (`xl/worksheets`, `sheet1.xml`).
pub fn split_dir(part: &str) -> (&str, &str) {
    let p = part.trim_start_matches('/');
    match p.rfind('/') {
        Some(i) => (p.get(..i).unwrap_or(""), p.get(i + 1..).unwrap_or("")),
        None => ("", p),
    }
}

/// Resolves a relationship target against the source part's directory.
pub fn resolve(dir: &str, target: &str) -> String {
    let target = target.replace('\\', "/");
    let target = target.split('#').next().unwrap_or("").to_string();
    let mut segs: Vec<&str> = if target.starts_with('/') { vec![] } else { dir.split('/').filter(|s| !s.is_empty()).collect() };
    for s in target.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                segs.pop();
            }
            s => segs.push(s),
        }
    }
    segs.join("/")
}

// ---------------------------------------------------------------- writing

pub struct ZipOut {
    zip: zip::ZipWriter<Cursor<Vec<u8>>>,
}

impl ZipOut {
    pub fn new() -> ZipOut {
        ZipOut { zip: zip::ZipWriter::new(Cursor::new(Vec::new())) }
    }
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), IoError> {
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .compression_level(Some(6))
            .last_modified_time(zip::DateTime::default())
            .large_file(data.len() as u64 >= u32::MAX as u64);
        self.zip.start_file(name, opts).map_err(|e| IoError::Zip(e.to_string()))?;
        self.zip.write_all(data).map_err(|e| IoError::Zip(e.to_string()))?;
        Ok(())
    }
    pub fn finish(self) -> Result<Vec<u8>, IoError> {
        self.zip.finish().map(|c| c.into_inner()).map_err(|e| IoError::Zip(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolving() {
        assert_eq!(resolve("xl", "worksheets/sheet1.xml"), "xl/worksheets/sheet1.xml");
        assert_eq!(resolve("xl/worksheets", "../drawings/d1.xml"), "xl/drawings/d1.xml");
        assert_eq!(resolve("xl/worksheets", "/xl/tables/t.xml"), "xl/tables/t.xml");
        assert_eq!(resolve("", "xl/workbook.xml"), "xl/workbook.xml");
        assert_eq!(resolve("a", "../../../x"), "x");
        assert_eq!(split_dir("xl/workbook.xml"), ("xl", "workbook.xml"));
        assert_eq!(split_dir("x.xml"), ("", "x.xml"));
    }

    /// An over-budget read must spend the package's allowance once and for all: loops over many
    /// broken parts (a sheet's optional VML relationships, for one) otherwise re-decompress the
    /// remaining allowance again and again.
    #[test]
    fn a_failed_part_ends_the_package_budget() {
        let z = crate::tests::make_zip(&[("a.xml", b"0123456789"), ("b.xml", b"hello")]);
        let mut pkg = Package::open(&z).unwrap();
        assert!(pkg.read("a.xml").unwrap().is_some());
        // Nothing left to spend within the per-part limit either.
        pkg.total = MAX_TOTAL - 1;
        assert!(pkg.read("b.xml").is_err());
        assert_eq!(pkg.total, MAX_TOTAL, "the failed read is charged");
        // Oversized repeated attempts stay cheap errors now: no allowance is left to pay for.
        assert!(matches!(pkg.read("b.xml"), Err(IoError::TooLarge(_))));
    }

    /// A part the reader can do without, damaged inside an intact archive, costs its own
    /// allowance: the parts after it are still read.
    #[test]
    fn a_damaged_optional_part_costs_only_its_allowance() {
        use std::io::Write as _;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        // A deflated part whose compressed stream is broken (a wrong CRC alone is accepted).
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("a.xml", deflated).unwrap();
        w.write_all(b"<a>damaged</a>").unwrap();
        w.start_file("b.xml", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"hello").unwrap();
        let mut z = w.finish().unwrap().into_inner();
        let data = 30 + usize::from(u16::from_le_bytes([z[26], z[27]])) + usize::from(u16::from_le_bytes([z[28], z[29]]));
        for b in &mut z[data..data + 4] {
            *b = 0xFF;
        }
        let mut pkg = Package::open(&z).unwrap();
        assert!(matches!(pkg.read_optional_limited("a.xml", 1024), Err(IoError::Zip(_))));
        assert_eq!(pkg.total, 1025, "the failed read is charged its allowance");
        assert_eq!(pkg.read("b.xml").unwrap().as_deref(), Some(&b"hello"[..]));
        // An ordinary read of it still ends the package's budget.
        assert!(pkg.read("a.xml").is_err());
        assert_eq!(pkg.total, MAX_TOTAL);
    }

    /// After a part whose compressed data is broken ends the budget, later reads name that part
    /// instead of claiming the (small) package expands past the limit.
    #[test]
    fn reads_after_a_broken_part_name_it() {
        use std::io::Write as _;
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("a.xml", deflated).unwrap();
        w.write_all(b"<a>broken</a>").unwrap();
        w.start_file("b.xml", zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(b"hello").unwrap();
        let mut z = w.finish().unwrap().into_inner();
        let data = 30 + usize::from(u16::from_le_bytes([z[26], z[27]])) + usize::from(u16::from_le_bytes([z[28], z[29]]));
        for b in &mut z[data..data + 4] {
            *b = 0xFF;
        }
        let mut pkg = Package::open(&z).unwrap();
        assert!(matches!(pkg.read("a.xml"), Err(IoError::Zip(m)) if m.starts_with("a.xml: ")));
        let later = pkg.read("b.xml").unwrap_err().to_string();
        assert!(later.contains("a.xml") && !later.contains("1 GB"), "{later}");
        assert_eq!(pkg.total, MAX_TOTAL, "the budget stays spent");
        assert!(pkg.checksum_failures().is_empty());
    }

    /// Recovery from local headers stops at an entry written with a data descriptor (its size
    /// comes after its data); the complete entries before it are kept.
    #[test]
    fn recovery_stops_at_a_data_descriptor_entry() {
        let z = crate::tests::make_zip(&[("a.xml", b"0123456789"), ("b.xml", b"hello")]);
        let cd = z.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
        let mut cursor = Cursor::new(&z[..cd]);
        assert_eq!(recover_entry(&mut cursor, MAX_TOTAL), Some(Some(("a.xml".into(), b"0123456789".to_vec()))));
        assert_eq!(recover_entry(&mut cursor, MAX_TOTAL), Some(Some(("b.xml".into(), b"hello".to_vec()))));
        assert_eq!(recover_entry(&mut cursor, MAX_TOTAL), None);

        // General purpose bit 3 on the first entry: its sizes would follow its data.
        let mut streamed = z[..cd].to_vec();
        streamed[6] |= 1 << 3;
        assert_eq!(recover_entry(&mut Cursor::new(&streamed[..]), MAX_TOTAL), None);
        // Over the remaining package allowance: not read.
        assert_eq!(recover_entry(&mut Cursor::new(&z[..cd]), 3), None);
    }
}
