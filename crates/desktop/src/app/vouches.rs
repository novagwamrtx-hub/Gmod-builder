//! The user's vouches and warnings, and everyone's scores.

use eframe::egui;
use gmcb_core::community::{self, Submission, Tally, Votes};

use super::{App, Msg, PROJECT_REPO};
use crate::files;
use crate::settings::Settings;

/// Windows refuses to open a much longer link, so a long list of votes is
/// shared a part at a time.
const MAX_LINK: usize = 2000;

impl App {
    /// Sets the user's verdict on an addon or collection: vouch, warn, or
    /// with `None` take it back.
    pub(crate) fn vote(&mut self, id: u64, verdict: Option<bool>) {
        self.info(match verdict {
            Some(true) => "Vouched. Share your vouches from Settings to count them for everyone",
            Some(false) => "Warned. Share your vouches from Settings to count them for everyone",
            None => "Taken back",
        });
        self.vote_on(&[id], verdict);
    }

    /// The same verdict on several things at once.
    pub(crate) fn vote_on(&mut self, ids: &[u64], verdict: Option<bool>) {
        for &id in ids {
            match verdict {
                Some(vouch) => self.votes.insert(id, vouch),
                None => self.votes.remove(&id),
            };
        }
        self.save_votes();
    }

    fn save_votes(&mut self) {
        let text = serde_json::json!({ "votes": self.votes, "shared": self.shared }).to_string();
        let written = Settings::votes_path()
            .ok_or_else(|| "no data folder".to_string())
            .and_then(|path| files::write(&path, &text).map_err(|e| e.to_string()));
        if let Err(e) = written {
            self.error(format!("Could not save your vouches: {e}"));
        }
    }

    /// What everyone has said about an addon, with the user's own verdict
    /// counted in whether or not it has reached the published totals yet.
    pub(crate) fn tally_of(&self, id: u64) -> Tally {
        let mut tally = self.scores.get(&id).copied().unwrap_or_default();
        let (mine, sent) = (self.votes.get(&id).copied(), self.shared.get(&id).copied());
        if mine == sent {
            // Shared, so most likely counted already. Make sure it shows.
            match mine {
                Some(true) => tally.up = tally.up.max(1),
                Some(false) => tally.down = tally.down.max(1),
                None => {}
            }
            return tally;
        }
        match sent {
            Some(true) => tally.up = tally.up.saturating_sub(1),
            Some(false) => tally.down = tally.down.saturating_sub(1),
            None => {}
        }
        match mine {
            Some(true) => tally.up += 1,
            Some(false) => tally.down += 1,
            None => {}
        }
        tally
    }

    /// How many verdicts wait to be shared: in the next link, and in all.
    pub(crate) fn unshared_votes(&self) -> (usize, usize) {
        let pending = Submission::pending(&self.votes, &self.shared);
        (pending.fitting(PROJECT_REPO, MAX_LINK).len(), pending.len())
    }

    /// Opens the browser on a filled-in GitHub issue carrying the verdicts
    /// that have not been shared yet, or as many of them as fit in a link.
    pub(crate) fn share_votes(&mut self, ctx: &egui::Context) {
        let pending = Submission::pending(&self.votes, &self.shared);
        let part = pending.fitting(PROJECT_REPO, MAX_LINK);
        if part.is_empty() {
            return;
        }
        ctx.open_url(egui::OpenUrl::new_tab(part.url(PROJECT_REPO)));
        // Whether the user goes on to send it cannot be known from here.
        // "Start over" in Settings is for when they did not.
        part.mark_shared(&mut self.shared);
        self.info(match pending.len() - part.len() {
            0 => "Press \"Create\" on the GitHub page that opened to send them".to_string(),
            left => format!("Press \"Create\" on the GitHub page that opened, then share the other {left}"),
        });
        self.save_votes();
    }

    /// Forgets what has been shared, so that everything is sent again.
    pub(crate) fn share_from_scratch(&mut self) {
        self.shared.clear();
        self.info("Everything is ready to be shared again");
        self.save_votes();
    }

    pub(super) fn fetch_scores(&mut self, ctx: &egui::Context) {
        if self.settings.community {
            self.spawn(ctx, async { Msg::Scores(community::fetch_scores(PROJECT_REPO).await) });
        }
    }
}

/// The user's verdicts, and those of them that have been shared.
pub(super) fn load_votes() -> (Votes, Votes) {
    let saved = Settings::votes_path().and_then(|path| std::fs::read_to_string(path).ok());
    saved.map(|text| parse_votes(&text)).unwrap_or_default()
}

fn parse_votes(text: &str) -> (Votes, Votes) {
    #[derive(serde::Deserialize)]
    struct Saved {
        votes: Votes,
        shared: Votes,
    }
    match serde_json::from_str::<Saved>(text) {
        Ok(saved) => (saved.votes, saved.shared),
        // Before the app kept track of what was shared, the file was the votes alone.
        Err(_) => (serde_json::from_str(text).unwrap_or_default(), Votes::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_votes_are_read_in_both_forms() {
        let votes: Votes = [(12, true), (34, false)].into();
        let shared: Votes = [(12, true)].into();
        let text = serde_json::json!({ "votes": votes, "shared": shared }).to_string();
        assert_eq!(text, r#"{"shared":{"12":true},"votes":{"12":true,"34":false}}"#);
        assert_eq!(parse_votes(&text), (votes.clone(), shared));
        assert_eq!(parse_votes(r#"{"12":true,"34":false}"#), (votes, Votes::new()));
        assert_eq!(parse_votes("{}"), (Votes::new(), Votes::new()));
        assert_eq!(parse_votes("not json"), (Votes::new(), Votes::new()));
    }
}
