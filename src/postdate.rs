//! Precision-aware publish timestamps.
//!
//! A post's publish date is not always a full instant. An mtime is second-exact,
//! but a date marker or a date-named post may be a day, a month, a bare year — or
//! a year BCE. `PostDate` carries the moment *and its precision* so formatting,
//! URLs, timeline grouping, and future-hold all respect how much the author
//! actually specified. See `post-model.md` §2 and the [review] amendment.
//!
//! The year is **signed and literal**: `-3000` means 3000 BCE (not the proleptic
//! astronomical year with its off-by-one), matching the URL grammar `/-3000/…`.

use chrono::{Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike};

/// How much of a `PostDate` the author actually specified. Ordering is
/// coarse→fine, used only as a last tiebreak in `Ord`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Precision {
    Year,
    Month,
    Day,
    Minute,
    Second,
}

/// A publish timestamp with precision and BCE support. Lower-than-precision
/// fields are normalized to their minimum (month/day = 1, time = 0), so the
/// struct also sorts correctly as a plain field tuple (year first — signed, so
/// BCE sorts before CE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PostDate {
    year: i32,
    month: u8,
    day: u8,
    hour: u8,
    minute: u8,
    second: u8,
    pub precision: Precision,
}

impl PostDate {
    /// A second-precise date from a file's mtime (the common case).
    pub fn from_mtime(dt: NaiveDateTime) -> Self {
        PostDate {
            year: dt.year(),
            month: dt.month() as u8,
            day: dt.day() as u8,
            hour: dt.hour() as u8,
            minute: dt.minute() as u8,
            second: dt.second() as u8,
            precision: Precision::Second,
        }
    }

    /// The current wall-clock moment, second-precise (used for future-hold).
    pub fn now() -> Self {
        Self::from_mtime(Local::now().naive_local())
    }

    /// Whether this post is scheduled for the future (held until its moment).
    pub fn is_future(&self, now: &PostDate) -> bool {
        self > now
    }

    /// The starting instant of this date as a local `NaiveDateTime`, for
    /// scheduling the future-hold wake timer. `None` for BCE / out-of-range
    /// years (which are never in the future anyway).
    pub fn to_local_instant(&self) -> Option<NaiveDateTime> {
        if self.year < 1 {
            return None;
        }
        let date = NaiveDate::from_ymd_opt(self.year, self.month as u32, self.day as u32)?;
        let time = NaiveTime::from_hms_opt(self.hour as u32, self.minute as u32, self.second as u32)?;
        Some(NaiveDateTime::new(date, time))
    }

    /// A `NaiveDate` for tag-cloud recency. Best-effort (BCE folds onto chrono's
    /// proleptic year, which is irrelevant to "most recent"); falls back to the
    /// epoch date if the components are somehow invalid.
    pub fn to_naive_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year, self.month as u32, self.day as u32)
            .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
    }

    /// `HHMMSS` — the time-of-day disambiguator carried as a URL path segment for
    /// same-day slug collisions and unlabeled entries.
    pub fn hms(&self) -> String {
        format!("{:02}{:02}{:02}", self.hour, self.minute, self.second)
    }

    /// Whether this date carries a real time-of-day (minute/second precision).
    pub fn has_time(&self) -> bool {
        matches!(self.precision, Precision::Minute | Precision::Second)
    }

    /// The `HHMMSS` disambiguator path segment, but only when a real time exists;
    /// a day/month/year-precision post has no time segment to append.
    pub fn time_seg(&self) -> Option<String> {
        self.has_time().then(|| self.hms())
    }

    /// The canonical date path prefix, respecting precision:
    /// year → `/2026` (or `/-3000`), month → `/2026/07`, day+ → `/2026/07/04`.
    pub fn date_path(&self) -> String {
        match self.precision {
            Precision::Year => format!("/{}", self.year),
            Precision::Month => format!("/{}/{:02}", self.year, self.month),
            _ => format!("/{}/{:02}/{:02}", self.year, self.month, self.day),
        }
    }

    /// The `/YYYY/MM` (or `/YYYY` for year precision) link a month heading points
    /// at — the timeline scope for this post's group.
    pub fn group_path(&self) -> String {
        match self.precision {
            Precision::Year => format!("/{}", self.year),
            _ => format!("/{}/{:02}", self.year, self.month),
        }
    }

    /// The timeline group label: "March 2026" for dated posts, the bare year (or
    /// "3000 BCE") for year-precision posts.
    pub fn group_label(&self) -> String {
        match self.precision {
            Precision::Year => self.year_label(),
            _ => format!("{} {}", month_name(self.month), self.display_year()),
        }
    }

    /// The `<time datetime="…">` machine attribute, at this date's precision.
    pub fn iso_attr(&self) -> String {
        match self.precision {
            Precision::Year => format!("{}", self.year),
            Precision::Month => format!("{}-{:02}", self.year, self.month),
            Precision::Day => format!("{}-{:02}-{:02}", self.year, self.month, self.day),
            Precision::Minute => format!(
                "{}-{:02}-{:02}T{:02}:{:02}",
                self.year, self.month, self.day, self.hour, self.minute
            ),
            Precision::Second => format!(
                "{}-{:02}-{:02}T{:02}:{:02}:{:02}",
                self.year, self.month, self.day, self.hour, self.minute, self.second
            ),
        }
    }

    /// The compact date shown in a timeline row / continue teaser, at precision:
    /// "2026-07-04", "2026-07", "2026" / "3000 BCE".
    pub fn short_date(&self) -> String {
        match self.precision {
            Precision::Year => self.year_label(),
            Precision::Month => format!("{}-{:02}", self.year, self.month),
            _ => format!("{}-{:02}-{:02}", self.year, self.month, self.day),
        }
    }

    /// The date shown in a post header — like `short_date` but carrying the time
    /// when the author gave one (minute/second precision).
    pub fn long_date(&self) -> String {
        match self.precision {
            Precision::Minute | Precision::Second => format!(
                "{}-{:02}-{:02} {:02}:{:02}",
                self.year, self.month, self.day, self.hour, self.minute
            ),
            _ => self.short_date(),
        }
    }

    /// A filesystem-safe stamp for a raw download filename fallback (unlabeled
    /// posts). Always second-shaped so two same-second posts still differ by
    /// nothing worse than the address already does.
    pub fn file_stamp(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// The full padded `YYYY-MM-DDTHHMMSS` used for URL date-prefix matching
    /// (`starts_with` a `/2026/07`-style prefix). Lower-precision fields read as
    /// their minimum, so a year-only post files at the start of its year.
    pub fn match_string(&self) -> String {
        format!(
            "{}-{:02}-{:02}T{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    fn display_year(&self) -> String {
        if self.year < 0 {
            format!("{} BCE", -self.year)
        } else {
            format!("{}", self.year)
        }
    }

    fn year_label(&self) -> String {
        self.display_year()
    }

    /// Parse `[-]YYYY[-MM[-DD[Thhmm[ss]]]]` with an optional trailing zone
    /// (`Z` or `±HHMM`, only after a time). Returns `None` for anything that is
    /// not such a date. A zoned datetime is normalized to site-local; a bare one
    /// is already local. This is the one grammar for both empty date-marker
    /// folders and date-named posts.
    pub fn parse(s: &str) -> Option<PostDate> {
        let (neg, rest) = match s.strip_prefix('-') {
            Some(r) => (true, r),
            None => (false, s),
        };
        let (date_part, time_part) = match rest.split_once('T') {
            Some((d, t)) => (d, Some(t)),
            None => (rest, None),
        };

        let dcomps: Vec<&str> = date_part.split('-').collect();
        if dcomps.is_empty() || dcomps.len() > 3 {
            return None;
        }
        // Year: 4 digits (so it reads as a date, not a plain small number).
        if dcomps[0].len() != 4 || !dcomps[0].bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let year_abs: i32 = dcomps[0].parse().ok()?;
        let year = if neg { -year_abs } else { year_abs };

        let mut month = 1u8;
        let mut day = 1u8;
        let mut precision = Precision::Year;
        if dcomps.len() >= 2 {
            month = parse_two(dcomps[1])?;
            if !(1..=12).contains(&month) {
                return None;
            }
            precision = Precision::Month;
        }
        if dcomps.len() >= 3 {
            day = parse_two(dcomps[2])?;
            if !(1..=31).contains(&day) {
                return None;
            }
            precision = Precision::Day;
        }

        let mut hour = 0u8;
        let mut minute = 0u8;
        let mut second = 0u8;
        if let Some(tp) = time_part {
            // A time is only meaningful on a full date.
            if precision != Precision::Day {
                return None;
            }
            let (hms, offset) = split_zone(tp)?;
            if (hms.len() != 4 && hms.len() != 6) || !hms.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            hour = hms[0..2].parse().ok()?;
            minute = hms[2..4].parse().ok()?;
            precision = Precision::Minute;
            if hms.len() == 6 {
                second = hms[4..6].parse().ok()?;
                precision = Precision::Second;
            }
            if hour > 23 || minute > 59 || second > 59 {
                return None;
            }
            // A zoned time resolves against a real (CE) calendar date, then
            // normalizes to local. BCE + zone is not meaningful — keep as given.
            if let Some(off) = offset {
                if let Some(local) = zoned_to_local(year, month, day, hour, minute, second, off) {
                    return Some(local.at_precision(precision));
                }
            }
        }

        // Validate the calendar date for the common CE range (reject 2026-02-30).
        if year >= 1 && precision >= Precision::Day && NaiveDate::from_ymd_opt(year, month as u32, day as u32).is_none() {
            return None;
        }

        Some(PostDate { year, month, day, hour, minute, second, precision })
    }

    /// Re-stamp a second-precise local instant with a coarser precision (used
    /// after zone conversion, which always yields a full instant).
    fn at_precision(mut self, precision: Precision) -> Self {
        self.precision = precision;
        self
    }
}

/// Parse a zero-padded two-digit field.
fn parse_two(s: &str) -> Option<u8> {
    if s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse().ok()
    } else {
        None
    }
}

/// Split an optional trailing timezone (`Z` or `±HHMM`) off the time part of a
/// marker. The zone sign sits after the time, never at the start, so it can't be
/// confused with the BCE year sign (already stripped before the `T`).
fn split_zone(tp: &str) -> Option<(&str, Option<FixedOffset>)> {
    if let Some(body) = tp.strip_suffix('Z') {
        return Some((body, Some(FixedOffset::east_opt(0)?)));
    }
    if tp.len() >= 5 {
        let bytes = tp.as_bytes();
        let sign_idx = tp.len() - 5;
        if bytes[sign_idx] == b'+' || bytes[sign_idx] == b'-' {
            let digits = &tp[sign_idx + 1..];
            if digits.bytes().all(|b| b.is_ascii_digit()) {
                let h: i32 = tp[sign_idx + 1..sign_idx + 3].parse().ok()?;
                let m: i32 = tp[sign_idx + 3..sign_idx + 5].parse().ok()?;
                if h <= 23 && m <= 59 {
                    let secs = h * 3600 + m * 60;
                    let off = if bytes[sign_idx] == b'+' {
                        FixedOffset::east_opt(secs)
                    } else {
                        FixedOffset::west_opt(secs)
                    }?;
                    return Some((&tp[..sign_idx], Some(off)));
                }
            }
        }
    }
    Some((tp, None))
}

/// Resolve a zoned wall-clock to site-local, returning a second-precise
/// `PostDate` (the caller re-stamps the intended precision).
fn zoned_to_local(y: i32, mo: u8, d: u8, h: u8, mi: u8, s: u8, off: FixedOffset) -> Option<PostDate> {
    let date = NaiveDate::from_ymd_opt(y, mo as u32, d as u32)?;
    let time = NaiveTime::from_hms_opt(h as u32, mi as u32, s as u32)?;
    let naive = NaiveDateTime::new(date, time);
    let local = off.from_local_datetime(&naive).single()?.with_timezone(&Local).naive_local();
    Some(PostDate::from_mtime(local))
}

/// Full month name for 1..=12; echoes a fallback for anything else.
fn month_name(m: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August",
        "September", "October", "November", "December",
    ];
    NAMES.get((m as usize).wrapping_sub(1)).copied().unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pd(s: &str) -> PostDate {
        PostDate::parse(s).unwrap_or_else(|| panic!("failed to parse {s:?}"))
    }

    #[test]
    fn parse_precisions() {
        assert_eq!(pd("2026").precision, Precision::Year);
        assert_eq!(pd("2026-07").precision, Precision::Month);
        assert_eq!(pd("2026-07-04").precision, Precision::Day);
        assert_eq!(pd("2026-07-04T1914").precision, Precision::Minute);
        assert_eq!(pd("2026-07-04T191430").precision, Precision::Second);
    }

    #[test]
    fn parse_bce() {
        let d = pd("-3000");
        assert_eq!(d.precision, Precision::Year);
        assert_eq!(d.date_path(), "/-3000");
        assert_eq!(d.group_label(), "3000 BCE");
    }

    #[test]
    fn parse_zoned_normalizes() {
        assert!(PostDate::parse("2026-07-04T1914Z").is_some());
        assert!(PostDate::parse("2026-07-04T1914+0200").is_some());
        assert!(PostDate::parse("2026-07-04T1914-0500").is_some());
    }

    #[test]
    fn parse_rejects_non_dates() {
        assert!(PostDate::parse("hello-world").is_none());
        assert!(PostDate::parse("alias resume").is_none());
        assert!(PostDate::parse("2026-13-03").is_none()); // bad month
        assert!(PostDate::parse("2026-02-30").is_none()); // not a real day
        assert!(PostDate::parse("202").is_none()); // year must be 4 digits
        assert!(PostDate::parse("2026-07T1914").is_none()); // time needs full date
    }

    #[test]
    fn paths_respect_precision() {
        assert_eq!(pd("2026").date_path(), "/2026");
        assert_eq!(pd("2026-07").date_path(), "/2026/07");
        assert_eq!(pd("2026-07-04").date_path(), "/2026/07/04");
        assert_eq!(pd("2026-07-04T191430").hms(), "191430");
    }

    #[test]
    fn ordering_bce_before_ce_and_by_time() {
        assert!(pd("-3000") < pd("2026"));
        assert!(pd("2026-07-04T1914") < pd("2026-07-04T1915"));
        assert!(pd("2026-07") < pd("2026-08"));
    }

    #[test]
    fn future_hold() {
        let now = PostDate::from_mtime(
            NaiveDate::from_ymd_opt(2026, 7, 8).unwrap().and_hms_opt(12, 0, 0).unwrap(),
        );
        assert!(pd("2027").is_future(&now));
        assert!(!pd("2025").is_future(&now));
        assert!(pd("2026-07-09").is_future(&now));
    }
}
