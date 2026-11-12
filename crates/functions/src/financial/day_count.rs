use super::{
    Arg, CellError, Ctx, DateSystem, R, Value, datetime_from_serial, days_in_month, is_leap, max_serial, num, num_val, opt_num, serial_from_ymd,
};

// ---------------------------------------------------------------------------------------------
// Dates and day counts
// ---------------------------------------------------------------------------------------------

pub(super) fn date(a: &[Arg], i: usize) -> R<f64> {
    let d = num(a, i)?.trunc();
    if !(0.0..=2_958_465.0).contains(&d) {
        return Err(CellError::Num);
    }
    Ok(d)
}

pub(super) fn ymd(sys: DateSystem, s: f64) -> R<(i32, u32, u32)> {
    let dt = datetime_from_serial(sys, s.trunc()).ok_or(CellError::Num)?;
    Ok((dt.year, dt.month, dt.day.max(1)))
}

fn serial(sys: DateSystem, y: i32, m: u32, d: u32) -> R<f64> {
    serial_from_ymd(sys, y as i64, m as i64, d as i64).ok_or(CellError::Num)
}

pub(super) fn basis_arg(a: &[Arg], i: usize) -> R<u32> {
    let b = opt_num(a, i, 0.0)?.trunc();
    if !(0.0..=4.0).contains(&b) {
        return Err(CellError::Num);
    }
    Ok(b as u32)
}

pub(super) fn freq_arg(a: &[Arg], i: usize) -> R<f64> {
    let f = num(a, i)?.trunc();
    if f != 1.0 && f != 2.0 && f != 4.0 {
        return Err(CellError::Num);
    }
    Ok(f)
}

/// 30/360 day count (US/NASD when `us`, European otherwise).
fn days360(sys: DateSystem, s1: f64, s2: f64, us: bool) -> R<f64> {
    days360_with(sys, s1, s2, us, false)
}

/// 30/360 day count. With `d31_before_feb`, the US rule that turns an end day of 31 into 30 looks at the start day
/// as written, before an end-of-February start becomes the 30th (Excel's COUPDAYBS: 28 Feb to 31 Aug is 181 days).
fn days360_with(sys: DateSystem, s1: f64, s2: f64, us: bool, d31_before_feb: bool) -> R<f64> {
    let (y1, m1, mut d1) = ymd(sys, s1)?;
    let (y2, m2, mut d2) = ymd(sys, s2)?;
    if us {
        let feb_end1 = m1 == 2 && d1 >= days_in_month(y1, 2);
        let feb_end2 = m2 == 2 && d2 >= days_in_month(y2, 2);
        let d1_written = d1;
        if feb_end1 && feb_end2 {
            d2 = 30;
        }
        if feb_end1 {
            d1 = 30;
        }
        if d2 == 31 && (if d31_before_feb { d1_written } else { d1 }) >= 30 {
            d2 = 30;
        }
        if d1 == 31 {
            d1 = 30;
        }
    } else {
        d1 = d1.min(30);
        d2 = d2.min(30);
    }
    Ok((y2 - y1) as f64 * 360.0 + (m2 as f64 - m1 as f64) * 30.0 + (d2 as f64 - d1 as f64))
}

/// Days between two dates under a basis.
pub(super) fn day_count(sys: DateSystem, s1: f64, s2: f64, basis: u32) -> R<f64> {
    match basis {
        0 => days360(sys, s1, s2, true),
        4 => days360(sys, s1, s2, false),
        _ => Ok(s2 - s1),
    }
}

/// Year fraction between two dates under a basis (YEARFRAC semantics).
pub(super) fn year_frac(sys: DateSystem, s1: f64, s2: f64, basis: u32) -> R<f64> {
    let (s1, s2) = if s1 <= s2 { (s1, s2) } else { (s2, s1) };
    match basis {
        0 => Ok(days360(sys, s1, s2, true)? / 360.0),
        2 => Ok((s2 - s1) / 360.0),
        3 => Ok((s2 - s1) / 365.0),
        4 => Ok(days360(sys, s1, s2, false)? / 360.0),
        _ => {
            let (y1, m1, d1) = ymd(sys, s1)?;
            let (y2, m2, d2) = ymd(sys, s2)?;
            let days = s2 - s1;
            if y1 == y2 {
                return Ok(days / if is_leap(y1) { 366.0 } else { 365.0 });
            }
            let within_year = y2 == y1 + 1 && (m2 < m1 || (m2 == m1 && d2 <= d1));
            if within_year {
                let feb29_1 = is_leap(y1) && (m1 < 2 || (m1 == 2 && d1 <= 29));
                let feb29_2 = is_leap(y2) && (m2 > 2 || (m2 == 2 && d2 == 29));
                let denom = if feb29_1 || feb29_2 { 366.0 } else { 365.0 };
                return Ok(days / denom);
            }
            let start = serial(sys, y1, 1, 1)?;
            let end = serial(sys, y2 + 1, 1, 1).unwrap_or(max_serial(sys) as f64 + 1.0);
            let avg = (end - start) / (y2 - y1 + 1) as f64;
            Ok(days / avg)
        }
    }
}

/// Days in a year for discount securities.
pub(super) fn year_basis(sys: DateSystem, basis: u32, s: f64) -> R<f64> {
    Ok(match basis {
        0 | 2 | 4 => 360.0,
        3 => 365.0,
        _ => {
            if is_leap(ymd(sys, s)?.0) {
                366.0
            } else {
                365.0
            }
        }
    })
}

/// Date `months` months before/after an anchor date, keeping end-of-month alignment.
pub(super) fn shift_months(sys: DateSystem, anchor: (i32, u32, u32), months: i64, eom: bool) -> R<f64> {
    let (y, m, d) = anchor;
    let total = y as i64 * 12 + (m as i64 - 1) + months;
    let ny = total.div_euclid(12);
    let nm = (total.rem_euclid(12) + 1) as u32;
    if !(1900..=9999).contains(&ny) {
        return Err(CellError::Num);
    }
    let dim = days_in_month(ny as i32, nm);
    let nd = if eom { dim } else { d.min(dim) };
    serial(sys, ny as i32, nm, nd)
}

pub(super) struct Coupon {
    pub(super) pcd: f64,
    pub(super) ncd: f64,
    pub(super) num: f64,
    /// Coupon period length in actual days (basis 1 COUPDAYS).
    pub(super) actual_days: f64,
}

/// Date `months` months before/after `d`, on the month end when `d` is one, otherwise on `d`'s day (clamped).
fn step_keep_month_end(sys: DateSystem, d: (i32, u32, u32), months: i64) -> R<f64> {
    shift_months(sys, d, months, d.2 == days_in_month(d.0, d.1))
}

/// Actual length of the coupon period containing `settle` (basis 1 COUPDAYS), for a month-end maturity.
///
/// Measured in Excel: here the coupon dates are stepped back from maturity one period at a time, each step clamping
/// the day to the shorter month (so 31 Aug steps to 28 Feb and then 28 Aug). The period whose start is on or before
/// settlement ends at `n`, and its length runs from one period before `n` (on the month end when `n` is one) to `n`.
/// These stepped dates differ from the month-end dates COUPPCD/COUPNCD report only in the day of the month, so the
/// period is the one at index `k` (that of COUPPCD) or the next one.
fn month_end_actual_days(sys: DateSystem, settle: f64, anchor: (i32, u32, u32), step: i64, k: i64) -> R<f64> {
    // Smallest day the stepped dates can clamp to: 28 when February is on the cycle, else the shortest month's length.
    let mut floor = anchor.2;
    for j in 1..=(12 / step.max(1)) {
        let total = anchor.0 as i64 * 12 + (anchor.1 as i64 - 1) - j * step;
        let m = (total.rem_euclid(12) + 1) as u32;
        floor = floor.min(if m == 2 { 28 } else { days_in_month(2001, m) });
    }
    // Stepped date `j` periods before maturity: the anchor's month, day clamped by every month passed on the way.
    let stepped = |j: i64| -> R<(i32, u32, u32)> {
        let mut day = anchor.2;
        let mut out = anchor;
        for i in 1..=j {
            let total = anchor.0 as i64 * 12 + (anchor.1 as i64 - 1) - i * step;
            let y = total.div_euclid(12);
            if !(1900..=9999).contains(&y) {
                return Err(CellError::Num);
            }
            let m = (total.rem_euclid(12) + 1) as u32;
            if day > floor {
                day = day.min(days_in_month(y as i32, m));
            }
            out = (y as i32, m, day);
            if i < j && day <= floor {
                // The day can drop no further: jump straight to period `j`.
                let total = anchor.0 as i64 * 12 + (anchor.1 as i64 - 1) - j * step;
                let y = total.div_euclid(12);
                if !(1900..=9999).contains(&y) {
                    return Err(CellError::Num);
                }
                let m = (total.rem_euclid(12) + 1) as u32;
                return Ok((y as i32, m, day.min(days_in_month(y as i32, m))));
            }
        }
        Ok(out)
    };
    let to_serial = |d: (i32, u32, u32)| serial(sys, d.0, d.1, d.2);
    let end_index = if k > 1 && to_serial(stepped(k - 1)?)? <= settle { k - 2 } else { k - 1 };
    let end = stepped(end_index)?;
    Ok(to_serial(end)? - step_keep_month_end(sys, end, -step)?)
}

pub(super) fn coupons(sys: DateSystem, settle: f64, mat: f64, freq: f64) -> R<Coupon> {
    if settle >= mat {
        return Err(CellError::Num);
    }
    let anchor = ymd(sys, mat)?;
    let eom = anchor.2 == days_in_month(anchor.0, anchor.1);
    let step = (12.0 / freq) as i64;
    // Estimate the count from the span, then adjust.
    let mut k = (((mat - settle) / 365.25 * freq).floor() as i64).max(1);
    while k > 1 && shift_months(sys, anchor, -(k - 1) * step, eom)? <= settle {
        k -= 1;
    }
    let mut guard = 0;
    while shift_months(sys, anchor, -k * step, eom)? > settle {
        k += 1;
        guard += 1;
        if guard > 100_000 {
            return Err(CellError::Num);
        }
    }
    let pcd = shift_months(sys, anchor, -k * step, eom)?;
    let ncd = shift_months(sys, anchor, -(k - 1) * step, eom)?;
    let actual_days = if eom { month_end_actual_days(sys, settle, anchor, step, k)? } else { ncd - pcd };
    Ok(Coupon { pcd, ncd, num: k as f64, actual_days })
}

pub(super) fn coup_days(c: &Coupon, freq: f64, basis: u32) -> f64 {
    match basis {
        1 => c.actual_days,
        3 => 365.0 / freq,
        _ => 360.0 / freq,
    }
}

pub(super) fn coup_daybs(sys: DateSystem, c: &Coupon, settle: f64, basis: u32) -> R<f64> {
    if basis == 0 {
        return days360_with(sys, c.pcd, settle, true, true);
    }
    day_count(sys, c.pcd, settle, basis)
}

pub(super) fn coup_daysnc(sys: DateSystem, c: &Coupon, settle: f64, freq: f64, basis: u32) -> R<f64> {
    match basis {
        0 => Ok(coup_days(c, freq, basis) - coup_daybs(sys, c, settle, basis)?),
        // 30E/360 counts settlement to the next coupon date directly (Excel).
        4 => day_count(sys, settle, c.ncd, basis),
        _ => Ok(c.ncd - settle),
    }
}

/// settlement, maturity, frequency, basis at positions 0..=3 for the COUP* functions.
fn coup_args(a: &[Arg], sys: DateSystem) -> R<(f64, f64, f64, u32, Coupon)> {
    let (s, m, f, b) = (date(a, 0)?, date(a, 1)?, freq_arg(a, 2)?, basis_arg(a, 3)?);
    let c = coupons(sys, s, m, f)?;
    Ok((s, m, f, b, c))
}

pub(super) fn coupdaybs(a: &[Arg], cx: &mut dyn Ctx) -> R<Value> {
    let (s, _, _, b, c) = coup_args(a, cx.date_system())?;
    num_val(coup_daybs(cx.date_system(), &c, s, b)?)
}

pub(super) fn coupdays(a: &[Arg], cx: &mut dyn Ctx) -> R<Value> {
    let (_, _, f, b, c) = coup_args(a, cx.date_system())?;
    num_val(coup_days(&c, f, b))
}

pub(super) fn coupdaysnc(a: &[Arg], cx: &mut dyn Ctx) -> R<Value> {
    let (s, _, f, b, c) = coup_args(a, cx.date_system())?;
    num_val(coup_daysnc(cx.date_system(), &c, s, f, b)?)
}

pub(super) fn coupncd(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    num_val(coup_args(a, c.date_system())?.4.ncd)
}

pub(super) fn couppcd(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    num_val(coup_args(a, c.date_system())?.4.pcd)
}

pub(super) fn coupnum(a: &[Arg], c: &mut dyn Ctx) -> R<Value> {
    num_val(coup_args(a, c.date_system())?.4.num)
}
