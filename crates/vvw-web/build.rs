#![allow(clippy::unreadable_literal)]

fn main() {
    // No rerun-if-changed lines — cargo reruns unconditionally,
    // giving every build a fresh timestamp.

    let datetime = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |_| "unknown".to_string(),
            |d| {
                let secs = d.as_secs();
                let days = secs / 86400;
                let time_of_day = secs % 86400;
                let hours = time_of_day / 3600;
                let minutes = (time_of_day % 3600) / 60;

                // Days since Unix epoch to Y-M-D (civil calendar)
                // Algorithm from Howard Hinnant (public domain)
                let z = days as i64 + 719_468;
                let era = z.div_euclid(146_097);
                let doe = z.rem_euclid(146_097) as u64;
                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
                let y = (yoe as i64) + era * 400;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp = (5 * doy + 2) / 153;
                let d = doy - (153 * mp + 2) / 5 + 1;
                let m = if mp < 10 { mp + 3 } else { mp - 9 };
                let y = if m <= 2 { y + 1 } else { y };

                format!("{y:04}-{m:02}-{d:02} {hours:02}:{minutes:02} UTC")
            },
        );

    println!("cargo:rustc-env=VVW_BUILD_DATETIME={datetime}");
}
