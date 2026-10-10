//! Vouches: what people who built collections with this app say about the
//! addons and collections they tried. A builder vouches for something that
//! worked well for them, or warns about something that broke or disappointed.
//!
//! Nothing here talks to a server of ours, because there is none. Votes are
//! kept on the builder's computer; sharing them means posting them as an
//! issue on the project's GitHub repository, where a workflow folds them into
//! a ledger (one set of votes per GitHub account) and publishes the totals as
//! a JSON file that the app downloads.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// Workshop id to verdict: `true` vouches, `false` warns.
pub type Votes = BTreeMap<u64, bool>;
/// Everyone's votes, by GitHub account. A new submission replaces that
/// account's verdict on the ids it mentions.
pub type Ledger = BTreeMap<String, Votes>;
pub type Scores = HashMap<u64, Tally>;

/// First line of a submission, so the workflow can tell one from other issues.
pub const HEADER: &str = "gmcb-vouches 1";
/// Issues with this title are read as submissions.
pub const ISSUE_TITLE: &str = "Vouches";
/// Most votes one submission may carry; keeps the issue link within what
/// browsers and GitHub accept.
pub const MAX_PER_SUBMISSION: usize = 400;
/// Below this many votes a score would mostly be noise.
const MIN_VOTES: u32 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    pub up: u32,
    pub down: u32,
}

impl Tally {
    pub fn votes(self) -> u32 {
        self.up + self.down
    }

    /// The vouch score, 0 to 100: the share of builders who vouched, marked
    /// down while few have voted (the lower end of a 95% Wilson interval).
    /// Three vouches out of three score 44; thirty out of thirty score 89.
    pub fn score(self) -> Option<u32> {
        let n = f64::from(self.votes());
        if self.votes() < MIN_VOTES {
            return None;
        }
        let z = 1.96_f64;
        let share = f64::from(self.up) / n;
        let centre = share + z * z / (2.0 * n);
        let spread = z * ((share * (1.0 - share) + z * z / (4.0 * n)) / n).sqrt();
        let lower = (centre - spread) / (1.0 + z * z / n);
        Some((lower * 100.0).round().clamp(0.0, 100.0) as u32)
    }
}

/// The text of a submission: the header, then one `+id` or `-id` per line.
pub fn encode(votes: &Votes) -> String {
    let mut text = String::from(HEADER);
    for (id, vouch) in votes.iter().take(MAX_PER_SUBMISSION) {
        text.push('\n');
        text.push(if *vouch { '+' } else { '-' });
        text.push_str(&id.to_string());
    }
    text
}

/// Reads a submission. Anything that is not a vote line is ignored, since
/// the text comes from a public issue; without the header there are no votes.
pub fn decode(text: &str) -> Votes {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    if lines.next() != Some(HEADER) {
        return Votes::new();
    }
    lines
        .filter_map(|line| {
            let vouch = match line.as_bytes().first()? {
                b'+' => true,
                b'-' => false,
                _ => return None,
            };
            let id: u64 = line[1..].parse().ok()?;
            (id != 0).then_some((id, vouch))
        })
        .take(MAX_PER_SUBMISSION)
        .collect()
}

/// Records `votes` as `account`'s, replacing their earlier verdicts.
pub fn merge(ledger: &mut Ledger, account: &str, votes: Votes) {
    if !votes.is_empty() {
        ledger.entry(account.to_string()).or_default().extend(votes);
    }
}

pub fn tally(ledger: &Ledger) -> BTreeMap<u64, Tally> {
    let mut totals: BTreeMap<u64, Tally> = BTreeMap::new();
    for votes in ledger.values() {
        for (&id, &vouch) in votes {
            let entry = totals.entry(id).or_default();
            if vouch {
                entry.up += 1;
            } else {
                entry.down += 1;
            }
        }
    }
    totals
}

/// Link that opens a new issue on `repo` ("owner/name") with the votes filled in.
pub fn submission_url(repo: &str, votes: &Votes) -> String {
    format!(
        "https://github.com/{repo}/issues/new?title={}&body={}",
        percent_encode(ISSUE_TITLE),
        percent_encode(&encode(votes))
    )
}

/// Where the published totals for `repo` can be downloaded.
pub fn scores_url(repo: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/main/community/scores.json")
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Downloads the published totals.
#[cfg(feature = "client")]
pub async fn fetch_scores(repo: &str) -> Result<Scores, String> {
    let response = reqwest::get(scores_url(repo)).await.map_err(|e| e.to_string())?;
    let text = response.error_for_status().map_err(|e| e.to_string())?.text().await.map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submissions_round_trip() {
        let votes: Votes = [(104482086, true), (160250458, false)].into();
        let text = encode(&votes);
        assert_eq!(text, "gmcb-vouches 1\n+104482086\n-160250458");
        assert_eq!(decode(&text), votes);
    }

    #[test]
    fn decoding_ignores_everything_that_is_not_a_vote() {
        let text = "gmcb-vouches 1\r\n+12\nhello\n*34\n-0\n+abc\n\n-56  ";
        assert_eq!(decode(text), [(12, true), (56, false)].into());
        assert!(decode("+12\n-56").is_empty(), "no header, no votes");
    }

    #[test]
    fn an_account_counts_once_and_can_change_its_mind() {
        let mut ledger = Ledger::new();
        merge(&mut ledger, "ann", [(1, true), (2, true)].into());
        merge(&mut ledger, "bob", [(1, true)].into());
        merge(&mut ledger, "ann", [(1, false)].into());
        merge(&mut ledger, "eve", Votes::new());
        let totals = tally(&ledger);
        assert_eq!(totals[&1], Tally { up: 1, down: 1 });
        assert_eq!(totals[&2], Tally { up: 1, down: 0 });
        assert!(!ledger.contains_key("eve"));
    }

    #[test]
    fn scores_grow_with_agreement_and_with_votes() {
        assert_eq!(Tally { up: 2, down: 0 }.score(), None);
        assert_eq!(Tally { up: 3, down: 0 }.score(), Some(44));
        assert_eq!(Tally { up: 30, down: 0 }.score(), Some(89));
        assert!(Tally { up: 30, down: 10 }.score() < Tally { up: 30, down: 0 }.score());
        assert_eq!(Tally { up: 0, down: 5 }.score(), Some(0));
    }

    #[test]
    fn links_are_encoded() {
        let url = submission_url("someone/gmcb", &[(12, true), (34, false)].into());
        assert_eq!(
            url,
            "https://github.com/someone/gmcb/issues/new?title=Vouches&body=gmcb-vouches%201%0A%2B12%0A-34"
        );
    }
}
