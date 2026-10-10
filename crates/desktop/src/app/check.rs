//! Checking the draft for problems and for what changed since last time.

use eframe::egui;
use gmcb_core::{analysis, changes};

use super::App;
use crate::format::now_unix;
use crate::local;

impl App {
    /// Checks the draft. `refresh` re-downloads every addon's details first;
    /// otherwise only the ones we lack are fetched.
    pub(crate) fn run_check(&mut self, ctx: &egui::Context, refresh: bool) {
        let ids: Vec<u64> = self
            .draft
            .ids
            .iter()
            .chain(&self.draft.maybe)
            .copied()
            .filter(|&id| refresh || self.needs_details(id))
            .collect();
        self.fetch_details(ctx, ids);
        if self.draft.ids.iter().any(|id| self.pending_details.contains(id)) {
            self.checking = true;
        } else {
            self.finish_check(ctx);
        }
    }

    pub(super) fn finish_check(&mut self, ctx: &egui::Context) {
        self.load_archives();
        let addons = self.draft_addons();
        let options = analysis::Options {
            target: self.draft.target.clone(),
            files: self
                .draft
                .ids
                .iter()
                .filter_map(|id| Some((*id, self.archives.get(id)?.files.clone())))
                .collect(),
            stale_after_years: self.settings.stale_after_years,
        };
        let report = analysis::analyze(&addons, now_unix(), &options);
        self.checking = false;
        // So the report can name missing required addons, not just number them.
        let unknown = analysis::missing_requirements(&report)
            .into_iter()
            .filter(|id| !self.addons.contains_key(id));
        self.fetch_details(ctx, unknown.collect());
        self.report = Some(report);

        self.changes = changes::diff(&self.draft.snapshot, &addons);
        if self.changes.is_empty() {
            self.mark_changes_seen();
        } else {
            changes::remember_new(&mut self.draft.snapshot, &addons);
        }
    }

    /// Makes the addons as they are now the baseline for "what changed".
    pub(crate) fn mark_changes_seen(&mut self) {
        self.draft.snapshot = changes::reviewed(&self.draft.snapshot, &self.draft_addons());
        self.draft.snapshot_at = now_unix();
        self.changes.clear();
    }

    pub(crate) fn dismiss(&mut self, key: String) {
        self.checkpoint();
        self.draft.dismissed.push(key);
    }

    pub(crate) fn restore_dismissed(&mut self) {
        self.checkpoint();
        self.draft.dismissed.clear();
    }

    /// Reads the file lists of the draft's addons that the game has on disk.
    pub(crate) fn load_archives(&mut self) {
        let Some(gmod) = &self.gmod_dir else { return };
        for &id in &self.draft.ids {
            if !self.archives.contains_key(&id) && self.installed.binary_search(&id).is_ok() {
                if let Some(archive) = local::read_archive(gmod, id) {
                    self.archives.insert(id, archive);
                }
            }
        }
    }
}
