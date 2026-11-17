//! Calendar conversion and stamp formatting.
//!
//! Data files use compact stamps so that paths stay short: `20261031` for a
//! date and `20261031-134502` for a date and time. The offset comes from the
//! workspace declaration instead of the host, so two machines agree.

/// Seconds since the Unix epoch.
pub fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

/// Shift an epoch to local seconds with a fixed offset.
fn local(epoch: u64, offset_hours: i32) -> i64 {
    epoch as i64 + i64::from(offset_hours) * 3600
}

/// Convert a shifted epoch into a civil date.
fn civil(seconds: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let hour = (rest / 3600) as u32;
    let minute = ((rest % 3600) / 60) as u32;
    let second = (rest % 60) as u32;

    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day, hour, minute, second)
}

/// Compact date, for example `20261031`.
pub fn date(epoch: u64, offset_hours: i32) -> String {
    let (year, month, day, _, _, _) = civil(local(epoch, offset_hours));
    format!("{year:04}{month:02}{day:02}")
}

/// Compact date and time, for example `20261031-134502`.
pub fn stamp(epoch: u64, offset_hours: i32) -> String {
    let (year, month, day, hour, minute, second) = civil(local(epoch, offset_hours));
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

/// Whether the text is a compact date or an ISO date.
pub fn is_date(text: &str) -> bool {
    match text.len() {
        8 => text.chars().all(|character| character.is_ascii_digit()),
        10 => {
            let bytes = text.as_bytes();
            bytes[4] == b'-'
                && bytes[7] == b'-'
                && bytes
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
        }
        _ => false,
    }
}

/// Whether the text is a compact stamp.
pub fn is_stamp(text: &str) -> bool {
    match text.len() {
        15 => {
            let bytes = text.as_bytes();
            bytes[8] == b'-'
                && bytes
                    .iter()
                    .enumerate()
                    .all(|(index, byte)| index == 8 || byte.is_ascii_digit())
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instant() {
        // 2026-10-31T05:00:00Z with a +8 offset is 20261031-130000.
        assert_eq!(stamp(1_793_422_800, 8), "20261031-130000");
        assert_eq!(date(1_793_422_800, 8), "20261031");
    }

    #[test]
    fn recognises_shapes() {
        assert!(is_date("20261031"));
        assert!(is_date("2026-10-31"));
        assert!(!is_date("2026/10/31"));
        assert!(is_stamp("20261031-134502"));
        assert!(!is_stamp("20261031"));
    }
}
