//! Vouches: what people who built collections with this app say about the
//! addons and collections they tried. A builder vouches for something that
//! worked well for them, or warns about something that broke or disappointed.
//!
//! Nothing here talks to a server of ours, because there is none. Votes are
//! kept on the builder's computer; sharing them means posting them as an
//! issue on the project's GitHub repository, where a workflow folds them into
//! a ledger (one set of votes per GitHub account) and publishes the totals as
//! a JSON file that the app downloads.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

/// Workshop id to verdict: `true` vouches, `false` warns.
pub type Votes = BTreeMap<u64, bool>;
/// Everyone's votes, by GitHub account number. The number, unlike the name,
/// stays the same when someone renames their account.
pub type Ledger = BTreeMap<u64, Votes>;
pub type Scores = HashMap<u64, Tally>;

/// First line of a submission, so the workflow can tell one from other issues.
pub const HEADER: &str = "gmcb-vouches 1";
/// Issues with this title are read as submissions.
pub const ISSUE_TITLE: &str = "Vouches";
/// Most verdicts read from one issue. The app sends far fewer at a time;
/// this is for issues written by hand.
pub const MAX_PER_SUBMISSION: usize = 1000;
/// Most verdicts kept for one account, so that nobody can bloat the ledger.
pub const MAX_PER_ACCOUNT: usize = 5000;
/// Below this many votes a score would mostly be noise.
pub const MIN_VOTES: u32 = 3;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tally {
    pub up: u32,
    pub down: u32,
}

impl Tally {
    pub fn votes(self) -> u32 {
        self.up + self.down
    }

    /// The vouch score, 0 to 100: the share of builders who vouched, pulled
    /// towards 50 while few have voted (as if two more had vouched and two
    /// had warned). Three vouches out of three score 71; thirty out of
    /// thirty score 94.
    pub fn score(self) -> Option<u32> {
        if self.votes() < MIN_VOTES {
            return None;
        }
        let share = f64::from(self.up + 2) / f64::from(self.votes() + 4);
        Some((share * 100.0).round() as u32)
    }
}

/// What one shared issue says: verdicts to record, and earlier ones to take
/// back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Submission {
    pub votes: Votes,
    pub undone: BTreeSet<u64>,
}

impl Submission {
    /// What has to be sent for the ledger to say `votes`, when `shared` is
    /// what was sent before.
    pub fn pending(votes: &Votes, shared: &Votes) -> Self {
        Self {
            votes: votes
                .iter()
                .filter(|(id, vouch)| shared.get(id) != Some(vouch))
                .map(|(&id, &vouch)| (id, vouch))
                .collect(),
            undone: shared.keys().filter(|id| !votes.contains_key(id)).copied().collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.votes.len() + self.undone.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The text of the issue: the header, then up to three lines such as
    /// `up 12,34`, `down 56` and `undo 78`.
    pub fn encode(&self) -> String {
        let with = |vouch: bool| self.votes.iter().filter(move |(_, &v)| v == vouch).map(|(id, _)| id);
        let mut text = String::from(HEADER);
        for (word, ids) in [
            ("up", with(true).map(u64::to_string).collect::<Vec<_>>()),
            ("down", with(false).map(u64::to_string).collect()),
            ("undo", self.undone.iter().map(u64::to_string).collect()),
        ] {
            if !ids.is_empty() {
                text.push_str(&format!("\n{word} {}", ids.join(",")));
            }
        }
        text
    }

    /// Reads an issue. The text comes from a stranger, so anything that is
    /// not understood is skipped, and without the header there is nothing.
    /// When an id is named more than once, the last mention counts.
    pub fn decode(text: &str) -> Self {
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        let mut found = Self::default();
        if lines.next() != Some(HEADER) {
            return found;
        }
        for line in lines {
            let (word, ids) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            let verdict = match word {
                "up" => Some(true),
                "down" => Some(false),
                "undo" => None,
                _ => continue,
            };
            let ids =
                ids.split(|c: char| c == ',' || c.is_whitespace()).filter_map(|id| id.parse::<u64>().ok());
            for id in ids.filter(|&id| id != 0) {
                if found.len() >= MAX_PER_SUBMISSION {
                    return found;
                }
                match verdict {
                    Some(vouch) => {
                        found.undone.remove(&id);
                        found.votes.insert(id, vouch);
                    }
                    None => {
                        found.votes.remove(&id);
                        found.undone.insert(id);
                    }
                }
            }
        }
        found
    }

    /// Link that opens a new issue on `repo` ("owner/name") with this
    /// submission filled in.
    pub fn url(&self, repo: &str) -> String {
        format!(
            "https://github.com/{repo}/issues/new?title={}&body={}",
            percent_encode(ISSUE_TITLE),
            percent_encode(&self.encode())
        )
    }

    /// As much of this submission as fits in a link of `max_len` characters;
    /// the rest has to go in another one. Takes back come first.
    pub fn fitting(&self, repo: &str, max_len: usize) -> Self {
        let (mut fits, mut too_many) = (0, self.len() + 1);
        while too_many - fits > 1 {
            let middle = (fits + too_many) / 2;
            if self.first(middle).url(repo).len() <= max_len {
                fits = middle;
            } else {
                too_many = middle;
            }
        }
        self.first(fits)
    }

    fn first(&self, count: usize) -> Self {
        let undone: BTreeSet<u64> = self.undone.iter().take(count).copied().collect();
        let left = count - undone.len();
        Self { votes: self.votes.iter().take(left).map(|(&id, &vouch)| (id, vouch)).collect(), undone }
    }

    /// Notes in `shared`, the record of what the user has sent, that this
    /// has been sent too.
    pub fn mark_shared(&self, shared: &mut Votes) {
        for id in &self.undone {
            shared.remove(id);
        }
        shared.extend(&self.votes);
    }

    /// Records this as `account`'s, replacing their earlier verdicts on the
    /// same things.
    pub fn apply(&self, ledger: &mut Ledger, account: u64) {
        let mut theirs = ledger.remove(&account).unwrap_or_default();
        for id in &self.undone {
            theirs.remove(id);
        }
        for (&id, &vouch) in &self.votes {
            if theirs.len() < MAX_PER_ACCOUNT || theirs.contains_key(&id) {
                theirs.insert(id, vouch);
            }
        }
        if !theirs.is_empty() {
            ledger.insert(account, theirs);
        }
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

/// An issue on the project's repository, as far as counting votes cares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub number: u64,
    /// GitHub's number for the account that opened it.
    pub account: u64,
    pub body: String,
}

/// Picks the submissions out of GitHub's list of a repository's issues (the
/// answer of `GET /repos/{repo}/issues`, one page after another), oldest
/// first. Pull requests and issues with another title are left alone.
pub fn submissions(json: &str) -> Result<Vec<Issue>, serde_json::Error> {
    fn collect(value: &serde_json::Value, found: &mut Vec<Issue>) {
        if let Some(list) = value.as_array() {
            list.iter().for_each(|item| collect(item, found));
            return;
        }
        let is_submission = value.get("pull_request").is_none()
            && value["title"].as_str().is_some_and(|title| title.trim() == ISSUE_TITLE);
        if let (true, Some(number), Some(account)) =
            (is_submission, value["number"].as_u64(), value["user"]["id"].as_u64())
        {
            let body = value["body"].as_str().unwrap_or_default().to_string();
            found.push(Issue { number, account, body });
        }
    }

    let mut found = Vec::new();
    for page in serde_json::Deserializer::from_str(json).into_iter::<serde_json::Value>() {
        collect(&page?, &mut found);
    }
    found.sort_by_key(|issue| issue.number);
    found.dedup_by_key(|issue| issue.number);
    Ok(found)
}

/// Folds `issues` into the ledger in order. Says, for each one, how many
/// verdicts it carried.
pub fn count(ledger: &mut Ledger, issues: &[Issue]) -> Vec<(u64, usize)> {
    issues
        .iter()
        .map(|issue| {
            let submission = Submission::decode(&issue.body);
            submission.apply(ledger, issue.account);
            (issue.number, submission.len())
        })
        .collect()
}

/// Where the published totals for `repo` can be downloaded.
pub fn scores_url(repo: &str) -> String {
    format!("https://raw.githubusercontent.com/{repo}/main/community/scores.json")
}

/// Commas stay as they are: they are allowed in a link, and a list of ids is
/// mostly commas.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b',' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Downloads the published totals. Before anyone has shared a vote there is
/// no file yet, which counts as no scores.
#[cfg(feature = "client")]
pub async fn fetch_scores(repo: &str) -> Result<Scores, String> {
    let failed = |e: reqwest::Error| e.without_url().to_string();
    let response = crate::http::client().get(scores_url(repo)).send().await.map_err(failed)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Scores::new());
    }
    let text = response.error_for_status().map_err(failed)?.text().await.map_err(failed)?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "someone/gmcb";

    fn submission(votes: &[(u64, bool)], undone: &[u64]) -> Submission {
        Submission { votes: votes.iter().copied().collect(), undone: undone.iter().copied().collect() }
    }

    #[test]
    fn submissions_round_trip() {
        let sent = submission(&[(104482086, true), (160250458, false), (7, true)], &[99]);
        let text = sent.encode();
        assert_eq!(text, "gmcb-vouches 1\nup 7,104482086\ndown 160250458\nundo 99");
        assert_eq!(Submission::decode(&text), sent);
        assert_eq!(Submission::default().encode(), "gmcb-vouches 1");
    }

    #[test]
    fn decoding_skips_what_it_does_not_understand() {
        let text = "gmcb-vouches 1\r\nup 12, 0,abc,34\nhello there\n+56\ndown 78  \n\nundo 34 90\nUP 1";
        assert_eq!(Submission::decode(text), submission(&[(12, true), (78, false)], &[34, 90]));
        assert!(Submission::decode("up 12\ndown 56").is_empty(), "no header, no votes");
        assert!(Submission::decode("").is_empty());

        let long: Vec<String> = (1..=MAX_PER_SUBMISSION as u64 + 50).map(|id| id.to_string()).collect();
        let text = format!("{HEADER}\nup {}", long.join(","));
        assert_eq!(Submission::decode(&text).len(), MAX_PER_SUBMISSION);
    }

    #[test]
    fn only_what_changed_since_last_time_is_sent() {
        let shared: Votes = [(1, true), (2, true), (3, false)].into();
        let votes: Votes = [(1, true), (2, false), (4, true)].into();
        let pending = Submission::pending(&votes, &shared);
        assert_eq!(pending, submission(&[(2, false), (4, true)], &[3]));

        let mut shared = shared;
        pending.mark_shared(&mut shared);
        assert_eq!(shared, votes);
        assert!(Submission::pending(&votes, &shared).is_empty());
    }

    #[test]
    fn a_long_list_is_sent_in_parts_that_fit_a_link() {
        let votes: Votes = (0..500).map(|i| (3_000_000_000 + i, i % 3 != 0)).collect();
        let mut shared: Votes = [(5, true), (6, false)].into();
        let mut links = 0;
        loop {
            let pending = Submission::pending(&votes, &shared);
            if pending.is_empty() {
                break;
            }
            let part = pending.fitting(REPO, 2000);
            assert!(part.url(REPO).len() <= 2000);
            assert!(part.len() > 100, "a link carries a good number of votes: {}", part.len());
            if links == 0 {
                assert_eq!(part.undone, [5, 6].into(), "takes back go first");
            }
            part.mark_shared(&mut shared);
            links += 1;
        }
        assert_eq!(shared, votes);
        assert!(links <= 4, "{links} links");

        let short = submission(&[(12, true)], &[]);
        assert_eq!(short.fitting(REPO, 2000), short);
        assert!(short.fitting(REPO, 10).is_empty(), "nothing fits a link that short");
    }

    #[test]
    fn an_account_counts_once_and_can_change_its_mind() {
        let mut ledger = Ledger::new();
        submission(&[(1, true), (2, true)], &[]).apply(&mut ledger, 10);
        submission(&[(1, true)], &[]).apply(&mut ledger, 11);
        submission(&[(1, false)], &[]).apply(&mut ledger, 10);
        submission(&[(1, true)], &[]).apply(&mut ledger, 11);
        Submission::default().apply(&mut ledger, 12);
        let totals = tally(&ledger);
        assert_eq!(totals[&1], Tally { up: 1, down: 1 });
        assert_eq!(totals[&2], Tally { up: 1, down: 0 });
        assert!(!ledger.contains_key(&12));

        submission(&[], &[1, 2]).apply(&mut ledger, 10);
        assert!(!ledger.contains_key(&10), "an account with nothing left is dropped");
        assert_eq!(tally(&ledger)[&1], Tally { up: 1, down: 0 });
    }

    #[test]
    fn one_account_cannot_fill_the_ledger() {
        let mut ledger = Ledger::new();
        for start in [0, 1000, 2000, 3000, 4000, 5000, 6000] {
            let votes: Vec<(u64, bool)> = (1..=1000).map(|i| (start + i, true)).collect();
            submission(&votes, &[]).apply(&mut ledger, 10);
        }
        assert_eq!(ledger[&10].len(), MAX_PER_ACCOUNT);
        submission(&[(1, false)], &[]).apply(&mut ledger, 10);
        assert!(!ledger[&10][&1], "what is already there can still change");
    }

    #[test]
    fn issues_are_picked_out_of_githubs_answer() {
        // Two pages as `gh api --paginate` prints them: one list after another.
        let json = r#"[
            {"number": 7, "title": "Vouches", "body": "gmcb-vouches 1\nup 12", "user": {"login": "ann", "id": 10}},
            {"number": 3, "title": " Vouches ", "body": "gmcb-vouches 1\r\ndown 12\r\nup 34", "user": {"login": "bob", "id": 11}},
            {"number": 4, "title": "The app crashes", "body": "gmcb-vouches 1\nup 99", "user": {"id": 12}}
        ][
            {"number": 9, "title": "Vouches", "body": null, "user": {"id": 13}},
            {"number": 8, "title": "Vouches", "body": "x", "user": {"id": 14}, "pull_request": {"url": "x"}},
            {"number": 10, "title": "Vouches", "body": "gmcb-vouches 1\nundo 12\nup 56", "user": {"id": 10}},
            {"title": "Vouches", "body": "no number"}
        ]"#;
        let issues = submissions(json).unwrap();
        assert_eq!(issues.iter().map(|issue| issue.number).collect::<Vec<_>>(), [3, 7, 9, 10]);
        assert_eq!(issues[0].account, 11);

        let mut ledger = Ledger::new();
        assert_eq!(count(&mut ledger, &issues), [(3, 2), (7, 1), (9, 0), (10, 2)]);
        assert_eq!(ledger[&10], [(56, true)].into());
        assert_eq!(ledger[&11], [(12, false), (34, true)].into());
        assert_eq!(ledger.len(), 2);

        // Counting the same issues again changes nothing.
        let before = ledger.clone();
        count(&mut ledger, &issues);
        assert_eq!(ledger, before);

        assert!(submissions("[]").unwrap().is_empty());
        assert!(submissions("").unwrap().is_empty());
        assert!(submissions("{ not json").is_err());
    }

    #[test]
    fn the_ledger_is_kept_as_json_by_account_number() {
        let mut ledger = Ledger::new();
        submission(&[(12, true)], &[]).apply(&mut ledger, 10);
        let text = serde_json::to_string(&ledger).unwrap();
        assert_eq!(text, r#"{"10":{"12":true}}"#);
        assert_eq!(serde_json::from_str::<Ledger>(&text).unwrap(), ledger);
        assert_eq!(serde_json::to_string(&tally(&ledger)).unwrap(), r#"{"12":{"up":1,"down":0}}"#);
    }

    #[test]
    fn scores_grow_with_agreement_and_with_votes() {
        assert_eq!(Tally { up: 2, down: 0 }.score(), None);
        assert_eq!(Tally { up: 3, down: 0 }.score(), Some(71));
        assert_eq!(Tally { up: 30, down: 0 }.score(), Some(94));
        assert_eq!(Tally { up: 9, down: 1 }.score(), Some(79));
        assert!(Tally { up: 30, down: 10 }.score() < Tally { up: 30, down: 0 }.score());
        assert_eq!(Tally { up: 0, down: 5 }.score(), Some(22));
        assert_eq!(Tally { up: 2, down: 2 }.score(), Some(50));
    }

    #[test]
    fn links_are_encoded() {
        let url = submission(&[(12, true), (34, false), (56, true)], &[78]).url(REPO);
        assert_eq!(
            url,
            "https://github.com/someone/gmcb/issues/new?title=Vouches&body=gmcb-vouches%201%0Aup%2012,56%0Adown%2034%0Aundo%2078"
        );
    }
}
