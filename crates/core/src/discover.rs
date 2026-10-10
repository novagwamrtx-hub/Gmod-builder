//! Ranking for "what do people building this kind of collection use?".
//!
//! The Steam client samples collections that match a theme; this module
//! counts their members and scores them. Counting alone surfaces addons that
//! are in every collection of any kind (admin mods, popular bases), so each
//! addon's share of the sample is weighed against its share of all
//! collections.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// Collections outside this size range say little: tiny ones are noise and
/// huge ones are someone's entire subscription list.
const MIN_MEMBERS: usize = 5;
const MAX_MEMBERS: usize = 400;
/// An addon must appear in at least this many sampled collections.
const MIN_APPEARANCES: u32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Suggestion {
    pub id: u64,
    /// Sampled collections that contain it.
    pub in_sample: u32,
    /// Collections of any kind that contain it.
    pub in_all: u64,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Discovery {
    /// Collections the counts are out of.
    pub sample_size: usize,
    /// Best first.
    pub suggestions: Vec<Suggestion>,
}

/// With at least this many collections that mention the whole theme, the
/// ones that do not are left out of the sample.
pub const MIN_FOCUSED: usize = 20;

/// Whether a collection's title or description mentions every word of the
/// theme. Plurals and case are ignored, so "Realistic Weapon Pack" is on
/// theme for "realistic weapons".
pub fn on_theme(theme: &str, title: &str, description: &str) -> bool {
    let text = format!("{title} {description}").to_lowercase().replace(['-', ':', '_'], " ");
    let theme = theme.to_lowercase().replace(['-', ':', '_'], " ");
    theme.split_whitespace().all(|word| {
        let stem = if word.len() > 3 { word.trim_end_matches('s') } else { word };
        text.contains(stem)
    })
}

/// Counts how many of the usable `collections` each addon is in. Returns the
/// number of usable collections and (addon, count) pairs, most common first.
pub fn count_members(collections: &[Vec<u64>]) -> (usize, Vec<(u64, u32)>) {
    let mut sample_size = 0;
    let mut counts: HashMap<u64, u32> = HashMap::new();
    for members in collections {
        let unique: HashSet<u64> = members.iter().copied().collect();
        if !(MIN_MEMBERS..=MAX_MEMBERS).contains(&unique.len()) {
            continue;
        }
        sample_size += 1;
        for id in unique {
            *counts.entry(id).or_default() += 1;
        }
    }
    let mut counts: Vec<(u64, u32)> =
        counts.into_iter().filter(|&(_, n)| n >= MIN_APPEARANCES).collect();
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    (sample_size, counts)
}

/// Scores (addon, appearances in the sample, appearances in all collections).
pub fn rank(sample_size: usize, total_collections: u64, counts: &[(u64, u32, u64)]) -> Vec<Suggestion> {
    let mut suggestions: Vec<Suggestion> = counts
        .iter()
        .map(|&(id, in_sample, in_all)| Suggestion {
            id,
            in_sample,
            in_all,
            score: score(sample_size, total_collections, in_sample, in_all),
        })
        .collect();
    suggestions.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.id.cmp(&b.id)));
    suggestions
}

/// Share of the sample, scaled by how over-represented the addon is there.
/// The logarithm keeps one-off niche addons from outranking the staples.
fn score(sample_size: usize, total_collections: u64, in_sample: u32, in_all: u64) -> f64 {
    if sample_size == 0 || total_collections == 0 {
        return 0.0;
    }
    let share = f64::from(in_sample) / sample_size as f64;
    let usual_share = in_all.max(1) as f64 / total_collections as f64;
    share * (1.0 + share / usual_share).ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_collection_once_and_skips_unusable_ones() {
        let collections = vec![
            vec![1, 1, 2, 3, 4, 5],
            vec![1, 2, 3, 4, 6],
            vec![1, 2, 7, 8, 9],
            vec![1, 2],                 // too small
            (0..500).collect(),         // someone's whole subscription list
        ];
        let (sample_size, counts) = count_members(&collections);
        assert_eq!(sample_size, 3);
        assert_eq!(counts, vec![(1, 3), (2, 3)]);
    }

    #[test]
    fn theme_specific_addons_beat_ones_that_are_everywhere() {
        // Figures from a real "zombie survival" sample of 269 collections out
        // of 2.2 million: an admin mod, a zombie NPC pack, a vehicle base.
        let ranked = rank(
            269,
            2_204_113,
            &[(1, 60, 359_174), (2, 36, 32_297), (3, 51, 545_178)],
        );
        let order: Vec<u64> = ranked.iter().map(|s| s.id).collect();
        assert_eq!(order, vec![2, 1, 3]);
    }

    #[test]
    fn on_theme_needs_every_word_but_forgives_plurals() {
        assert!(on_theme("realistic weapons", "Realistic Weapon Pack", ""));
        assert!(on_theme("hospital rp", "My server", "A Hospital-RP content pack"));
        assert!(!on_theme("hospital rp", "LePtitDarkRP #1", "content"));
        assert!(on_theme("", "anything", ""));
    }

    #[test]
    fn empty_inputs_do_not_divide_by_zero() {
        assert_eq!(rank(0, 0, &[(1, 0, 0)])[0].score, 0.0);
    }
}
