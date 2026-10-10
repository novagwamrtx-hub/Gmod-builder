//! The draft: editing it with undo, pulling in what addons require, and
//! keeping it saved in the library of drafts.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use gmcb_core::{Addon, AddonStatus, CollectionDraft};

use super::{App, Msg};
use crate::settings::Settings;

const AUTOSAVE_DELAY: Duration = Duration::from_secs(1);
const UNDO_STEPS: usize = 40;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum DraftView {
    #[default]
    Order,
    ByType,
    Largest,
    Updated,
}

impl DraftView {
    pub const ALL: [DraftView; 4] =
        [DraftView::Order, DraftView::ByType, DraftView::Largest, DraftView::Updated];

    pub fn label(self) -> &'static str {
        match self {
            DraftView::Order => "My order",
            DraftView::ByType => "By type",
            DraftView::Largest => "Largest first",
            DraftView::Updated => "Recently updated",
        }
    }
}

pub struct LibraryEntry {
    pub path: PathBuf,
    pub name: String,
    pub count: usize,
}

impl App {
    /// Call before changing the draft, so the change can be undone.
    pub(crate) fn checkpoint(&mut self) {
        self.undo.push(self.draft.clone());
        if self.undo.len() > UNDO_STEPS {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub(crate) fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub(crate) fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub(crate) fn undo(&mut self, ctx: &egui::Context) {
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.draft, previous));
            self.after_history_step(ctx, "Undone");
        }
    }

    pub(crate) fn redo(&mut self, ctx: &egui::Context) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.draft, next));
            self.after_history_step(ctx, "Redone");
        }
    }

    fn after_history_step(&mut self, ctx: &egui::Context, what: &str) {
        self.want_requirements.clear();
        self.collection_input = self.draft.collection_id.map(|id| id.to_string()).unwrap_or_default();
        self.info(what);
        self.draft_changed(ctx);
    }

    /// Call after any edit to the draft's addon list.
    pub(crate) fn draft_changed(&mut self, ctx: &egui::Context) {
        if self.report.is_some() || self.checking {
            self.run_check(ctx, false);
        } else {
            let wanted = self.draft.ids.iter().chain(&self.draft.maybe).copied();
            let missing: Vec<u64> = wanted.filter(|&id| self.needs_details(id)).collect();
            self.fetch_details(ctx, missing);
        }
    }

    pub(crate) fn add_to_draft(&mut self, ctx: &egui::Context, id: u64) {
        let title = self.title_of(id);
        if self.add_many(ctx, vec![id], true) > 1 {
            self.info(format!("Added {title} and what it requires"));
        }
    }

    /// Adds addons, returning how many were new (required items included).
    pub(crate) fn add_many(&mut self, ctx: &egui::Context, ids: Vec<u64>, with_required: bool) -> usize {
        self.checkpoint();
        let mut added = 0;
        for id in ids {
            if self.draft.add(id) {
                added += 1;
                if with_required && self.settings.auto_requirements {
                    self.want_requirements.insert(id);
                }
            }
        }
        if added == 0 {
            self.undo.pop();
            return 0;
        }
        added += self.resolve_requirements();
        self.draft_changed(ctx);
        added
    }

    /// Adds the required items of addons waiting for them, as far as their
    /// details are known. Returns how many were added.
    pub(super) fn resolve_requirements(&mut self) -> usize {
        let mut added = 0;
        loop {
            let ready: Vec<(u64, Vec<u64>)> = self
                .want_requirements
                .iter()
                .filter_map(|id| {
                    let addon = self.addons.get(id)?;
                    match &addon.required {
                        Some(required) => Some((*id, required.clone())),
                        // Unavailable, or keyless data that will never say.
                        None if addon.status != AddonStatus::Ok || !self.client.has_key() => {
                            Some((*id, Vec::new()))
                        }
                        None => None,
                    }
                })
                .collect();
            if ready.is_empty() {
                return added;
            }
            for (id, required) in ready {
                self.want_requirements.remove(&id);
                for required in required {
                    if self.draft.add(required) {
                        added += 1;
                        self.want_requirements.insert(required);
                    }
                }
            }
        }
    }

    pub(crate) fn remove_from_draft(&mut self, ctx: &egui::Context, id: u64) {
        self.checkpoint();
        self.draft.remove(id);
        self.want_requirements.remove(&id);
        let dependents: Vec<String> = self
            .draft
            .ids
            .iter()
            .filter_map(|other| self.addons.get(other))
            .filter(|other| other.required.as_ref().is_some_and(|r| r.contains(&id)))
            .map(Addon::display_title)
            .collect();
        let title = self.title_of(id);
        match dependents.as_slice() {
            [] => self.info(format!("Removed {title}. Ctrl+Z to undo")),
            [one] => self.error(format!("Removed {title}, which {one} requires. Ctrl+Z to undo")),
            [one, rest @ ..] => self.error(format!(
                "Removed {title}, which {one} and {} more require. Ctrl+Z to undo",
                rest.len()
            )),
        }
        self.draft_changed(ctx);
    }

    /// Puts an addon on the maybe pile, or takes it off.
    pub(crate) fn toggle_maybe(&mut self, ctx: &egui::Context, id: u64) {
        if self.draft.contains(id) {
            return;
        }
        self.checkpoint();
        if self.draft.maybe.contains(&id) {
            self.draft.maybe.retain(|&x| x != id);
        } else {
            self.draft.maybe.push(id);
        }
        self.draft_changed(ctx);
    }

    /// Removes every ticked addon from the draft, or with `to_maybe` moves
    /// them to the maybe pile.
    pub(crate) fn clear_picked(&mut self, ctx: &egui::Context, to_maybe: bool) {
        let picked: Vec<u64> =
            self.draft.ids.iter().copied().filter(|id| self.picked.contains(id)).collect();
        self.picked.clear();
        if picked.is_empty() {
            return;
        }
        self.checkpoint();
        for &id in &picked {
            self.draft.remove(id);
            self.want_requirements.remove(&id);
            if to_maybe {
                self.draft.maybe.push(id);
            }
        }
        let what = if to_maybe { "Moved to the maybe pile" } else { "Removed" };
        self.info(format!("{what}: {} addons. Ctrl+Z to undo", picked.len()));
        self.draft_changed(ctx);
    }

    pub(crate) fn move_in_draft(&mut self, from: usize, to: usize) {
        self.checkpoint();
        self.draft.ids.swap(from, to);
    }

    /// Imports whatever was pasted: one collection, or a list of addons.
    pub(crate) fn import(&mut self, ctx: &egui::Context, text: &str) -> bool {
        match gmcb_core::handoff::parse_id_list(text).as_slice() {
            [] => {
                self.error("Paste a collection link, or addon links or ids");
                false
            }
            &[id] => {
                let client = self.client.clone();
                self.spawn(ctx, async move {
                    Msg::Imported { id, result: client.collection_items(id).await }
                });
                true
            }
            ids => {
                let added = self.add_many(ctx, ids.to_vec(), false);
                self.info(format!("Added {added} addon(s)"));
                true
            }
        }
    }

    pub(crate) fn import_installed(&mut self, ctx: &egui::Context) {
        let added = self.add_many(ctx, self.installed.clone(), false);
        self.info(format!("Added {added} addon(s) installed on this computer"));
    }

    /// The draft's addons in order. Ones without details yet are stand-ins
    /// with just an id.
    pub(crate) fn draft_addons(&self) -> Vec<Addon> {
        self.draft.ids.iter().map(|&id| self.addon_or_stand_in(id)).collect()
    }

    pub(crate) fn addon_or_stand_in(&self, id: u64) -> Addon {
        let mut addon = self
            .addons
            .get(&id)
            .cloned()
            .unwrap_or_else(|| Addon::unavailable(id, AddonStatus::Ok));
        // Steam forgets the title of a removed addon; we may not have.
        if addon.title.is_empty() {
            if let Some(title) = self.draft.remembered_title(id) {
                addon.title = title.to_string();
            }
        }
        addon
    }

    // ---- The library of saved drafts ----

    pub(crate) fn refresh_library(&mut self) {
        let Some(dir) = Settings::drafts_dir() else { return };
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        let mut library: Vec<(std::time::SystemTime, LibraryEntry)> = entries
            .flatten()
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .filter_map(|entry| {
                let path = entry.path();
                let draft: CollectionDraft =
                    serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
                let modified = entry.metadata().and_then(|m| m.modified()).ok()?;
                let name = match draft.name.trim() {
                    "" => "Untitled collection".to_string(),
                    name => name.to_string(),
                };
                Some((modified, LibraryEntry { path, name, count: draft.ids.len() }))
            })
            .collect();
        library.sort_by_key(|entry| std::cmp::Reverse(entry.0));
        self.library = library.into_iter().map(|(_, entry)| entry).collect();
    }

    /// Writes the draft if it has changed and the user has paused editing.
    pub(crate) fn autosave(&mut self, ctx: &egui::Context) {
        if self.draft == self.saved {
            self.dirty_since = None;
            return;
        }
        let since = *self.dirty_since.get_or_insert_with(Instant::now);
        match AUTOSAVE_DELAY.checked_sub(since.elapsed()) {
            Some(left) => ctx.request_repaint_after(left),
            None => self.save_now(),
        }
    }

    /// Writes the draft to its file, giving it one in the library if it has
    /// none. A draft with nothing in it is not worth a file.
    pub(crate) fn save_now(&mut self) {
        self.dirty_since = None;
        if self.draft == self.saved {
            return;
        }
        let blank = self.draft.ids.is_empty() && self.draft.maybe.is_empty() && self.draft.name.trim().is_empty();
        if self.draft_path.is_none() && blank {
            return;
        }
        let is_new = self.draft_path.is_none();
        let path = match self.draft_path.clone().or_else(new_library_path) {
            Some(path) => path,
            None => return self.error("Could not find a folder to keep drafts in"),
        };
        let written = serde_json::to_string_pretty(&self.draft)
            .map_err(|e| e.to_string())
            .and_then(|text| {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&path, text).map_err(|e| e.to_string())
            });
        // Either way, do not retry on every frame.
        self.saved = self.draft.clone();
        match written {
            Ok(()) => {
                self.draft_path = Some(path.clone());
                if self.settings.last_draft.as_ref() != Some(&path) {
                    self.settings.last_draft = Some(path);
                    self.write_settings();
                }
                if is_new {
                    self.refresh_library();
                }
            }
            Err(e) => self.error(format!("Could not save the draft: {e}")),
        }
    }

    /// Saves the current draft and opens another.
    pub(crate) fn open_draft(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.save_now();
        self.load_draft(ctx, path);
        self.refresh_library();
    }

    pub(super) fn load_draft(&mut self, ctx: &egui::Context, path: PathBuf) {
        let loaded = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<CollectionDraft>(&text).map_err(|e| e.to_string()));
        match loaded {
            Ok(draft) => {
                self.set_draft(draft, Some(path.clone()));
                self.settings.last_draft = Some(path);
                self.write_settings();
                self.draft_changed(ctx);
            }
            Err(e) => self.error(format!("Could not open draft: {e}")),
        }
    }

    pub(crate) fn new_draft(&mut self) {
        self.save_now();
        self.set_draft(CollectionDraft::default(), None);
        self.settings.last_draft = None;
        self.write_settings();
        self.refresh_library();
    }

    /// Writes a copy of the draft somewhere else, without switching to it.
    pub(crate) fn export_draft(&mut self, path: PathBuf) {
        let written = serde_json::to_string_pretty(&self.draft)
            .map_err(|e| e.to_string())
            .and_then(|text| std::fs::write(&path, text).map_err(|e| e.to_string()));
        match written {
            Ok(()) => self.info(format!("Saved a copy to {}", path.display())),
            Err(e) => self.error(format!("Could not save the copy: {e}")),
        }
    }

    pub(crate) fn delete_draft(&mut self) {
        if let Some(path) = self.draft_path.take() {
            if let Err(e) = std::fs::remove_file(&path) {
                self.error(format!("Could not delete the draft: {e}"));
            }
        }
        self.set_draft(CollectionDraft::default(), None);
        self.settings.last_draft = None;
        self.write_settings();
        self.refresh_library();
    }

    fn set_draft(&mut self, draft: CollectionDraft, path: Option<PathBuf>) {
        self.saved = draft.clone();
        self.draft = draft;
        self.draft_path = path;
        self.dirty_since = None;
        self.collection_input = self.draft.collection_id.map(|id| id.to_string()).unwrap_or_default();
        self.report = None;
        self.changes.clear();
        self.undo.clear();
        self.redo.clear();
        self.want_requirements.clear();
        self.confirm_delete = false;
    }
}

fn new_library_path() -> Option<PathBuf> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    Some(Settings::drafts_dir()?.join(format!("draft-{millis}.json")))
}
