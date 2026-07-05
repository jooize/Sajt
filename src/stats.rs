//! Derived presentation data: the tag-cloud statistics, the grade thresholds
//! that drive both the quality meter and the level filter, the Finder-color
//! mapping, and the view filter (level / favorites / search) applied per request.

use crate::entry::Entry;
use chrono::NaiveDate;
use std::collections::BTreeMap;

/// Grade thresholds, kept in ONE place because they drive two things at once:
/// the meter ticks on each row and the level filter's cutoffs. `notable` is the
/// top 50%, `best` the top 22% (a grade is a percentile in `0.0..=1.0`).
pub const NOTABLE: f32 = 0.50;
pub const BEST: f32 = 0.78;

/// Quality tier of a grade: 1 = ungraded/ordinary, 2 = notable, 3 = best.
/// An ungraded entry (`None`) is tier 1 — it shows under "everything" only.
pub fn tier(grade: Option<f32>) -> u8 {
    match grade {
        Some(q) if q >= BEST => 3,
        Some(q) if q >= NOTABLE => 2,
        _ => 1,
    }
}

/// Map a Finder color index (0-7) to the CSS custom property that paints it.
/// 0 none and 1 gray both fall through to the neutral dot.
pub fn finder_color_var(color: u8) -> &'static str {
    match color {
        2 => "var(--tag-green)",
        3 => "var(--tag-purple)",
        4 => "var(--tag-blue)",
        5 => "var(--tag-yellow)",
        6 => "var(--tag-red)",
        7 => "var(--tag-orange)",
        _ => "var(--tag-gray)",
    }
}

/// One row of the tag cloud: a topic, how many entries carry it, when it was
/// last active, and the Finder color of its freshest use.
#[derive(Debug, Clone)]
pub struct TagStat {
    pub name: String,
    pub count: usize,
    pub last_active: NaiveDate,
    pub color: u8,
}

/// The cloud as a whole, alphabetical, plus the range needed to scale sizes and
/// judge recency.
#[derive(Debug, Clone, Default)]
pub struct CloudStats {
    pub tags: Vec<TagStat>,
    pub max_count: usize,
    /// The most recent activity across all topics — recency is judged against
    /// this, not the wall clock, so the cloud is deterministic.
    pub newest: Option<NaiveDate>,
}

/// Build the cloud from a set of entries. Only topical tags are counted;
/// machinery tags (`favorite`, `public`, `Do…`) never appear.
pub fn compute_cloud(entries: &[&Entry]) -> CloudStats {
    let mut by_name: BTreeMap<String, TagStat> = BTreeMap::new();
    for entry in entries {
        let date = entry.timestamp.date();
        for tag in entry.topical_tags() {
            let stat = by_name.entry(tag.name.clone()).or_insert(TagStat {
                name: tag.name.clone(),
                count: 0,
                last_active: date,
                color: tag.color,
            });
            stat.count += 1;
            // Color and last-active follow the freshest use of the tag.
            if date >= stat.last_active {
                stat.last_active = date;
                stat.color = tag.color;
            }
        }
    }

    let tags: Vec<TagStat> = by_name.into_values().collect();
    let max_count = tags.iter().map(|t| t.count).max().unwrap_or(0);
    let newest = tags.iter().map(|t| t.last_active).max();
    CloudStats { tags, max_count, newest }
}

impl CloudStats {
    /// Cloud font-size in rem: bigger for busier topics. Matches the mockup's
    /// `0.82 + share * 0.43` curve.
    pub fn size_rem(&self, count: usize) -> f32 {
        let share = if self.max_count > 1 {
            (count as f32 - 1.0) / (self.max_count as f32 - 1.0)
        } else {
            0.0
        };
        0.82 + share * 0.43
    }

    /// Recency ink for a topic: fresh topics are dark and bold, dormant ones
    /// fade. Returns `(css-color, font-weight)`, judged against `newest`.
    pub fn ink(&self, last_active: NaiveDate) -> (&'static str, u16) {
        let days = self
            .newest
            .map(|n| (n - last_active).num_days())
            .unwrap_or(0);
        if days <= 14 {
            ("var(--ink)", 650)
        } else if days <= 60 {
            ("var(--soft)", 550)
        } else {
            ("var(--faint)", 500)
        }
    }
}

/// The transient view filter carried in the query string: the grade floor
/// (`?grade=notable|best`), favorites-only (`?favorites`), and a free-text
/// search (`?q=…`). Composes with the path filter (tags / date) that
/// `ContentQuery` already handles.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ViewFilter {
    /// Grade floor: 0 everything, 1 notable, 2 best.
    pub level: u8,
    pub fav: bool,
    pub q: Option<String>,
}

impl ViewFilter {
    /// Parse from decoded query parameters (`grade`, `favorites`, `q`).
    pub fn from_params(grade: Option<&str>, fav: bool, q: Option<&str>) -> Self {
        let level = match grade.map(|s| s.to_ascii_lowercase()) {
            Some(ref s) if s == "best" || s == "2" => 2,
            Some(ref s) if s == "notable" || s == "1" => 1,
            _ => 0,
        };
        let q = q
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        ViewFilter { level, fav, q }
    }

    /// The grade floor as its URL/UI word.
    pub fn grade_word(&self) -> &'static str {
        match self.level {
            2 => "best",
            1 => "notable",
            _ => "everything",
        }
    }

    /// Does an entry pass the view filter?
    pub fn matches(&self, entry: &Entry) -> bool {
        if tier(entry.grade) < self.level + 1 {
            return false;
        }
        if self.fav && !entry.is_favorite() {
            return false;
        }
        if let Some(ref q) = self.q {
            let q = q.to_lowercase();
            if !self.haystack(entry).contains(&q) {
                return false;
            }
        }
        true
    }

    /// Everything a search query is matched against: label, tags, type, ext.
    fn haystack(&self, entry: &Entry) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(l) = entry.display_label.as_deref().or(entry.label.as_deref()) {
            parts.push(l.to_string());
        }
        parts.push(entry.kind().to_string());
        parts.push(entry.extension.clone());
        for t in entry.topical_tags() {
            parts.push(t.name.clone());
        }
        parts.join(" ").to_lowercase()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers() {
        assert_eq!(tier(None), 1);
        assert_eq!(tier(Some(0.1)), 1);
        assert_eq!(tier(Some(0.5)), 2);
        assert_eq!(tier(Some(0.77)), 2);
        assert_eq!(tier(Some(0.78)), 3);
        assert_eq!(tier(Some(1.0)), 3);
    }

    #[test]
    fn level_parsing() {
        assert_eq!(ViewFilter::from_params(Some("best"), false, None).level, 2);
        assert_eq!(ViewFilter::from_params(Some("notable"), false, None).level, 1);
        assert_eq!(ViewFilter::from_params(Some("everything"), false, None).level, 0);
        assert_eq!(ViewFilter::from_params(None, false, None).level, 0);
        assert_eq!(ViewFilter::from_params(Some("garbage"), false, None).level, 0);
    }

    #[test]
    fn empty_query_is_none() {
        assert_eq!(ViewFilter::from_params(None, false, Some("   ")).q, None);
        assert_eq!(
            ViewFilter::from_params(None, false, Some(" hi ")).q,
            Some("hi".to_string())
        );
    }

    #[test]
    fn color_mapping() {
        assert_eq!(finder_color_var(0), "var(--tag-gray)");
        assert_eq!(finder_color_var(3), "var(--tag-purple)");
        assert_eq!(finder_color_var(7), "var(--tag-orange)");
        assert_eq!(finder_color_var(99), "var(--tag-gray)");
    }
}
