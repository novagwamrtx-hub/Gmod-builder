//! The user's vouches and warnings, and everyone's scores.

use eframe::egui;
use gmcb_core::community::{self, Tally, Votes};

use super::{App, Msg};
use crate::settings::Settings;

/// The project's GitHub repository ("owner/name"), which collects shared
/// vouches and publishes the scores. Empty until the project has one; the
/// community features stay local-only meanwhile.
pub const COMMUNITY_REPO: &str = "";

impl App {
    /// Sets the user's verdict on an addon or collection: vouch, warn, or
    /// with `None` take it back.
    pub(crate) fn vote(&mut self, id: u64, verdict: Option<bool>) {
        match verdict {
            Some(vouch) => self.votes.insert(id, vouch),
            None => self.votes.remove(&id),
        };
        self.unshared_votes = true;
        let saved = Settings::votes_path()
            .ok_or_else(|| "no data folder".to_string())
            .and_then(|path| {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                let text = serde_json::to_string(&self.votes).map_err(|e| e.to_string())?;
                std::fs::write(path, text).map_err(|e| e.to_string())
            });
        if let Err(e) = saved {
            self.error(format!("Could not save your vouches: {e}"));
        }
    }

    /// What everyone has said about an addon, with the user's own vote
    /// counted in even before it is shared.
    pub(crate) fn tally_of(&self, id: u64) -> Tally {
        let mut tally = self.scores.get(&id).copied().unwrap_or_default();
        match self.votes.get(&id) {
            Some(true) => tally.up = tally.up.max(1),
            Some(false) => tally.down = tally.down.max(1),
            None => {}
        }
        tally
    }

    /// Opens the browser on a filled-in GitHub issue carrying the votes.
    pub(crate) fn share_votes(&mut self, ctx: &egui::Context) {
        if COMMUNITY_REPO.is_empty() || self.votes.is_empty() {
            return;
        }
        ctx.open_url(egui::OpenUrl::new_tab(community::submission_url(COMMUNITY_REPO, &self.votes)));
        self.unshared_votes = false;
        self.info("Press \"Create\" on the page that opened to send them");
    }

    pub(super) fn fetch_scores(&mut self, ctx: &egui::Context) {
        if COMMUNITY_REPO.is_empty() || !self.settings.community {
            return;
        }
        self.spawn(ctx, async { Msg::Scores(community::fetch_scores(COMMUNITY_REPO).await) });
    }
}

pub(super) fn load_votes() -> Votes {
    Settings::votes_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}
