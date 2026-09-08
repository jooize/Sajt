//! Grade derivation: turn the append-only pairwise-judgement ledger into a
//! per-post grade in `0.0..=1.0`, read-only and fully recomputable.
//!
//! The ledger is `.sajt-grade-judgements.jsonl` in the content root (plus any
//! `.sajt-grade-judgements*.jsonl` conflict copies iCloud may leave), written
//! by the author's grading tool — never by this server. Each line is one pairwise
//! judgement ("winner beat loser"). We union every ledger file, drop exact
//! duplicates, resolve names through the alias map, keep only judgements between
//! two *current* posts (dangling ones from deletions are skipped), and fit a
//! Bradley-Terry model to get each post's strength. The grade is then
//! `P(this post beats a uniformly random other graded post)`.
//!
//! Bradley-Terry is a batch maximum-likelihood fit (order-independent, so the
//! same ledger always yields the same grades, and deleting a judgement cleanly
//! un-counts it — unlike live Elo). A Bayesian anchor prior keeps every strength
//! finite even when deletions fragment the comparison graph, where a plain MLE
//! would diverge. Absent or empty ledger -> empty map -> every post is ungraded
//! and the site is simply "everything" + favorites, fully functional.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

/// One pairwise judgement as stored in the ledger: `winner` beat `loser`. `at`
/// is an RFC-3339 instant that both dates the judgement and, crucially,
/// distinguishes two genuine comparisons of the same pair from an accidental
/// duplicate (an iCloud conflict copy repeats the identical line, `at` and all).
#[derive(Debug, Clone, Deserialize)]
pub struct Judgement {
    pub winner: String,
    pub loser: String,
    #[serde(default)]
    pub at: Option<String>,
}

/// The ledger file name stem. The canonical file is `<STEM>.jsonl`; iCloud
/// conflict copies (`<STEM> 2.jsonl`) are merged in and de-duplicated.
const LEDGER_STEM: &str = ".sajt-grade-judgements";

/// Virtual pseudo-comparisons every post plays against a fixed-strength anchor:
/// `PRIOR` wins and `PRIOR` losses. This is the Bayesian prior — it pins the
/// otherwise arbitrary Bradley-Terry scale and keeps a post that only ever won
/// (or a post in a disconnected fragment) at a finite strength instead of ±∞.
/// Small so real judgements dominate as soon as there are a few.
const PRIOR: f64 = 1.0;

/// Load and merge every ledger file in the content root, newest-agnostic and
/// order-independent. Missing files or unreadable lines fail closed (skipped);
/// an absent ledger simply yields an empty list.
pub fn load_judgements(content_dir: &Path) -> Vec<Judgement> {
    let read_dir = match std::fs::read_dir(content_dir) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(),
    };

    // Dedup across all files by the full (winner, loser, at) triple so iCloud
    // conflict copies merge losslessly while genuine repeat comparisons (which
    // carry a distinct `at`) are all kept.
    let mut seen: std::collections::HashSet<(String, String, Option<String>)> =
        std::collections::HashSet::new();
    let mut out: Vec<Judgement> = Vec::new();

    for entry in read_dir.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if !is_ledger_name(&name) {
            continue;
        }
        let text = match std::fs::read_to_string(entry.path()) {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!("Could not read grade ledger {}: {}", name, e);
                continue;
            }
        };
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match serde_json::from_str::<Judgement>(line) {
                Ok(j) => {
                    let key = (j.winner.clone(), j.loser.clone(), j.at.clone());
                    if seen.insert(key) {
                        out.push(j);
                    }
                }
                Err(e) => {
                    tracing::warn!("Skipping malformed judgement in {} line {}: {}", name, n + 1, e);
                }
            }
        }
    }
    out
}

/// True for the canonical ledger file and its iCloud conflict copies:
/// `.sajt-grade-judgements.jsonl`, `.sajt-grade-judgements 2.jsonl`, …
fn is_ledger_name(name: &str) -> bool {
    name.starts_with(LEDGER_STEM) && name.ends_with(".jsonl")
}

/// Derive a grade in `0.0..=1.0` for every post that appears in the ledger.
///
/// `resolve` maps any name found in the ledger (a post label or one of its
/// `alias <name>/` addresses) to the canonical post label, or `None` if no such
/// post exists any more. Judgements touching a vanished post, or comparing a
/// post with itself, are skipped. Posts absent from the ledger are simply not in
/// the returned map (their grade stays `None`).
pub fn derive_grades<F>(judgements: &[Judgement], resolve: F) -> HashMap<String, f32>
where
    F: Fn(&str) -> Option<String>,
{
    // Collapse judgements onto canonical, currently-existing posts and index the
    // participants densely for the fit.
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut names: Vec<String> = Vec::new();
    // wins[i][j] = number of times post i beat post j.
    let mut wins: Vec<HashMap<usize, u32>> = Vec::new();

    let idx_of = |name: String, index: &mut HashMap<String, usize>, names: &mut Vec<String>, wins: &mut Vec<HashMap<usize, u32>>| -> usize {
        if let Some(&i) = index.get(&name) {
            i
        } else {
            let i = names.len();
            index.insert(name.clone(), i);
            names.push(name);
            wins.push(HashMap::new());
            i
        }
    };

    for j in judgements {
        let (Some(w), Some(l)) = (resolve(&j.winner), resolve(&j.loser)) else {
            continue; // dangling: one side no longer exists
        };
        if w == l {
            continue; // a post never grades against itself
        }
        let wi = idx_of(w, &mut index, &mut names, &mut wins);
        let li = idx_of(l, &mut index, &mut names, &mut wins);
        *wins[wi].entry(li).or_insert(0) += 1;
    }

    let n = names.len();
    if n == 0 {
        return HashMap::new();
    }

    // Total real wins per post, and total real games between each ordered pair.
    let mut win_total = vec![0u32; n];
    // games[i][j] = wins[i][j] + wins[j][i] (symmetric count of comparisons).
    let mut games: Vec<HashMap<usize, u32>> = vec![HashMap::new(); n];
    for i in 0..n {
        for (&j, &c) in &wins[i] {
            win_total[i] += c;
            *games[i].entry(j).or_insert(0) += c;
            *games[j].entry(i).or_insert(0) += c;
        }
    }

    // Bradley-Terry MM (minorization-maximization) iteration (Hunter 2004),
    // with each post also playing PRIOR wins + PRIOR losses against a fixed
    // anchor of strength 1.0. The anchor is not a post and is never updated, so
    // it pins the scale and regularizes disconnected fragments.
    const ANCHOR: f64 = 1.0;
    let mut strength = vec![1.0f64; n];
    for _ in 0..1000 {
        let mut next = vec![0.0f64; n];
        let mut max_rel_change = 0.0f64;
        for i in 0..n {
            // Numerator: real wins + prior wins against the anchor.
            let numer = win_total[i] as f64 + PRIOR;
            // Denominator: expected games term over real opponents + the anchor.
            let mut denom = 0.0;
            for (&j, &g) in &games[i] {
                denom += g as f64 / (strength[i] + strength[j]);
            }
            denom += (2.0 * PRIOR) / (strength[i] + ANCHOR);
            let updated = if denom > 0.0 { numer / denom } else { strength[i] };
            next[i] = updated;
            let rel = (updated - strength[i]).abs() / strength[i].max(1e-12);
            max_rel_change = max_rel_change.max(rel);
        }
        // Guard against any non-finite blow-up (shouldn't happen with the prior).
        if next.iter().any(|s| !s.is_finite() || *s <= 0.0) {
            break;
        }
        strength = next;
        if max_rel_change < 1e-9 {
            break;
        }
    }

    // Grade = P(this post beats a uniformly random *other* graded post), the mean
    // Bradley-Terry win probability across all other participants. With n == 1
    // there is no other post to compare against, so that lone post stays ungraded.
    let mut grades = HashMap::new();
    if n < 2 {
        return grades;
    }
    for i in 0..n {
        let mut sum = 0.0f64;
        for j in 0..n {
            if i == j {
                continue;
            }
            sum += strength[i] / (strength[i] + strength[j]);
        }
        let grade = (sum / (n as f64 - 1.0)) as f32;
        grades.insert(names[i].clone(), grade.clamp(0.0, 1.0));
    }
    grades
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A resolver where every name is its own canonical post (no aliases, all
    /// posts exist).
    fn identity(name: &str) -> Option<String> {
        Some(name.to_string())
    }

    fn j(winner: &str, loser: &str, at: &str) -> Judgement {
        Judgement { winner: winner.into(), loser: loser.into(), at: Some(at.into()) }
    }

    #[test]
    fn total_order_grades_are_monotonic() {
        // a beats b and c; b beats c. Grades must rank a > b > c, all in range.
        let js = vec![
            j("a", "b", "t1"),
            j("a", "c", "t2"),
            j("b", "c", "t3"),
        ];
        let g = derive_grades(&js, identity);
        assert!(g["a"] > g["b"], "{g:?}");
        assert!(g["b"] > g["c"], "{g:?}");
        for v in g.values() {
            assert!((0.0..=1.0).contains(v), "{g:?}");
        }
        // The winner of everything is notable; the loser of everything is not.
        assert!(g["a"] >= 0.5, "{g:?}");
        assert!(g["c"] < 0.5, "{g:?}");
    }

    #[test]
    fn repeated_wins_raise_a_grade() {
        // Winning the same matchup many times should outrank winning it once.
        let many: Vec<Judgement> = (0..5).map(|k| j("a", "b", &format!("t{k}"))).collect();
        let g_many = derive_grades(&many, identity);
        let g_one = derive_grades(&[j("a", "b", "t0")], identity);
        assert!(g_many["a"] > g_one["a"], "more wins -> higher: {g_many:?} vs {g_one:?}");
    }

    #[test]
    fn disconnected_graph_stays_finite() {
        // Two islands that never meet: a>b and c>d. The anchor prior keeps every
        // strength finite; winners beat losers; all grades are valid numbers.
        let js = vec![j("a", "b", "t1"), j("c", "d", "t2")];
        let g = derive_grades(&js, identity);
        assert_eq!(g.len(), 4);
        for v in g.values() {
            assert!(v.is_finite() && (0.0..=1.0).contains(v), "{g:?}");
        }
        assert!(g["a"] > g["b"], "{g:?}");
        assert!(g["c"] > g["d"], "{g:?}");
    }

    #[test]
    fn dangling_and_self_judgements_are_skipped() {
        // "ghost" no longer exists; a-vs-a is nonsense. Only a>b remains.
        let js = vec![
            j("a", "ghost", "t1"),
            j("a", "a", "t2"),
            j("a", "b", "t3"),
        ];
        let resolve = |name: &str| match name {
            "a" | "b" => Some(name.to_string()),
            _ => None, // "ghost" was deleted
        };
        let g = derive_grades(&js, resolve);
        assert_eq!(g.len(), 2, "only a and b participate: {g:?}");
        assert!(g["a"] > g["b"], "{g:?}");
    }

    #[test]
    fn alias_rekeys_to_canonical() {
        // A judgement written against an old alias must fold onto the canonical
        // post, so "old-name" and "canonical" are one participant, not two.
        let js = vec![j("canonical", "b", "t1"), j("old-name", "b", "t2")];
        let resolve = |name: &str| match name {
            "old-name" | "canonical" => Some("canonical".to_string()),
            "b" => Some("b".to_string()),
            _ => None,
        };
        let g = derive_grades(&js, resolve);
        assert_eq!(g.len(), 2, "alias merged into canonical: {g:?}");
        assert!(g.contains_key("canonical") && g.contains_key("b"));
        assert!(g["canonical"] > g["b"], "{g:?}");
    }

    #[test]
    fn empty_ledger_is_empty_map() {
        assert!(derive_grades(&[], identity).is_empty());
    }

    #[test]
    fn ledger_name_matching() {
        assert!(is_ledger_name(".sajt-grade-judgements.jsonl"));
        assert!(is_ledger_name(".sajt-grade-judgements 2.jsonl")); // iCloud copy
        assert!(!is_ledger_name(".sajt-grade-judgements.jsonl.bak"));
        assert!(!is_ledger_name("grade-judgements.jsonl"));
        assert!(!is_ledger_name(".DS_Store"));
    }
}
