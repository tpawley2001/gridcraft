//! NOW(), TODAY() and the other "current date/time" readers use the computer's local clock, as
//! Excel does, not UTC. Each parent test re-runs this test binary with `TZ` pinned for that one
//! process (never `set_var` here: other tests run in parallel) and requires its child to pass.

use std::process::Command;

const CHILD: &str = "GRIDCRAFT_LOCAL_CLOCK_CHILD";

fn is_child() -> bool {
    std::env::var_os(CHILD).is_some()
}

fn utc_serial() -> f64 {
    gridcraft_core::date::serial_from_unix_ms(gridcraft_core::date::unix_now_ms().expect("clock"))
}

/// Hours the local clock is behind UTC, from `now_serial()`.
fn hours_behind_utc() -> f64 {
    let utc = utc_serial();
    let local = gridcraft_calc::now_serial();
    (utc - local) * 24.0
}

fn check_child(expected_hours_behind: f64) {
    let behind = hours_behind_utc();
    assert!((behind - expected_hours_behind).abs() < 0.01, "now_serial is {behind} h behind UTC, expected {expected_hours_behind}");
    let ms = gridcraft_core::date::unix_now_ms().expect("clock");
    let offset_h = gridcraft_core::date::local_offset_ms(ms) / 3_600_000.0;
    assert!((offset_h + expected_hours_behind).abs() < 1e-9, "local_offset_ms is {offset_h} h");
    let local = gridcraft_core::date::local_now_serial().expect("clock");
    assert!((local - gridcraft_calc::now_serial()).abs() * 86_400.0 < 5.0, "now_serial is the local serial");
}

fn run_child(test: &str, tz: &str) {
    let out = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .env("TZ", tz)
        .output()
        .expect("run child");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "child {test} with TZ={tz} failed:\n{stdout}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("1 passed"), "child {test} ran:\n{stdout}");
}

#[test]
fn child_utc_minus_8() {
    if is_child() {
        check_child(8.0);
    }
}

#[test]
fn child_tokyo() {
    if is_child() {
        check_child(-9.0);
    }
}

#[test]
fn child_utc() {
    if is_child() {
        check_child(0.0);
    }
}

#[test]
#[cfg(unix)]
fn now_is_local_wall_clock_west_of_utc() {
    if !is_child() {
        run_child("child_utc_minus_8", "Etc/GMT+8");
    }
}

#[test]
#[cfg(unix)]
fn now_is_local_wall_clock_east_of_utc() {
    if !is_child() {
        run_child("child_tokyo", "Asia/Tokyo");
    }
}

#[test]
#[cfg(unix)]
fn now_is_utc_when_the_zone_is_utc() {
    if !is_child() {
        run_child("child_utc", "UTC");
    }
}
