//! A calendar day, for naming day notes and for searching by date.

use crate::edit::DateTime;

/// A day. Ordered by time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl std::fmt::Display for Date {
    /// `2026-11-12`.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in(year: i32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 31,
    }
}

impl Date {
    /// The day of `when`.
    pub fn of(when: &DateTime) -> Date {
        Date { year: when.year, month: when.month, day: when.day }
    }

    /// `2026-11-12` (also at the start of longer text, as in a file name). None for anything that is not a real day.
    pub fn parse(text: &str) -> Option<Date> {
        let t = text.get(..10)?;
        let mut parts = t.split('-');
        let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
        if y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return None;
        }
        let date = Date { year: y.parse().ok()?, month: m.parse().ok()?, day: d.parse().ok()? };
        ((1..=12).contains(&date.month) && (1..=days_in(date.year, date.month)).contains(&date.day)).then_some(date)
    }

    /// The first and last day of `2026-11`, or the one day of `2026-11-12`, or the year of `2026`.
    pub fn span(text: &str) -> Option<(Date, Date)> {
        if let Some(day) = Date::parse(text).filter(|_| text.len() == 10) {
            return Some((day, day));
        }
        let mut parts = text.split('-');
        let year: i32 = parts.next().filter(|y| y.len() == 4)?.parse().ok()?;
        match (parts.next(), parts.next()) {
            (None, _) => Some((Date { year, month: 1, day: 1 }, Date { year, month: 12, day: 31 })),
            (Some(m), None) if m.len() == 2 => {
                let month: u32 = m.parse().ok().filter(|m| (1..=12).contains(m))?;
                Some((Date { year, month, day: 1 }, Date { year, month, day: days_in(year, month) }))
            }
            _ => None,
        }
    }

    /// The day `n` days before this one.
    #[must_use]
    pub fn days_back(self, n: u32) -> Date {
        let mut d = self;
        for _ in 0..n {
            if d.day > 1 {
                d.day -= 1;
            } else {
                (d.year, d.month) = if d.month > 1 { (d.year, d.month - 1) } else { (d.year - 1, 12) };
                d.day = days_in(d.year, d.month);
            }
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(year: i32, month: u32, day: u32) -> Date {
        Date { year, month, day }
    }

    #[test]
    fn a_day_reads_and_writes_as_iso() {
        assert_eq!(d(2026, 3, 7).to_string(), "2026-03-07");
        assert_eq!(Date::parse("2026-03-07"), Some(d(2026, 3, 7)));
        assert_eq!(Date::parse("2026-03-07.md"), Some(d(2026, 3, 7)), "a file name starts with its day");
        for bad in ["2026-13-01", "2026-02-30", "2026-3-7", "tomorrow", "", "2026-00-10"] {
            assert_eq!(Date::parse(bad), None, "{bad}");
        }
        assert_eq!(Date::parse("2024-02-29"), Some(d(2024, 2, 29)), "a leap day is real");
        assert_eq!(Date::parse("2025-02-29"), None);
    }

    #[test]
    fn a_month_or_a_year_is_a_span_of_days() {
        assert_eq!(Date::span("2026-11-12"), Some((d(2026, 11, 12), d(2026, 11, 12))));
        assert_eq!(Date::span("2026-02"), Some((d(2026, 2, 1), d(2026, 2, 28))));
        assert_eq!(Date::span("2026"), Some((d(2026, 1, 1), d(2026, 12, 31))));
        assert_eq!(Date::span("2026-14"), None);
        assert_eq!(Date::span("soon"), None);
    }

    #[test]
    fn going_back_crosses_months_and_years() {
        assert_eq!(d(2026, 11, 12).days_back(6), d(2026, 11, 6));
        assert_eq!(d(2026, 3, 2).days_back(3), d(2026, 2, 27));
        assert_eq!(d(2026, 1, 1).days_back(1), d(2025, 12, 31));
        assert!(d(2025, 12, 31) < d(2026, 1, 1));
    }
}
