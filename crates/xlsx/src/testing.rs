//! Test support: builds password-encrypted workbooks in code, so tests here and in the crates
//! above (engine, UI) never need an encrypted file on disk. Compiled for this crate's tests and,
//! elsewhere, only with the `test-support` feature (a dev-dependency feature, never shipped).
//! The encryption is `craft-offcrypto`'s; the compound file is laid out here (storages included).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod xls_pivot;
pub mod xlsb_pivot;

/// The spin count `encrypt_workbook` uses: Excel's default.
pub const SPIN_COUNT: u32 = 100_000;

/// `package` (normally an `.xlsx` zip) as Excel stores it with a password: Agile encryption
/// (AES-256, SHA-512) inside a compound file.
pub fn encrypt_workbook(package: &[u8], password: &str) -> Vec<u8> {
    craft_offcrypto::encrypt_with_spin_count(package, password, SPIN_COUNT).unwrap()
}

/// The same with Standard encryption (AES-128, SHA-1), as Excel 2007 wrote it.
pub fn encrypt_workbook_standard(package: &[u8], password: &str) -> Vec<u8> {
    craft_offcrypto::encrypt_standard(package, password, 128).unwrap()
}

/// A version 3 compound file (512-byte sectors) holding `streams` under the root, laid out as
/// Excel lays it out: streams under 4096 bytes in the mini stream, larger ones in regular
/// sectors. Up to 109 FAT sectors (about 7 MB of streams). A name `Storage/Stream` puts the
/// stream inside a storage of that name under the root.
pub fn compound_file(streams: &[(&str, &[u8])]) -> Vec<u8> {
    const SECTOR: usize = 512;
    const MINI: usize = 64;
    const CUTOFF: usize = 4096;
    const FREE: u32 = 0xFFFF_FFFF;
    const END: u32 = 0xFFFF_FFFE;
    const FAT_SECTOR: u32 = 0xFFFF_FFFD;
    let is_mini = |d: &[u8]| d.len() < CUTOFF;

    // The mini stream: small streams in 64-byte mini sectors, chained by the mini FAT.
    let mut container = vec![];
    let mut mini_fat: Vec<u32> = vec![];
    let mut mini_starts = vec![];
    for (_, d) in streams {
        if !is_mini(d) || d.is_empty() {
            mini_starts.push(END);
            continue;
        }
        let first = mini_fat.len();
        let n = d.len().div_ceil(MINI);
        for k in 0..n {
            mini_fat.push(if k + 1 == n { END } else { (first + k + 1) as u32 });
        }
        mini_starts.push(first as u32);
        container.extend_from_slice(d);
        container.resize((first + n) * MINI, 0);
    }

    // Storages named by `Storage/Stream` paths, after the streams in the directory.
    let mut storages: Vec<&str> = vec![];
    for (name, _) in streams {
        if let Some((st, _)) = name.split_once('/')
            && !storages.contains(&st)
        {
            storages.push(st);
        }
    }
    // Regular-sector blobs, in order: directory, mini FAT, mini stream, then each large stream.
    let dir_len = (streams.len() + storages.len() + 1).div_ceil(4) * SECTOR;
    let mut blobs: Vec<Vec<u8>> = vec![vec![0; dir_len], mini_fat.iter().flat_map(|v| v.to_le_bytes()).collect(), container.clone()];
    blobs.extend(streams.iter().map(|(_, d)| if is_mini(d) { vec![] } else { d.to_vec() }));
    let counts: Vec<usize> = blobs.iter().map(|b| b.len().div_ceil(SECTOR)).collect();
    let body: usize = counts.iter().sum();
    let mut fat_sectors = 1;
    while fat_sectors * 128 < fat_sectors + body {
        fat_sectors += 1;
    }
    assert!(fat_sectors <= 109, "the test writer has no DIFAT sectors");
    let mut fat = vec![FREE; fat_sectors * 128];
    fat[..fat_sectors].fill(FAT_SECTOR);
    let mut starts = vec![];
    let mut next = fat_sectors;
    for &n in &counts {
        if n == 0 {
            starts.push(END);
            continue;
        }
        starts.push(next as u32);
        for k in 0..n {
            fat[next + k] = if k + 1 == n { END } else { (next + k + 1) as u32 };
        }
        next += n;
    }

    let put16 = |b: &mut [u8], at: usize, v: u16| b[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let put32 = |b: &mut [u8], at: usize, v: u32| b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    // Directory: the root (owning the mini stream), then the streams chained as right siblings.
    let dir = &mut blobs[0];
    for k in 0..dir_len / 128 {
        for at in [68, 72, 76] {
            put32(dir, k * 128 + at, FREE);
        }
    }
    let mut entry = |k: usize, name: &str, kind: u8, start: u32, size: usize| {
        let e = k * 128;
        let units: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        dir[e..e + units.len()].copy_from_slice(&units);
        put16(dir, e + 64, units.len() as u16 + 2);
        dir[e + 66] = kind;
        dir[e + 67] = 1; // black
        put32(dir, e + 116, start);
        put32(dir, e + 120, size as u32);
    };
    entry(0, "Root Entry", 5, starts[2], container.len());
    // Each storage's children (0 = the root), chained as right siblings under its child link.
    let mut children: Vec<Vec<usize>> = vec![vec![]; storages.len() + 1];
    for (k, (name, data)) in streams.iter().enumerate() {
        let start = if is_mini(data) { mini_starts[k] } else { starts[3 + k] };
        let (parent, leaf) = match name.split_once('/') {
            Some((st, leaf)) => (1 + storages.iter().position(|s| *s == st).unwrap(), leaf),
            None => (0, *name),
        };
        entry(k + 1, leaf, 2, start, data.len());
        children[parent].push(k + 1);
    }
    for (i, st) in storages.iter().enumerate() {
        let k = streams.len() + 1 + i;
        entry(k, st, 1, 0, 0);
        children[0].push(k);
    }
    for (parent, kids) in children.iter().enumerate() {
        let parent = if parent == 0 { 0 } else { streams.len() + parent };
        if let Some(first) = kids.first() {
            put32(dir, parent * 128 + 76, *first as u32);
        }
        for pair in kids.windows(2) {
            put32(dir, pair[0] * 128 + 72, pair[1] as u32);
        }
    }

    let mut out = vec![0u8; SECTOR * (1 + fat_sectors + body)];
    out[..8].copy_from_slice(&crate::cfb::SIGNATURE);
    put16(&mut out, 24, 0x3E); // minor version
    put16(&mut out, 26, 3); // major version
    put16(&mut out, 28, 0xFFFE); // byte order
    put16(&mut out, 30, 9); // 512-byte sectors
    put16(&mut out, 32, 6); // 64-byte mini sectors
    put32(&mut out, 44, fat_sectors as u32);
    put32(&mut out, 48, starts[0]);
    put32(&mut out, 56, CUTOFF as u32);
    put32(&mut out, 60, starts[1]); // first mini FAT sector (END when there is none)
    put32(&mut out, 64, counts[1] as u32);
    put32(&mut out, 68, END); // no DIFAT sectors
    for i in 0..109 {
        put32(&mut out, 76 + i * 4, if i < fat_sectors { i as u32 } else { FREE });
    }
    for (i, v) in fat.iter().enumerate() {
        put32(&mut out, SECTOR + i * 4, *v);
    }
    for (blob, start) in blobs.iter().zip(&starts) {
        if !blob.is_empty() {
            let at = SECTOR * (1 + *start as usize);
            out[at..at + blob.len()].copy_from_slice(blob);
        }
    }
    out
}
