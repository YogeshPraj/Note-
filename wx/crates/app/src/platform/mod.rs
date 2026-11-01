//! Everything that differs by operating system, behind one trait.
//!
//! The rest of the shell calls `platform::current()` and never mentions an OS. Adding a platform means
//! one new implementation of [`Platform`]; callers do not change (Liskov / dependency inversion).

use std::path::Path;

use notepp_core::edit::{self, DateTime};

#[cfg(windows)]
mod windows;
#[cfg(unix)]
mod unix;

/// Operating-system services the shell needs. Window handles are raw native handles (`0` = none).
pub trait Platform: Sync {
    /// Show `path` selected in the system file manager.
    fn reveal_in_file_manager(&self, path: &Path);
    /// Open a terminal, in `dir` when given.
    fn open_terminal(&self, dir: Option<&Path>);
    /// Open a file or address with the program the system associates with it.
    fn open_default(&self, target: &str);
    /// Open `path` with a named program (a browser).
    fn open_with(&self, program: &str, path: &Path);
    /// Move a file to the trash / Recycle Bin.
    fn recycle(&self, path: &Path) -> Result<(), String> {
        let _ = path;
        Err("Moving files to the trash is not available here yet".into())
    }

    fn local_time(&self) -> DateTime {
        utc_now()
    }
    /// Time then date, in the user's regional format when the OS can provide it.
    fn date_time_text(&self, long: bool) -> String {
        edit::format_date_time(if long { "HH:mm:ss dddd, MMMM dd, yyyy" } else { "HH:mm yyyy-MM-dd" }, &self.local_time())
    }
}

/// The implementation for the OS this build targets.
pub fn current() -> &'static dyn Platform {
    #[cfg(windows)]
    {
        &windows::Windows
    }
    #[cfg(unix)]
    {
        &unix::Unix
    }
}

/// Civil date and time in UTC, for platforms without a time-zone API of their own.
pub(crate) fn utc_now() -> DateTime {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
    DateTime { year, month, day, weekday: ((days + 4).rem_euclid(7)) as u32, hour: (rem / 3600) as u32, minute: (rem % 3600 / 60) as u32, second: (rem % 60) as u32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_fields_are_in_range() {
        let t = utc_now();
        assert!(t.year >= 2024 && (1..=12).contains(&t.month) && (1..=31).contains(&t.day));
        assert!(t.weekday < 7 && t.hour < 24 && t.minute < 60 && t.second < 60);
    }

    #[test]
    fn date_time_text_is_not_empty_and_uses_local_time() {
        assert!(!current().date_time_text(false).trim().is_empty());
        assert!(!current().date_time_text(true).trim().is_empty());
    }

    #[test]
    fn defaults_are_safe_no_ops() {
        struct Bare;
        impl Platform for Bare {
            fn reveal_in_file_manager(&self, _: &Path) {}
            fn open_terminal(&self, _: Option<&Path>) {}
            fn open_default(&self, _: &str) {}
            fn open_with(&self, _: &str, _: &Path) {}
        }
        let p = Bare;
        assert!(p.recycle(Path::new("x")).is_err());
    }
}