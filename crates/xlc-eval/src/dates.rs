//! Excel date serials, including the 1900 leap-year bug (§8.4).
//!
//! In the 1900 date system, serial 1 = 1900-01-01, and serial 60 is the
//! nonexistent 1900-02-29 (a Lotus 1-2-3 compatibility bug Excel keeps
//! forever). Consequences we must reproduce:
//!   - serials 1..=59 map to 1900-01-01..1900-02-28
//!   - serial 60 is fictitious; DAY(60)=29, MONTH(60)=2, YEAR(60)=1900
//!   - serials >= 61 are offset by one day vs. the proleptic calendar
//!   - DATE(1900,3,1) = 61
//!
//! The 1904 system (workbook flag) has no bug: serial 0 = 1904-01-01.

/// Days in month, honoring real Gregorian leap years.
fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Days from 1899-12-31 to y-m-d (proleptic Gregorian), for y >= 1900.
fn days_from_epoch(y: i32, m: u32, d: u32) -> i64 {
    let mut days: i64 = 0;
    for yy in 1900..y {
        days += if is_leap(yy) { 366 } else { 365 };
    }
    for mm in 1..m {
        days += days_in_month(y, mm) as i64;
    }
    days + d as i64
}

/// (year, month, day) → 1900-system serial. Excel's DATE() also accepts
/// out-of-range months/days by rolling over (DATE(2020,13,1) =
/// DATE(2021,1,1)); that normalization happens here.
pub fn ymd_to_serial_1900(mut y: i32, mut m: i32, d: i32) -> i64 {
    // Roll months into years.
    y += (m - 1).div_euclid(12);
    m = (m - 1).rem_euclid(12) + 1;
    // Roll days via day-count arithmetic from the 1st of the month.
    let base = days_from_epoch(y, m as u32, 1);
    let mut serial = base + (d as i64 - 1);
    // The phantom Feb 29 1900: every real date from 1900-03-01 on is one
    // serial later than the raw proleptic count.
    if serial >= 60 {
        serial += 1;
    }
    serial
}

/// 1900-system serial → (year, month, day). Serial 60 returns the
/// fictitious (1900, 2, 29). Serial 0 returns (1900, 1, 0) — Excel's
/// "January 0, 1900" for time-only values.
pub fn serial_to_ymd_1900(serial: i64) -> (i32, u32, u32) {
    if serial == 60 {
        return (1900, 2, 29);
    }
    if serial == 0 {
        return (1900, 1, 0);
    }
    // Undo the phantom day for post-bug serials.
    let mut days = if serial > 60 { serial - 1 } else { serial };
    let mut y = 1900;
    loop {
        let ylen = if is_leap(y) { 366 } else { 365 };
        if days > ylen {
            days -= ylen;
            y += 1;
        } else {
            break;
        }
    }
    let mut m = 1;
    while days > days_in_month(y, m) as i64 {
        days -= days_in_month(y, m) as i64;
        m += 1;
    }
    (y, m, days as u32)
}

/// 1904-system serial → 1900-system serial (for normalizing workbooks
/// saved with the Mac epoch): the offset is 1462 days.
pub fn serial_1904_to_1900(serial: i64) -> i64 {
    serial + 1462
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_and_bug_anchor_points() {
        assert_eq!(ymd_to_serial_1900(1900, 1, 1), 1);
        assert_eq!(ymd_to_serial_1900(1900, 2, 28), 59);
        // The bug: March 1, 1900 is 61, leaving 60 for the phantom Feb 29.
        assert_eq!(ymd_to_serial_1900(1900, 3, 1), 61);
        assert_eq!(serial_to_ymd_1900(60), (1900, 2, 29));
        assert_eq!(serial_to_ymd_1900(59), (1900, 2, 28));
        assert_eq!(serial_to_ymd_1900(61), (1900, 3, 1));
    }

    #[test]
    fn known_modern_serials() {
        // Verified against Excel: 2020-01-01 = 43831, 2026-08-08 = 46242.
        assert_eq!(ymd_to_serial_1900(2020, 1, 1), 43831);
        assert_eq!(serial_to_ymd_1900(43831), (2020, 1, 1));
        assert_eq!(ymd_to_serial_1900(2026, 8, 8), 46242);
        // 1999-12-31 = 36525; 2000 WAS a leap year (div-400 rule).
        assert_eq!(ymd_to_serial_1900(1999, 12, 31), 36525);
        assert_eq!(ymd_to_serial_1900(2000, 2, 29), 36585);
    }

    #[test]
    fn date_rollover() {
        // DATE(2020,13,1) = DATE(2021,1,1); DATE(2020,1,32) = DATE(2020,2,1).
        assert_eq!(
            ymd_to_serial_1900(2020, 13, 1),
            ymd_to_serial_1900(2021, 1, 1)
        );
        assert_eq!(
            ymd_to_serial_1900(2020, 1, 32),
            ymd_to_serial_1900(2020, 2, 1)
        );
        // DATE(2020,0,5) = DATE(2019,12,5).
        assert_eq!(
            ymd_to_serial_1900(2020, 0, 5),
            ymd_to_serial_1900(2019, 12, 5)
        );
    }

    #[test]
    fn roundtrip_sweep() {
        // Every serial 1..80000 (through year 2119) round-trips, except the
        // phantom 60 which maps to the fictitious date by design.
        for s in 1i64..80_000 {
            let (y, m, d) = serial_to_ymd_1900(s);
            if s == 60 {
                assert_eq!((y, m, d), (1900, 2, 29));
                continue;
            }
            assert_eq!(ymd_to_serial_1900(y, m as i32, d as i32), s, "serial {s}");
        }
    }

    #[test]
    fn epoch_1904_offset() {
        // 1904 serial 0 = 1904-01-01 = 1900-system 1462.
        assert_eq!(serial_to_ymd_1900(serial_1904_to_1900(0)), (1904, 1, 1));
    }
}
