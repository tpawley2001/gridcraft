//! Builds Excel 97-2003 (`.xls`, BIFF8) workbooks holding a PivotTable, records assembled per
//! [MS-XLS]: "Sheet1" holds the source list A1:C7 (Region, Product, Sales) and "Report" the
//! PivotTable views, whose cache is stream `0001` of the `_SX_DB_CUR` storage.

/// The source list on Sheet1 (after the header row Region, Product, Sales).
pub const SOURCE: [(&str, &str, f64); 6] =
    [("East", "A", 10.0), ("West", "B", 20.0), ("East", "B", 5.0), ("North", "A", 7.0), ("West", "A", 3.0), ("East", "C", 8.0)];

/// Cache field indices.
pub const REGION: i16 = 0;
pub const PRODUCT: i16 = 1;
pub const SALES: i16 = 2;
/// The data field (Σ Values) in a row or column field list.
pub const VALUES: i16 = -2;

/// One PivotTable view on "Report".
#[derive(Clone, Debug, Default)]
pub struct View {
    pub name: String,
    /// Report body: first row, last row, first column, last column (zero-based).
    pub body: [u16; 4],
    pub rows: Vec<i16>,
    pub cols: Vec<i16>,
    /// (field, item index or 0x7FFD for all items).
    pub pages: Vec<(i16, i16)>,
    /// (field, iiftab, custom name).
    pub data: Vec<(i16, u16, Option<String>)>,
}

/// What the `_SX_DB_CUR` storage holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cache {
    Present,
    Missing,
    /// The stream ends inside a record.
    Truncated,
    /// These bytes as the cache stream.
    Bytes(Vec<u8>),
}

pub(super) fn rec(id: u16, payload: &[u8]) -> Vec<u8> {
    let mut v = id.to_le_bytes().to_vec();
    v.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    v.extend_from_slice(payload);
    v
}

pub(super) fn u16s(v: &[u16]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// XLUnicodeString, compressed characters.
pub(super) fn xlstr(s: &str) -> Vec<u8> {
    [&(s.len() as u16).to_le_bytes()[..], &[0], s.as_bytes()].concat()
}

/// XLUnicodeStringNoCch, compressed characters.
fn nocch(s: &str) -> Vec<u8> {
    [&[0u8][..], s.as_bytes()].concat()
}

pub(super) fn bof(dt: u16) -> Vec<u8> {
    rec(0x0809, &[&u16s(&[0x0600, dt, 0x0DBB, 0x07CC])[..], &[0; 8]].concat())
}

pub(super) fn eof() -> Vec<u8> {
    rec(0x000A, &[])
}

fn label(r: u16, c: u16, s: &str) -> Vec<u8> {
    rec(0x0204, &[u16s(&[r, c, 0]), xlstr(s)].concat())
}

fn number(r: u16, c: u16, n: f64) -> Vec<u8> {
    rec(0x0203, &[&u16s(&[r, c, 0])[..], &n.to_le_bytes()].concat())
}

/// The distinct values of a source column, in first-appearance order (the cache items).
fn items(field: i16) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for (region, product, sales) in SOURCE {
        let v = match field {
            REGION => region.to_string(),
            PRODUCT => product.to_string(),
            _ => sales.to_string(),
        };
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// The pivot cache stream: SXDB, then per field SXFDB and its cache items, then EOF.
pub fn cache_stream() -> Vec<u8> {
    let mut s = rec(0x00C6, &[&6u32.to_le_bytes()[..], &u16s(&[1, 0x0001, 0, 3, 3, 6, 1, 0])].concat());
    for (field, name) in [(REGION, "Region"), (PRODUCT, "Product"), (SALES, "Sales")] {
        let its = items(field);
        // fAllAtoms, plus fNumField for Sales or fTextEtcField for the others.
        let flags = if field == SALES { 0x0001 | 0x0020 | 0x0100 } else { 0x0001 | 0x0080 };
        s.extend(rec(0x00C7, &[u16s(&[flags, 0, 0, 0, 0, 0, its.len() as u16]), xlstr(name)].concat()));
        for it in its {
            s.extend(match it.parse::<f64>() {
                Ok(n) if field == SALES => rec(0x00C9, &n.to_le_bytes()),
                _ => rec(0x00CD, &xlstr(&it)),
            });
        }
    }
    s.extend(eof());
    s
}

/// The records of one PivotTable view.
fn view_records(v: &View) -> Vec<u8> {
    let data_fields: Vec<i16> = v.data.iter().map(|d| d.0).collect();
    let data_name = "Data";
    let flags = 0x0003; // fRwGrand, fColGrand
    let mut out = rec(
        0x00B0,
        &[
            u16s(&[v.body[0], v.body[1], v.body[2], v.body[3]]),
            u16s(&[v.body[0] + 1, v.body[0] + 1 + v.cols.len() as u16, v.body[2] + v.rows.len().max(1) as u16, 0, 0]),
            u16s(&[if v.rows.contains(&VALUES) { 1 } else { 2 }, 0xFFFF, 3]),
            u16s(&[v.rows.len() as u16, v.cols.len() as u16, v.pages.len() as u16, v.data.len() as u16, 0, 0, flags, 1]),
            u16s(&[v.name.len() as u16, data_name.len() as u16]),
            nocch(&v.name),
            nocch(data_name),
        ]
        .concat(),
    );
    for field in [REGION, PRODUCT, SALES] {
        let mut axis = 0u16;
        if v.rows.contains(&field) {
            axis |= 1;
        }
        if v.cols.contains(&field) {
            axis |= 2;
        }
        if v.pages.iter().any(|p| p.0 == field) {
            axis |= 4;
        }
        if data_fields.contains(&field) {
            axis |= 8;
        }
        let on_axis = axis & 7 != 0;
        let its = if on_axis { items(field) } else { vec![] };
        let count = its.len() + usize::from(on_axis);
        out.extend(rec(0x00B1, &u16s(&[axis, u16::from(on_axis), u16::from(on_axis), count as u16, 0xFFFF])));
        for (k, _) in its.iter().enumerate() {
            out.extend(rec(0x00B2, &u16s(&[0, 0, k as u16, 0xFFFF])));
        }
        if on_axis {
            out.extend(rec(0x00B2, &u16s(&[1, 0, 0xFFFF, 0xFFFF])));
        }
        // SXVDEx: drag flags, AutoSort ascending by caption, no AutoShow.
        let ex_flags: u32 = 0x1E | (1 << 9) | (1 << 10) | (1 << 24);
        out.extend(rec(0x0100, &[&ex_flags.to_le_bytes()[..], &u16s(&[0xFFFF, 0xFFFF, 0, 0xFFFF]), &[0; 8]].concat()));
    }
    let ivd = |list: &[i16]| rec(0x00B4, &list.iter().flat_map(|i| i.to_le_bytes()).collect::<Vec<u8>>());
    if !v.rows.is_empty() {
        out.extend(ivd(&v.rows));
    }
    if !v.cols.is_empty() {
        out.extend(ivd(&v.cols));
    }
    if !v.pages.is_empty() {
        out.extend(rec(0x00B6, &v.pages.iter().flat_map(|(f, i)| u16s(&[*f as u16, *i as u16, 1])).collect::<Vec<u8>>()));
    }
    for (field, func, name) in &v.data {
        let (cch, st) = match name {
            Some(n) => (n.len() as u16, nocch(n)),
            None => (0xFFFF, vec![]),
        };
        out.extend(rec(0x00C5, &[u16s(&[*field as u16, *func, 0, 0, 0, 0, cch]), st].concat()));
    }
    out
}

/// The workbook: `views` on "Report", their stale report cells (`stale`, row/column/value), and
/// the cache stream as `cache` says.
pub fn workbook(views: &[View], stale: &[(u16, u16, f64)], cache: Cache) -> Vec<u8> {
    let mut sheet1 = bof(0x0010);
    for (c, h) in ["Region", "Product", "Sales"].iter().enumerate() {
        sheet1.extend(label(0, c as u16, h));
    }
    for (r, (region, product, sales)) in SOURCE.iter().enumerate() {
        let r = r as u16 + 1;
        sheet1.extend(label(r, 0, region));
        sheet1.extend(label(r, 1, product));
        sheet1.extend(number(r, 2, *sales));
    }
    sheet1.extend(eof());
    let mut report = bof(0x0010);
    for (r, c, n) in stale {
        report.extend(number(*r, *c, *n));
    }
    for v in views {
        report.extend(view_records(v));
    }
    report.extend(eof());

    let boundsheet = |pos: u32, name: &str| rec(0x0085, &[&pos.to_le_bytes()[..], &[0, 0, name.len() as u8, 0], name.as_bytes()].concat());
    // DConRef Sheet1!A1:C7: RefU, cchFile, then the self-reference "\x02Sheet1" and one unused byte.
    let dcon = rec(0x0051, &[u16s(&[0, 6]), vec![0, 2], u16s(&[7]), vec![0, 2], b"Sheet1".to_vec(), vec![0]].concat());
    let globals = |p1: u32, p2: u32| {
        [bof(0x0005), boundsheet(p1, "Sheet1"), boundsheet(p2, "Report"), rec(0x00D5, &u16s(&[1])), rec(0x00E3, &u16s(&[1])), dcon.clone(), eof()]
            .concat()
    };
    let g_len = globals(0, 0).len() as u32;
    let stream = [globals(g_len, g_len + sheet1.len() as u32), sheet1, report].concat();
    let cache_bytes = cache_stream();
    let mut streams: Vec<(&str, Vec<u8>)> = vec![("Workbook", stream)];
    match cache {
        Cache::Present => streams.push(("_SX_DB_CUR/0001", cache_bytes)),
        Cache::Truncated => streams.push(("_SX_DB_CUR/0001", cache_bytes[..cache_bytes.len() / 2 + 1].to_vec())),
        Cache::Bytes(b) => streams.push(("_SX_DB_CUR/0001", b)),
        Cache::Missing => {}
    }
    let refs: Vec<(&str, &[u8])> = streams.iter().map(|(n, d)| (*n, d.as_slice())).collect();
    super::compound_file(&refs)
}
