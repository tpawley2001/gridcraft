//! The Compound File Binary (CFB/OLE2) container as the in-tree `.xls` reader needs it: streams
//! and storages read from, and written to, a compound file. (Telling an encrypted package from a
//! legacy file is `craft-cfb`'s, see `crypto::is_encrypted`.) Every read is bounds-checked and
//! every loop bounded.

/// The CFB signature at the start of the file.
pub(crate) const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

const END_OF_CHAIN: u32 = 0xFFFF_FFFE;
/// Sector ids at or above this are markers (free, end of chain, FAT/DIFAT sector), not sectors.
const MAX_REG_SECT: u32 = 0xFFFF_FFFA;
/// DIFAT sectors followed at most.
const MAX_DIFAT_SECTORS: usize = 1024;

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([*s.first()?, *s.get(1)?, *s.get(2)?, *s.get(3)?]))
}

/// Directory entry type of a stream object ([MS-CFB] 2.6.1).
const STREAM_OBJECT: u8 = 2;
/// Directory entry type of a storage object ([MS-CFB] 2.6.1).
const STORAGE_OBJECT: u8 = 1;
/// "No entry" in the directory tree (NOSTREAM).
const NO_STREAM: u32 = 0xFFFF_FFFF;
/// Streams larger than this are refused (the format allows more, a workbook never needs it).
const MAX_STREAM: u64 = 1 << 30;

/// One directory entry ([MS-CFB] 2.6).
struct DirEntry {
    name: String,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
    clsid: [u8; 16],
}

/// A parsed compound file: the FAT, the mini FAT and the directory, all bounds-checked.
/// Implemented from Microsoft's public [MS-CFB] specification.
pub(crate) struct Compound<'a> {
    bytes: &'a [u8],
    sector_size: usize,
    mini_sector_size: usize,
    mini_cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    entries: Vec<DirEntry>,
}

impl<'a> Compound<'a> {
    /// Parses the header, FAT, mini FAT and directory. Errors on anything that can't be followed.
    pub(crate) fn open(b: &'a [u8]) -> Result<Compound<'a>, String> {
        let bad = |m: &str| format!("damaged compound file: {m}");
        if !b.starts_with(&SIGNATURE) {
            return Err(bad("missing signature"));
        }
        let major = u16_at(b, 26).ok_or_else(|| bad("truncated header"))?;
        let shift = u16_at(b, 30).ok_or_else(|| bad("truncated header"))?;
        if shift != 9 && shift != 12 {
            return Err(bad("unsupported sector size"));
        }
        let mini_shift = u16_at(b, 32).ok_or_else(|| bad("truncated header"))?;
        if mini_shift != 6 {
            return Err(bad("unsupported mini sector size"));
        }
        let sector_size = 1usize << shift;
        let per = sector_size / 4;
        // Sectors that physically exist after the header sector: every chain is bounded by it.
        let sectors = (b.len() / sector_size).saturating_sub(1);
        let sector = |id: u32| -> Option<&[u8]> {
            let start = (id as usize).checked_add(1)?.checked_mul(sector_size)?;
            b.get(start..start.checked_add(sector_size)?)
        };
        // DIFAT: 109 FAT sector ids in the header, then chained DIFAT sectors.
        let mut fat_sectors: Vec<u32> = (0..109).filter_map(|i| u32_at(b, 76 + i * 4)).filter(|&s| s < MAX_REG_SECT).collect();
        let mut next = u32_at(b, 68).ok_or_else(|| bad("truncated header"))?;
        let mut hops = 0usize;
        while next < MAX_REG_SECT {
            hops += 1;
            if hops > sectors.min(MAX_DIFAT_SECTORS * 64) {
                return Err(bad("DIFAT chain loops"));
            }
            let s = sector(next).ok_or_else(|| bad("DIFAT sector out of range"))?;
            fat_sectors.extend((0..per - 1).filter_map(|i| u32_at(s, i * 4)).filter(|&x| x < MAX_REG_SECT));
            next = u32_at(s, (per - 1) * 4).ok_or_else(|| bad("truncated DIFAT sector"))?;
        }
        if fat_sectors.len() > sectors {
            return Err(bad("more FAT sectors than the file holds"));
        }
        let mut fat = Vec::with_capacity(fat_sectors.len().saturating_mul(per));
        for id in fat_sectors {
            let s = sector(id).ok_or_else(|| bad("FAT sector out of range"))?;
            fat.extend((0..per).filter_map(|i| u32_at(s, i * 4)));
        }
        let mut cf = Compound { bytes: b, sector_size, mini_sector_size: 64, mini_cutoff: 4096, fat, mini_fat: vec![], entries: vec![] };
        if let Some(c) = u32_at(b, 56) {
            cf.mini_cutoff = u64::from(c);
        }
        // Directory.
        let dir_start = u32_at(b, 48).ok_or_else(|| bad("truncated header"))?;
        let dir = cf.read_chain(dir_start, u64::MAX)?;
        for e in dir.as_chunks::<128>().0 {
            let len = usize::from(u16_at(e, 64).unwrap_or(0)).min(64);
            let units: Vec<u16> = (0..len.saturating_sub(2) / 2).filter_map(|i| u16_at(e, i * 2)).collect();
            let mut size = u32_at(e, 120).map(u64::from).unwrap_or(0);
            if major >= 4 {
                size |= u64::from(u32_at(e, 124).unwrap_or(0)) << 32;
            }
            cf.entries.push(DirEntry {
                name: String::from_utf16_lossy(&units),
                kind: e.get(66).copied().unwrap_or(0),
                left: u32_at(e, 68).unwrap_or(NO_STREAM),
                right: u32_at(e, 72).unwrap_or(NO_STREAM),
                child: u32_at(e, 76).unwrap_or(NO_STREAM),
                start: u32_at(e, 116).unwrap_or(END_OF_CHAIN),
                size,
                clsid: e.get(80..96).and_then(|c| <[u8; 16]>::try_from(c).ok()).unwrap_or([0; 16]),
            });
        }
        if cf.entries.first().is_none_or(|r| r.kind != 5) {
            return Err(bad("missing root entry"));
        }
        // Mini FAT.
        let mini_start = u32_at(b, 60).ok_or_else(|| bad("truncated header"))?;
        if mini_start < MAX_REG_SECT {
            let raw = cf.read_chain(mini_start, u64::MAX)?;
            cf.mini_fat = (0..raw.len() / 4).filter_map(|i| u32_at(&raw, i * 4)).collect();
        }
        Ok(cf)
    }

    /// Follows a FAT chain from `start`, reading at most `limit` bytes. A chain longer than the
    /// FAT (so a cycle) or pointing outside the file is an error. The file may end inside the
    /// chain's last sector (some writers don't pad it, and Excel opens those files): that sector
    /// reads short, and the caller checks the stream's declared size against what was read.
    fn read_chain(&self, start: u32, limit: u64) -> Result<Vec<u8>, String> {
        let limit = usize::try_from(limit.min(MAX_STREAM)).unwrap_or(usize::MAX);
        let mut out = Vec::new();
        let mut id = start;
        let mut seen = vec![false; self.fat.len()];
        let mut cut = false;
        while id != END_OF_CHAIN && out.len() < limit {
            if cut {
                return Err("damaged compound file: sector out of range (the file may be truncated)".into());
            }
            if id >= MAX_REG_SECT {
                return Err("damaged compound file: broken sector chain".into());
            }
            match seen.get_mut(id as usize) {
                Some(v) if !*v => *v = true,
                Some(_) => return Err("damaged compound file: sector chain loops".into()),
                None => return Err("damaged compound file: sector missing from the FAT".into()),
            }
            let start = (id as usize).checked_add(1).and_then(|i| i.checked_mul(self.sector_size));
            let s = start
                .and_then(|s| self.bytes.get(s..s.checked_add(self.sector_size)?.min(self.bytes.len())))
                .ok_or("damaged compound file: sector out of range (the file may be truncated)")?;
            cut = s.len() < self.sector_size;
            out.extend_from_slice(s);
            id = *self.fat.get(id as usize).ok_or("damaged compound file: sector missing from the FAT")?;
        }
        Ok(out)
    }

    /// Follows a mini FAT chain inside the mini stream container. Like `read_chain`, the
    /// container may end inside the chain's last mini sector.
    fn read_mini_chain(&self, container: &[u8], start: u32, limit: usize) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        let mut id = start;
        let mut seen = vec![false; self.mini_fat.len()];
        let mut cut = false;
        while id != END_OF_CHAIN && out.len() < limit {
            if cut {
                return Err("damaged compound file: mini sector out of range".into());
            }
            if id >= MAX_REG_SECT {
                return Err("damaged compound file: broken mini sector chain".into());
            }
            match seen.get_mut(id as usize) {
                Some(v) if !*v => *v = true,
                Some(_) => return Err("damaged compound file: mini sector chain loops".into()),
                None => return Err("damaged compound file: mini sector missing from the mini FAT".into()),
            }
            let at = (id as usize).checked_mul(self.mini_sector_size);
            let s = at
                .and_then(|a| container.get(a..a.checked_add(self.mini_sector_size)?.min(container.len())))
                .ok_or("damaged compound file: mini sector out of range")?;
            cut = s.len() < self.mini_sector_size;
            out.extend_from_slice(s);
            id = *self.mini_fat.get(id as usize).ok_or("damaged compound file: mini sector missing from the mini FAT")?;
        }
        Ok(out)
    }

    /// The entry ids of the children of a storage whose child tree starts at `child` (a
    /// red-black tree through left/right siblings), walked with a bounded stack.
    fn children(&self, child: u32) -> Result<Vec<usize>, String> {
        let mut out = Vec::new();
        let mut stack = vec![child];
        let mut seen = 0usize;
        while let Some(i) = stack.pop() {
            if i == NO_STREAM {
                continue;
            }
            seen += 1;
            if seen > self.entries.len() {
                return Err("damaged compound file: directory tree loops".into());
            }
            let Some(e) = self.entries.get(i as usize) else { continue };
            out.push(i as usize);
            stack.push(e.left);
            stack.push(e.right);
        }
        Ok(out)
    }

    /// The directory entry called `name` (case-insensitive) of type `kind` directly under the
    /// storage `parent`.
    fn child_entry(&self, parent: &DirEntry, name: &str, kind: u8) -> Result<Option<&DirEntry>, String> {
        for i in self.children(parent.child)? {
            if let Some(e) = self.entries.get(i)
                && e.kind == kind
                && e.name.eq_ignore_ascii_case(name)
            {
                return Ok(Some(e));
            }
        }
        Ok(None)
    }

    /// The directory entry of the stream called `name` (case-insensitive) directly under the
    /// root storage.
    fn root_entry(&self, name: &str) -> Result<Option<&DirEntry>, String> {
        let Some(root) = self.entries.first() else { return Ok(None) };
        self.child_entry(root, name, STREAM_OBJECT)
    }

    /// Whether the root storage holds a stream called `name`, without reading it.
    pub(crate) fn has_root_stream(&self, name: &str) -> bool {
        matches!(self.root_entry(name), Ok(Some(_)))
    }

    /// The contents of the stream called `name` (case-insensitive) directly under the root
    /// storage, or `None` when there is no such stream.
    pub(crate) fn root_stream(&self, name: &str) -> Result<Option<Vec<u8>>, String> {
        let found = self.root_entry(name)?;
        let Some(e) = found else { return Ok(None) };
        let mut container = None;
        self.read_entry(e, &mut container).map(Some)
    }

    /// The contents of the stream `name` inside the storage `storage`, both directly under the
    /// root and matched case-insensitively, or `None` when either is missing.
    pub(crate) fn storage_stream(&self, storage: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
        let Some(root) = self.entries.first() else { return Ok(None) };
        let Some(dir) = self.child_entry(root, storage, STORAGE_OBJECT)? else { return Ok(None) };
        match self.child_entry(dir, name, STREAM_OBJECT)? {
            Some(e) => self.read_entry(e, &mut None).map(Some),
            None => Ok(None),
        }
    }

    /// The bytes of the stream entry `e`. `container` caches the mini stream between calls.
    fn read_entry(&self, e: &DirEntry, container: &mut Option<Vec<u8>>) -> Result<Vec<u8>, String> {
        if e.size > MAX_STREAM {
            return Err("the workbook stream is larger than 1 GiB".into());
        }
        let size = usize::try_from(e.size).map_err(|_| "stream too large".to_string())?;
        let mut data = if e.size < self.mini_cutoff {
            if size == 0 {
                return Ok(Vec::new());
            }
            let mini = match container.take() {
                Some(c) => c,
                None => {
                    let root = self.entries.first().ok_or("damaged compound file: missing root entry")?;
                    self.read_chain(root.start, root.size)?
                }
            };
            let read = self.read_mini_chain(&mini, e.start, size);
            *container = Some(mini);
            read?
        } else {
            self.read_chain(e.start, e.size)?
        };
        if data.len() < size {
            return Err("damaged compound file: a stream is shorter than its declared size (the file may be truncated)".into());
        }
        data.truncate(size);
        Ok(data)
    }

    /// Everything inside the storage called `name` (case-insensitive) directly under the root,
    /// or `None` when there is no such storage. Nesting is bounded by [`MAX_DEPTH`], every entry
    /// is visited at most once, and the streams' total size is capped at [`MAX_STREAM`].
    pub(crate) fn root_storage(&self, name: &str) -> Result<Option<Vec<Node>>, String> {
        let Some(root) = self.entries.first() else { return Ok(None) };
        let found = self
            .children(root.child)?
            .into_iter()
            .find(|&i| self.entries.get(i).is_some_and(|e| e.kind == STORAGE_OBJECT && e.name.eq_ignore_ascii_case(name)));
        let Some(i) = found else { return Ok(None) };
        let mut walk = Walk { visited: vec![false; self.entries.len()], total: 0, container: None };
        if let Some(v) = walk.visited.get_mut(0) {
            *v = true;
        }
        self.subtree(i, 0, &mut walk).map(Some)
    }

    fn subtree(&self, storage: usize, depth: usize, walk: &mut Walk) -> Result<Vec<Node>, String> {
        if depth > MAX_DEPTH {
            return Err("damaged compound file: storages nested too deeply".into());
        }
        match walk.visited.get_mut(storage) {
            Some(v) if !*v => *v = true,
            _ => return Err("damaged compound file: directory tree loops".into()),
        }
        let Some(s) = self.entries.get(storage) else { return Err("damaged compound file: missing directory entry".into()) };
        let mut out = Vec::new();
        for i in self.children(s.child)? {
            let Some(e) = self.entries.get(i) else { continue };
            match e.kind {
                STORAGE_OBJECT => {
                    let children = self.subtree(i, depth + 1, walk)?;
                    out.push(Node::Storage { name: e.name.clone(), clsid: e.clsid, children });
                }
                STREAM_OBJECT => {
                    match walk.visited.get_mut(i) {
                        Some(v) if !*v => *v = true,
                        _ => return Err("damaged compound file: directory tree loops".into()),
                    }
                    walk.total = walk.total.saturating_add(e.size);
                    if walk.total > MAX_STREAM {
                        return Err("the storage is larger than 1 GiB".into());
                    }
                    let data = self.read_entry(e, &mut walk.container)?;
                    out.push(Node::Stream { name: e.name.clone(), data });
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

/// State shared while reading a storage subtree.
struct Walk {
    visited: Vec<bool>,
    total: u64,
    container: Option<Vec<u8>>,
}

/// Storages nested deeper than this are refused (a VBA project uses three levels).
const MAX_DEPTH: usize = 32;

/// A storage or stream inside a compound file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Storage { name: String, clsid: [u8; 16], children: Vec<Node> },
    Stream { name: String, data: Vec<u8> },
}

impl Node {
    pub(crate) fn name(&self) -> &str {
        match self {
            Node::Storage { name, .. } | Node::Stream { name, .. } => name,
        }
    }
}

const SECTOR: usize = 512;
const MINI_SECTOR: usize = 64;
const MINI_CUTOFF: usize = 4096;
const FREE_SECT: u32 = 0xFFFF_FFFF;
const FAT_SECT: u32 = 0xFFFF_FFFD;
const DIFAT_SECT: u32 = 0xFFFF_FFFC;
/// Ids per FAT sector, and DIFAT ids per DIFAT sector (the last slot links the next one).
const PER_SECTOR: usize = SECTOR / 4;

/// The [MS-CFB] 2.6.4 order of sibling names: shorter first, then by upper-cased UTF-16 units.
fn name_order(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |s: &str| -> Vec<u16> {
        s.chars()
            .map(|c| {
                let mut up = c.to_uppercase();
                match (up.next(), up.next()) {
                    (Some(u), None) => u,
                    _ => c,
                }
            })
            .collect::<String>()
            .encode_utf16()
            .collect()
    };
    a.encode_utf16().count().cmp(&b.encode_utf16().count()).then_with(|| key(a).cmp(&key(b)))
}

/// A directory entry being written.
struct OutEntry {
    name: Vec<u16>,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    clsid: [u8; 16],
    /// Index into the stream list for stream entries.
    stream: Option<usize>,
}

fn to_u32(n: usize) -> Result<u32, String> {
    u32::try_from(n).ok().filter(|&v| v < MAX_REG_SECT).ok_or_else(|| "compound file too large".to_string())
}

/// Links the sorted sibling ids `ids[lo..hi]` as a balanced binary tree; returns its root.
fn link_siblings(entries: &mut [OutEntry], ids: &[u32], depth: usize) -> u32 {
    if ids.is_empty() || depth > 64 {
        return NO_STREAM;
    }
    let mid = ids.len() / 2;
    let (left, rest) = ids.split_at(mid);
    let Some((&me, right)) = rest.split_first() else { return NO_STREAM };
    let l = link_siblings(entries, left, depth + 1);
    let r = link_siblings(entries, right, depth + 1);
    if let Some(e) = entries.get_mut(me as usize) {
        e.left = l;
        e.right = r;
    }
    me
}

/// Adds `nodes` (children of the entry `parent`) to the directory, recursively.
fn add_children<'n>(nodes: &'n [Node], parent: usize, depth: usize, entries: &mut Vec<OutEntry>, streams: &mut Vec<&'n [u8]>) -> Result<(), String> {
    if depth > MAX_DEPTH {
        return Err("storages nested too deeply".into());
    }
    let mut sorted: Vec<&Node> = nodes.iter().collect();
    sorted.sort_by(|a, b| name_order(a.name(), b.name()));
    let mut ids = Vec::with_capacity(sorted.len());
    let mut storages = Vec::new();
    for n in &sorted {
        let name: Vec<u16> = n.name().encode_utf16().collect();
        if name.is_empty() || name.len() > 31 {
            return Err(format!("the name \"{}\" is not a valid compound file name", n.name()));
        }
        let id = entries.len();
        ids.push(to_u32(id)?);
        let (kind, clsid, stream) = match n {
            Node::Storage { clsid, children, .. } => {
                storages.push((id, children.as_slice()));
                (STORAGE_OBJECT, *clsid, None)
            }
            Node::Stream { data, .. } => {
                streams.push(data);
                (STREAM_OBJECT, [0; 16], Some(streams.len() - 1))
            }
        };
        entries.push(OutEntry { name, kind, left: NO_STREAM, right: NO_STREAM, child: NO_STREAM, clsid, stream });
    }
    let top = link_siblings(entries, &ids, 0);
    if let Some(p) = entries.get_mut(parent) {
        p.child = top;
    }
    for (id, children) in storages {
        add_children(children, id, depth + 1, entries, streams)?;
    }
    Ok(())
}

/// Appends a chain of `count` sectors starting at `first` to the FAT; returns the start id.
fn chain(fat: &mut [u32], first: usize, count: usize) -> Result<u32, String> {
    if count == 0 {
        return Ok(END_OF_CHAIN);
    }
    for k in 0..count {
        let next = if k + 1 == count { END_OF_CHAIN } else { to_u32(first + k + 1)? };
        let slot = fat.get_mut(first + k).ok_or("compound file layout overflow")?;
        *slot = next;
    }
    to_u32(first)
}

/// Writes a version 3 compound file ([MS-CFB]: 512-byte sectors, 64-byte mini sectors below
/// 4096 bytes) whose root storage holds `nodes`. Storage CLSIDs are kept; the root's CLSID and
/// every timestamp are zero. Siblings form a binary tree in [MS-CFB] 2.6.4 order, all black.
pub(crate) fn write_compound(nodes: &[Node]) -> Result<Vec<u8>, String> {
    let root_name: Vec<u16> = "Root Entry".encode_utf16().collect();
    let mut entries = vec![OutEntry { name: root_name, kind: 5, left: NO_STREAM, right: NO_STREAM, child: NO_STREAM, clsid: [0; 16], stream: None }];
    let mut streams: Vec<&[u8]> = Vec::new();
    add_children(nodes, 0, 0, &mut entries, &mut streams)?;
    let total: usize = streams.iter().fold(0usize, |t, s| t.saturating_add(s.len()));
    if total as u64 > MAX_STREAM {
        return Err("the streams are larger than 1 GiB".into());
    }

    // The mini stream and its FAT.
    let mut container: Vec<u8> = Vec::new();
    let mut mini_fat: Vec<u32> = Vec::new();
    let mut mini_starts = vec![END_OF_CHAIN; streams.len()];
    for (s, start) in streams.iter().zip(mini_starts.iter_mut()) {
        if s.is_empty() || s.len() >= MINI_CUTOFF {
            continue;
        }
        let first = mini_fat.len();
        let n = s.len().div_ceil(MINI_SECTOR);
        mini_fat.resize(first + n, FREE_SECT);
        *start = chain(&mut mini_fat, first, n)?;
        container.extend_from_slice(s);
        container.resize((first + n) * MINI_SECTOR, 0);
    }
    let mini_fat_bytes: Vec<u8> = mini_fat.iter().flat_map(|v| v.to_le_bytes()).collect();

    // Regular-sector blobs after the FAT and DIFAT: directory, mini FAT, mini stream, then each
    // stream at or above the cutoff.
    let mut dir = vec![0u8; entries.len().div_ceil(4) * SECTOR];
    let mut blobs: Vec<&[u8]> = vec![&[], &mini_fat_bytes, &container];
    blobs.extend(streams.iter().map(|s| if s.len() >= MINI_CUTOFF { *s } else { &[][..] }));
    let counts: Vec<usize> = std::iter::once(dir.len() / SECTOR).chain(blobs.iter().skip(1).map(|b| b.len().div_ceil(SECTOR))).collect();
    let body = counts.iter().fold(0usize, |t, &c| t.saturating_add(c));
    let difat_for = |fat_n: usize| fat_n.saturating_sub(109).div_ceil(PER_SECTOR - 1);
    let mut fat_n = 1usize;
    while fat_n.saturating_mul(PER_SECTOR) < fat_n + difat_for(fat_n) + body {
        fat_n += 1;
    }
    let difat_n = difat_for(fat_n);
    let mut fat = vec![FREE_SECT; fat_n * PER_SECTOR];
    for slot in fat.iter_mut().take(fat_n) {
        *slot = FAT_SECT;
    }
    for slot in fat.iter_mut().skip(fat_n).take(difat_n) {
        *slot = DIFAT_SECT;
    }
    let mut starts = Vec::with_capacity(counts.len());
    let mut next = fat_n + difat_n;
    for &n in &counts {
        starts.push(chain(&mut fat, next, n)?);
        next += n;
    }
    let start_of = |k: usize| starts.get(k).copied().unwrap_or(END_OF_CHAIN);

    // Directory entries.
    for (k, e) in entries.iter().enumerate() {
        let at = k * 128;
        let Some(slot) = dir.get_mut(at..at + 128) else { continue };
        for (i, u) in e.name.iter().enumerate() {
            put16(slot, i * 2, *u);
        }
        put16(slot, 64, u16::try_from((e.name.len() + 1) * 2).unwrap_or(64));
        put8(slot, 66, e.kind);
        put8(slot, 67, 1); // black
        put32(slot, 68, e.left);
        put32(slot, 72, e.right);
        put32(slot, 76, e.child);
        if let Some(c) = slot.get_mut(80..96) {
            c.copy_from_slice(&e.clsid);
        }
        let (start, size) = match e.stream {
            None if k == 0 => (if container.is_empty() { END_OF_CHAIN } else { start_of(2) }, container.len()),
            None => (0, 0),
            Some(i) => {
                let len = streams.get(i).map_or(0, |s| s.len());
                let start = if len == 0 {
                    END_OF_CHAIN
                } else if len < MINI_CUTOFF {
                    mini_starts.get(i).copied().unwrap_or(END_OF_CHAIN)
                } else {
                    start_of(3 + i)
                };
                (start, len)
            }
        };
        put32(slot, 116, start);
        put32(slot, 120, to_u32(size)?);
    }
    for k in entries.len()..dir.len() / 128 {
        if let Some(slot) = dir.get_mut(k * 128..k * 128 + 128) {
            for at in [68, 72, 76] {
                put32(slot, at, NO_STREAM);
            }
        }
    }

    // Header.
    let mut out = vec![0u8; SECTOR];
    if let Some(sig) = out.get_mut(..8) {
        sig.copy_from_slice(&SIGNATURE);
    }
    put16(&mut out, 24, 0x3E);
    put16(&mut out, 26, 3);
    put16(&mut out, 28, 0xFFFE);
    put16(&mut out, 30, 9);
    put16(&mut out, 32, 6);
    put32(&mut out, 44, to_u32(fat_n)?);
    put32(&mut out, 48, start_of(0));
    put32(&mut out, 56, MINI_CUTOFF as u32);
    put32(&mut out, 60, start_of(1));
    put32(&mut out, 64, to_u32(counts.get(1).copied().unwrap_or(0))?);
    put32(&mut out, 68, if difat_n == 0 { END_OF_CHAIN } else { to_u32(fat_n)? });
    put32(&mut out, 72, to_u32(difat_n)?);
    for i in 0..109 {
        put32(&mut out, 76 + i * 4, if i < fat_n { to_u32(i)? } else { FREE_SECT });
    }
    let sector_bytes = (fat_n + difat_n + body).saturating_mul(SECTOR);
    out.reserve(sector_bytes);
    // FAT sectors.
    out.extend(fat.iter().flat_map(|v| v.to_le_bytes()));
    // DIFAT sectors: the FAT sector ids past the header's 109, chained.
    let extra: Vec<usize> = (109..fat_n).collect();
    for (d, ids) in extra.chunks(PER_SECTOR - 1).enumerate() {
        let mut s = vec![FREE_SECT; PER_SECTOR];
        for (slot, &id) in s.iter_mut().zip(ids) {
            *slot = to_u32(id)?;
        }
        if let Some(last) = s.last_mut() {
            *last = if d + 1 == difat_n { END_OF_CHAIN } else { to_u32(fat_n + d + 1)? };
        }
        out.extend(s.iter().flat_map(|v| v.to_le_bytes()));
    }
    // Directory, mini FAT, mini stream, large streams, each padded to whole sectors.
    out.extend_from_slice(&dir);
    for b in blobs.iter().skip(1) {
        out.extend_from_slice(b);
        out.resize(out.len().div_ceil(SECTOR) * SECTOR, 0);
    }
    Ok(out)
}

fn put8(b: &mut [u8], at: usize, v: u8) {
    if let Some(x) = b.get_mut(at) {
        *x = v;
    }
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    if let Some(s) = b.get_mut(at..at + 2) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

fn put32(b: &mut [u8], at: usize, v: u32) {
    if let Some(s) = b.get_mut(at..at + 4) {
        s.copy_from_slice(&v.to_le_bytes());
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `compound_file` with one regular-sector stream of `len` bytes (FAT in sector 0, directory
    /// in sector 1, the stream from sector 2), cut right after the stream's last byte, the way
    /// some writers leave the final sector unpadded.
    fn unpadded(len: usize) -> (Vec<u8>, Vec<u8>) {
        let data: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        let mut file = crate::testing::compound_file(&[("Workbook", &data)]);
        file.truncate(512 * 3 + len);
        (file, data)
    }

    #[test]
    fn an_unpadded_last_sector_still_reads() {
        let (file, data) = unpadded(5000);
        assert_ne!(file.len() % 512, 0);
        let cf = Compound::open(&file).unwrap();
        assert_eq!(cf.root_stream("Workbook").unwrap().unwrap(), data);
        // Bytes the stream needs that the file doesn't hold are still damage.
        let cf = Compound::open(&file[..file.len() - 1]).unwrap();
        assert!(cf.root_stream("Workbook").is_err());
    }

    #[test]
    fn an_unpadded_last_mini_sector_still_reads() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let mut file = crate::testing::compound_file(&[("Workbook", &data)]);
        // Header, FAT, directory, mini FAT, then the mini stream holding the stream's bytes.
        file.truncate(512 * 4 + data.len());
        let cf = Compound::open(&file).unwrap();
        assert_eq!(cf.root_stream("Workbook").unwrap().unwrap(), data);
        let cf = Compound::open(&file[..file.len() - 1]).unwrap();
        assert!(cf.root_stream("Workbook").is_err());
    }

    #[test]
    fn a_cut_sector_inside_a_chain_is_damage() {
        let (mut file, _) = unpadded(5000);
        // The cut sector (11) points on to the directory sector and the stream claims those
        // bytes too: the partial sector would splice 392 bytes into the middle of the stream.
        file[512 + 11 * 4..512 + 12 * 4].copy_from_slice(&1u32.to_le_bytes());
        file[1024 + 128 + 120..1024 + 128 + 124].copy_from_slice(&5512u32.to_le_bytes());
        let cf = Compound::open(&file).unwrap();
        assert!(cf.root_stream("Workbook").is_err());
    }

    #[test]
    fn a_cut_mini_sector_inside_a_chain_is_damage() {
        let small = [7u8; 100];
        let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let mut file = crate::testing::compound_file(&[("Small", &small), ("Workbook", &data)]);
        // "Small" holds mini sectors 0-1, "Workbook" 2-17; the mini stream starts at sector 3.
        file.truncate(512 * 4 + 128 + data.len());
        assert_eq!(Compound::open(&file).unwrap().root_stream("Workbook").unwrap().unwrap(), data);
        // The cut mini sector (17) points on to sector 0 and the stream claims those bytes too.
        file[1536 + 17 * 4..1536 + 18 * 4].copy_from_slice(&0u32.to_le_bytes());
        file[1024 + 2 * 128 + 120..1024 + 2 * 128 + 124].copy_from_slice(&1064u32.to_le_bytes());
        let cf = Compound::open(&file).unwrap();
        assert!(cf.root_stream("Workbook").is_err());
    }

    fn bytes(len: usize, seed: u8) -> Vec<u8> {
        (0..len).map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed)).collect()
    }

    fn stream(name: &str, data: Vec<u8>) -> Node {
        Node::Stream { name: name.into(), data }
    }

    /// Reads every child of the root back as nodes, through the public reading paths.
    fn read_back(file: &[u8], storages: &[&str]) -> Vec<(String, Vec<Node>)> {
        let cf = Compound::open(file).unwrap();
        storages.iter().map(|s| (s.to_string(), cf.root_storage(s).unwrap().unwrap())).collect()
    }

    fn sorted(mut nodes: Vec<Node>) -> Vec<Node> {
        nodes.sort_by(|a, b| a.name().cmp(b.name()));
        nodes
            .into_iter()
            .map(|n| match n {
                Node::Storage { name, clsid, children } => Node::Storage { name, clsid, children: sorted(children) },
                s => s,
            })
            .collect()
    }

    #[test]
    fn writes_a_nested_tree_that_reads_back() {
        let clsid: [u8; 16] = std::array::from_fn(|i| i as u8 + 1);
        let vba = vec![stream("dir", bytes(10, 1)), stream("Module1", bytes(5000, 2)), stream("Empty", vec![])];
        let form = vec![stream("f", bytes(100, 3)), stream("o", bytes(4096, 4))];
        let nodes = vec![
            Node::Storage { name: "VBA".into(), clsid: [0; 16], children: vba.clone() },
            stream("PROJECT", bytes(300, 5)),
            Node::Storage { name: "UserForm1".into(), clsid, children: form.clone() },
        ];
        let file = write_compound(&nodes).unwrap();
        assert_eq!(file.len() % 512, 0);
        let cf = Compound::open(&file).unwrap();
        assert_eq!(cf.root_stream("PROJECT").unwrap().unwrap(), bytes(300, 5));
        assert_eq!(cf.root_storage("vba").unwrap().unwrap().len(), 3);
        let back = read_back(&file, &["VBA", "UserForm1"]);
        assert_eq!(sorted(back[0].1.clone()), sorted(vba));
        assert_eq!(sorted(back[1].1.clone()), sorted(form));
        // The storage CLSID survives.
        let e = cf.entries.iter().find(|e| e.name == "UserForm1").unwrap();
        assert_eq!(e.clsid, clsid);
        assert_eq!(cf.entries[0].clsid, [0; 16]);
        assert!(cf.root_storage("Missing").unwrap().is_none());
    }

    #[test]
    fn siblings_follow_the_cfb_name_order() {
        let names = ["b", "A", "ccc", "Ab", "aa", "Z"];
        let file = write_compound(&names.iter().map(|n| stream(n, vec![1])).collect::<Vec<_>>()).unwrap();
        let cf = Compound::open(&file).unwrap();
        // An in-order walk of the root's tree is sorted by the 2.6.4 comparison.
        fn walk(cf: &Compound<'_>, i: u32, out: &mut Vec<String>) {
            if i == NO_STREAM {
                return;
            }
            let e = &cf.entries[i as usize];
            walk(cf, e.left, out);
            out.push(e.name.clone());
            walk(cf, e.right, out);
        }
        let mut order = vec![];
        walk(&cf, cf.entries[0].child, &mut order);
        assert_eq!(order, ["A", "b", "Z", "aa", "Ab", "ccc"]);
        for n in names {
            assert_eq!(cf.root_stream(n).unwrap().unwrap(), vec![1]);
        }
    }

    #[test]
    fn a_stream_needing_several_fat_sectors_reads_back() {
        let big = bytes(200 * 512 + 17, 9);
        let file = write_compound(&[stream("Big", big.clone()), stream("small", bytes(5, 1))]).unwrap();
        assert!(u32_at(&file, 44).unwrap() >= 2);
        let cf = Compound::open(&file).unwrap();
        assert_eq!(cf.root_stream("Big").unwrap().unwrap(), big);
        assert_eq!(cf.root_stream("small").unwrap().unwrap(), bytes(5, 1));
    }

    #[test]
    fn the_difat_chain_is_written_past_109_fat_sectors() {
        // 110 FAT sectors' worth of data: one DIFAT sector.
        let big = vec![7u8; 110 * 128 * 512];
        let file = write_compound(&[stream("Big", big.clone())]).unwrap();
        assert_eq!(u32_at(&file, 72), Some(1));
        assert_eq!(Compound::open(&file).unwrap().root_stream("Big").unwrap().unwrap(), big);
    }

    #[test]
    fn invalid_names_are_refused() {
        assert!(write_compound(&[stream("", vec![])]).is_err());
        assert!(write_compound(&[stream(&"x".repeat(32), vec![])]).is_err());
    }

    #[test]
    fn a_storage_tree_that_loops_is_damage() {
        let nodes = vec![Node::Storage { name: "S".into(), clsid: [0; 16], children: vec![stream("a", vec![1])] }];
        let mut file = write_compound(&nodes).unwrap();
        // Point the storage's child at the root's own entry.
        let s = (0..4).find(|k| file[512 * 2 + k * 128] == b'S').unwrap();
        file[512 * 2 + s * 128 + 76..512 * 2 + s * 128 + 80].copy_from_slice(&1u32.to_le_bytes());
        let cf = Compound::open(&file).unwrap();
        assert!(cf.root_storage("S").is_err());
    }
}
