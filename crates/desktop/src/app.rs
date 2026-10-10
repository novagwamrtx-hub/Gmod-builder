//! Application state and background work. The UI lives in `tabs.rs` and the
//! `tab_*.rs` files.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui;
use gmcb_core::analysis::{self, Report};
use gmcb_core::changes::{self, Change};
use gmcb_core::community::{self, Scores, Tally, Votes};
use gmcb_core::discover::Discovery;
use gmcb_core::gma::Archive;
use gmcb_core::{
    Addon, AddonStatus, CollectionDraft, Error, Kind, SearchPage, SearchQuery, Sort, SteamClient,
};

use crate::format::now_unix;
use crate::local;
use crate::settings::Settings;

const SEARCH_PAGE_SIZE: u32 = 50;
const AUTOSAVE_DELAY: Duration = Duration::from_secs(1);
const UNDO_STEPS: usize = 40;
/// A saved discovery is reused for this long before the Workshop is asked again.
const DISCOVERY_KEPT: u64 = 3 * 24 * 60 * 60;
/// The project's GitHub repository ("owner/name"), which collects shared
/// vouches and publishes the scores. Empty until the project has one; the
/// community features stay local-only meanwhile.
pub const COMMUNITY_REPO: &str = "";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Discover,
    Search,
    Draft,
    Check,
    Export,
    Settings,
}

/// Lists in the detail panel that are fetched when asked for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RelatedKind {
    /// Other addons by the same author.
    Author,
    /// Addons that list this one as required.
    BuiltFor,
}

/// Result of a background task, delivered to the UI thread.
enum Msg {
    Search { generation: u64, result: Result<SearchPage, Error> },
    Details { ids: Vec<u64>, result: Result<Vec<Addon>, Error> },
    Imported { id: u64, result: Result<Vec<u64>, Error> },
    Discovered { generation: u64, key: DiscoverKey, result: Result<(Discovery, Vec<Addon>), Error> },
    Related { id: u64, kind: RelatedKind, result: Result<Vec<Addon>, Error> },
    KeyChecked { key: String, result: Result<(), Error> },
    Scores(Result<Scores, String>),
}

#[derive(Default)]
pub struct SearchState {
    pub text: String,
    pub tags: Vec<String>,
    pub excluded: Vec<String>,
    pub sort: Sort,
    /// Search collections instead of addons.
    pub collections: bool,
    /// Dupes, saves and demos share the Workshop with addons; hidden unless set.
    pub include_non_addons: bool,
    pub hide_in_draft: bool,
    /// Client-side filters; 0 means no limit.
    pub max_size_mb: u32,
    pub max_age_years: u32,
    pub results: Vec<Addon>,
    /// Whether `results` are collections.
    pub results_are_collections: bool,
    pub total: u64,
    pub next_cursor: Option<String>,
    pub loading: bool,
    pub searched: bool,
    /// A collection opened from the results, to pick addons out of.
    pub browse: Option<Browse>,
    /// Bumped on every new search so late pages of an old one are dropped.
    generation: u64,
}

pub struct Browse {
    pub id: u64,
    pub title: String,
    pub ids: Vec<u64>,
}

/// Lower-cased theme, the addon the search was built around, and how many
/// hundred collections were read.
type DiscoverKey = (String, Option<u64>, usize);

/// A discovery as kept on disk.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedDiscovery {
    at: u64,
    discovery: Discovery,
    addons: Vec<Addon>,
}

#[derive(Default)]
pub struct DiscoverState {
    pub theme: String,
    pub anchor_input: String,
    pub anchor: Option<u64>,
    pub loading: bool,
    pub result: Option<Discovery>,
    /// What `result` is for, as shown to the user.
    pub result_label: String,
    /// When `result` was worked out, if it was not just now.
    pub result_from: Option<u64>,
    /// The ideas list under the search field is showing.
    pub ideas_open: bool,
    /// Types the user has switched off in the results.
    pub hidden: HashSet<Kind>,
    cache: HashMap<DiscoverKey, Discovery>,
    generation: u64,
}

#[derive(Default)]
pub struct Related {
    pub author: Option<Vec<u64>>,
    pub built_for: Option<Vec<u64>>,
    pub loading: bool,
}

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

#[derive(Clone, PartialEq, Eq)]
pub enum KeyStatus {
    Unknown,
    Checking,
    Valid,
    Invalid(String),
}

pub struct LibraryEntry {
    pub path: PathBuf,
    pub name: String,
    pub count: usize,
}

pub struct Status {
    pub text: String,
    pub is_error: bool,
    pub since: Instant,
}

pub struct App {
    rt: tokio::runtime::Runtime,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub(crate) client: SteamClient,
    pub(crate) settings: Settings,
    pub(crate) key_input: String,
    pub(crate) key_status: KeyStatus,
    /// A key spotted on the clipboard while the key field is empty.
    pub(crate) clipboard_key: Option<String>,
    pub(crate) clipboard_checked: f64,
    pub(crate) gmod_input: String,
    pub(crate) tab: Tab,

    pub(crate) draft: CollectionDraft,
    pub(crate) draft_path: Option<PathBuf>,
    /// The draft as last written to disk.
    saved: CollectionDraft,
    dirty_since: Option<Instant>,
    pub(crate) library: Vec<LibraryEntry>,
    pub(crate) confirm_delete: bool,
    undo: Vec<CollectionDraft>,
    redo: Vec<CollectionDraft>,
    pub(crate) draft_view: DraftView,
    /// Addons ticked in the draft, for acting on several at once.
    pub(crate) picked: HashSet<u64>,

    /// Details of every addon seen this session.
    pub(crate) addons: HashMap<u64, Addon>,
    pending_details: HashSet<u64>,
    /// Addons whose required items should join the draft once known.
    want_requirements: HashSet<u64>,
    pub(crate) search: SearchState,
    pub(crate) discover: DiscoverState,
    /// The addon shown in the detail panel.
    pub(crate) selected: Option<u64>,
    pub(crate) related: HashMap<u64, Related>,
    pub(crate) import_input: String,
    pub(crate) collection_input: String,

    pub(crate) report: Option<Report>,
    /// What changed since the draft's snapshot, as of the last check.
    pub(crate) changes: Vec<Change>,
    pub(crate) show_dismissed: bool,
    pub(crate) checking: bool,
    pub(crate) trim_lua: bool,

    pub(crate) gmod_dir: Option<PathBuf>,
    /// Addons the game has downloaded on this computer.
    pub(crate) installed: Vec<u64>,
    pub(crate) archives: HashMap<u64, Archive>,

    /// The user's own vouches (true) and warnings (false).
    pub(crate) votes: Votes,
    /// Votes cast since they were last shared.
    pub(crate) unshared_votes: bool,
    /// Everyone's totals, as published by the project.
    pub(crate) scores: Scores,

    /// When the tab last changed, for the page's entrance.
    pub(crate) tab_changed: f64,
    pub(crate) shown_tab: Option<Tab>,
    /// The addon last shown in the detail panel, kept while it slides shut.
    pub(crate) last_selected: Option<u64>,

    pub(crate) busy: usize,
    pub(crate) status: Option<Status>,
}

impl App {
    /// `draft` is a file to open instead of the one from last time.
    pub fn new(ctx: &egui::Context, draft: Option<PathBuf>) -> Self {
        let settings = Settings::load();
        let (tx, rx) = channel();
        let gmod_dir = local::find_gmod(&settings.gmod_dir);
        let mut app = Self {
            rt: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("failed to start the async runtime"),
            tx,
            rx,
            client: SteamClient::new(Some(settings.api_key.clone())),
            key_input: settings.api_key.clone(),
            key_status: KeyStatus::Unknown,
            clipboard_key: None,
            clipboard_checked: 0.0,
            gmod_input: settings.gmod_dir.clone(),
            tab: if settings.api_key.trim().is_empty() { Tab::Settings } else { Tab::Discover },
            draft: CollectionDraft::default(),
            draft_path: None,
            saved: CollectionDraft::default(),
            dirty_since: None,
            library: Vec::new(),
            confirm_delete: false,
            undo: Vec::new(),
            redo: Vec::new(),
            draft_view: DraftView::default(),
            picked: HashSet::new(),
            addons: HashMap::new(),
            pending_details: HashSet::new(),
            want_requirements: HashSet::new(),
            search: SearchState::default(),
            discover: DiscoverState::default(),
            selected: None,
            related: HashMap::new(),
            import_input: String::new(),
            collection_input: String::new(),
            report: None,
            changes: Vec::new(),
            show_dismissed: false,
            checking: false,
            trim_lua: true,
            installed: gmod_dir.as_deref().map(local::installed_ids).unwrap_or_default(),
            gmod_dir,
            archives: HashMap::new(),
            votes: load_votes(),
            unshared_votes: false,
            scores: Scores::new(),
            tab_changed: 0.0,
            shown_tab: None,
            last_selected: None,
            busy: 0,
            status: None,
            settings,
        };
        crate::theme::apply(ctx, &app.settings.look);
        app.fetch_scores(ctx);
        app.refresh_library();
        let reopen = app.settings.last_draft.clone().filter(|path| path.is_file());
        match draft {
            Some(path) => {
                app.tab = Tab::Draft;
                app.load_draft(ctx, path);
            }
            None => {
                if let Some(path) = reopen {
                    app.load_draft(ctx, path);
                }
            }
        }
        app
    }

    fn spawn(&mut self, ctx: &egui::Context, task: impl Future<Output = Msg> + Send + 'static) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        self.busy += 1;
        self.rt.spawn(async move {
            let _ = tx.send(task.await);
            ctx.request_repaint();
        });
    }

    pub(crate) fn info(&mut self, text: impl Into<String>) {
        self.status = Some(Status { text: text.into(), is_error: false, since: Instant::now() });
    }

    pub(crate) fn error(&mut self, text: impl ToString) {
        self.status = Some(Status { text: text.to_string(), is_error: true, since: Instant::now() });
    }

    pub(crate) fn title_of(&self, id: u64) -> String {
        match self.addons.get(&id).filter(|a| !a.title.is_empty()) {
            Some(addon) => addon.title.clone(),
            None => match self.draft.remembered_title(id) {
                Some(title) => title.to_string(),
                None => format!("Addon {id}"),
            },
        }
    }

    pub(crate) fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            self.busy = self.busy.saturating_sub(1);
            match msg {
                Msg::Search { generation, result } => {
                    if generation != self.search.generation {
                        continue;
                    }
                    self.search.loading = false;
                    self.search.searched = true;
                    match result {
                        Ok(page) => {
                            if !self.search.results_are_collections {
                                self.remember(&page.addons);
                            }
                            let got_any = !page.addons.is_empty();
                            self.search.total = page.total;
                            self.search.results.extend(page.addons);
                            let more = got_any && (self.search.results.len() as u64) < page.total;
                            self.search.next_cursor = page.next_cursor.filter(|_| more);
                        }
                        Err(e) => self.error(e),
                    }
                }
                Msg::Details { ids, result } => {
                    for id in &ids {
                        self.pending_details.remove(id);
                    }
                    match result {
                        Ok(addons) => self.remember(&addons),
                        Err(e) => {
                            self.checking = false;
                            self.error(e);
                        }
                    }
                    let required = self.resolve_requirements();
                    if required > 0 {
                        self.info(format!("Also added {required} required addon(s)"));
                        self.draft_changed(ctx);
                    } else if self.checking && self.pending_details.is_empty() {
                        self.finish_check(ctx);
                    }
                }
                Msg::Imported { id, result } => match result {
                    Ok(ids) => {
                        let added = self.add_many(ctx, ids, false);
                        self.info(format!("Imported {added} addon(s)"));
                    }
                    // A lone id that is not a collection is an addon.
                    Err(Error::NotFound(_)) => {
                        self.add_many(ctx, vec![id], true);
                    }
                    Err(e) => self.error(e),
                },
                Msg::Discovered { generation, key, result } => {
                    if generation != self.discover.generation {
                        continue;
                    }
                    self.discover.loading = false;
                    match result {
                        Ok((discovery, addons)) => {
                            self.remember(&addons);
                            save_discovery(&key, &discovery, &addons);
                            self.discover.result_from = None;
                            self.discover.cache.insert(key, discovery.clone());
                            self.discover.result = Some(discovery);
                        }
                        Err(e) => self.error(e),
                    }
                }
                Msg::Related { id, kind, result } => {
                    let found = match result {
                        Ok(addons) => {
                            self.remember(&addons);
                            addons.iter().map(|a| a.id).filter(|&other| other != id).collect()
                        }
                        Err(e) => {
                            self.error(e);
                            Vec::new()
                        }
                    };
                    let related = self.related.entry(id).or_default();
                    related.loading = false;
                    match kind {
                        RelatedKind::Author => related.author = Some(found),
                        RelatedKind::BuiltFor => related.built_for = Some(found),
                    }
                }
                Msg::KeyChecked { key, result } => {
                    if key != self.settings.api_key {
                        continue;
                    }
                    self.key_status = match result {
                        Ok(()) => KeyStatus::Valid,
                        Err(Error::InvalidKey) => {
                            KeyStatus::Invalid("Steam does not accept this key".to_string())
                        }
                        Err(e) => KeyStatus::Invalid(format!("Could not check the key: {e}")),
                    };
                }
                // Scores are a nicety; without them the app just shows none.
                Msg::Scores(result) => self.scores = result.unwrap_or_default(),
            }
        }
    }

    /// Keeps addon details, without letting keyless data (no required items,
    /// no votes) replace fuller data we already have.
    fn remember(&mut self, addons: &[Addon]) {
        for addon in addons {
            let keep_old = self
                .addons
                .get(&addon.id)
                .is_some_and(|old| old.required.is_some() && addon.required.is_none());
            if !keep_old {
                self.addons.insert(addon.id, addon.clone());
            }
        }
    }

    /// Starts a new search, or with `more` fetches the next page of this one.
    pub(crate) fn start_search(&mut self, ctx: &egui::Context, more: bool) {
        let cursor = if more {
            match self.search.next_cursor.take() {
                Some(cursor) => Some(cursor),
                None => return,
            }
        } else {
            self.search.generation += 1;
            self.search.results.clear();
            self.search.results_are_collections = self.search.collections;
            self.search.total = 0;
            self.search.next_cursor = None;
            self.search.browse = None;
            None
        };
        let collections = self.search.results_are_collections;
        // Type and category tags describe addons; collections have neither.
        let (mut tags, excluded_tags) = if collections {
            (Vec::new(), Vec::new())
        } else {
            (self.search.tags.clone(), self.search.excluded.clone())
        };
        if !collections && !self.search.include_non_addons {
            tags.push("Addon".to_string());
        }
        let query = SearchQuery {
            text: self.search.text.clone(),
            tags,
            excluded_tags,
            sort: self.search.sort,
            collections,
            containing: None,
            cursor,
            per_page: SEARCH_PAGE_SIZE,
        };
        self.search.loading = true;
        let client = self.client.clone();
        let generation = self.search.generation;
        self.spawn(ctx, async move {
            Msg::Search { generation, result: client.search(&query).await }
        });
    }

    /// Opens a collection from the search results to pick addons out of.
    pub(crate) fn browse_collection(&mut self, ctx: &egui::Context, collection: &Addon) {
        let ids = collection.required.clone().unwrap_or_default();
        self.fetch_details(ctx, ids.iter().copied().filter(|&id| self.needs_details(id)).collect());
        self.search.browse = Some(Browse { id: collection.id, title: collection.display_title(), ids });
    }

    /// With `fresh`, asks the Workshop even if the answer is already saved.
    pub(crate) fn start_discover(&mut self, ctx: &egui::Context, fresh: bool) {
        let theme = self.discover.theme.trim().to_string();
        let anchor = self.discover.anchor;
        if theme.is_empty() && anchor.is_none() {
            self.error("Say what you are building, or pick an addon to build around");
            return;
        }
        self.discover.generation += 1;
        self.discover.result_label = match (theme.is_empty(), anchor) {
            (false, Some(id)) => format!("\"{theme}\" collections that contain {}", self.title_of(id)),
            (false, None) => format!("collections matching \"{theme}\""),
            (true, Some(id)) => format!("collections that contain {}", self.title_of(id)),
            (true, None) => String::new(),
        };
        // A named idea searches for its tried-and-tested phrase.
        let theme = crate::ideas::named(&theme).map_or(theme, |idea| idea.search.to_string());
        let pages = self.settings.discover_pages.clamp(1, 5);
        let key = (theme.to_lowercase(), anchor, pages);
        self.discover.ideas_open = false;
        if fresh {
            self.discover.cache.remove(&key);
        } else if let Some(cached) = self.discover.cache.get(&key) {
            self.discover.result = Some(cached.clone());
            self.discover.loading = false;
            return;
        } else if let Some(saved) = load_discovery(&key) {
            self.remember(&saved.addons);
            self.discover.result = Some(saved.discovery);
            self.discover.result_from = Some(saved.at);
            self.discover.loading = false;
            return;
        }
        self.discover.result = None;
        self.discover.loading = true;
        let client = self.client.clone();
        let generation = self.discover.generation;
        self.spawn(ctx, async move {
            let result = async {
                let discovery = client.discover(&theme, anchor, pages).await?;
                let ids: Vec<u64> = discovery.suggestions.iter().map(|s| s.id).collect();
                let addons = client.details(&ids).await?;
                Ok((discovery, addons))
            }
            .await;
            Msg::Discovered { generation, key, result }
        });
    }

    /// Drops the discoveries held in memory, leaving the ones on disk.
    #[cfg(test)]
    pub(crate) fn forget_discoveries(&mut self) {
        self.discover.cache.clear();
    }

    /// Shows an addon in the detail panel.
    pub(crate) fn select(&mut self, ctx: &egui::Context, id: u64) {
        self.selected = Some(id);
        let mut wanted = vec![id];
        wanted.extend(self.addons.get(&id).and_then(|a| a.required.clone()).unwrap_or_default());
        wanted.retain(|&id| self.needs_details(id));
        self.fetch_details(ctx, wanted);
    }

    pub(crate) fn load_related(&mut self, ctx: &egui::Context, id: u64, kind: RelatedKind) {
        let creator = self.addons.get(&id).map_or(0, |a| a.creator);
        self.related.entry(id).or_default().loading = true;
        let client = self.client.clone();
        self.spawn(ctx, async move {
            let result = match kind {
                RelatedKind::Author => client.user_files(creator).await,
                RelatedKind::BuiltFor => {
                    let query = SearchQuery {
                        tags: vec!["Addon".to_string()],
                        sort: Sort::MostSubscribed,
                        containing: Some(id),
                        per_page: 30,
                        ..Default::default()
                    };
                    client.search(&query).await.map(|page| page.addons)
                }
            };
            Msg::Related { id, kind, result }
        });
    }

    /// True if we have nothing on this addon, or only keyless data that a key
    /// could now complete with required items.
    pub(crate) fn needs_details(&self, id: u64) -> bool {
        match self.addons.get(&id) {
            None => true,
            Some(addon) => {
                self.client.has_key() && addon.required.is_none() && addon.status == AddonStatus::Ok
            }
        }
    }

    pub(crate) fn fetch_details(&mut self, ctx: &egui::Context, ids: Vec<u64>) {
        let ids: Vec<u64> = ids.into_iter().filter(|id| self.pending_details.insert(*id)).collect();
        if ids.is_empty() {
            return;
        }
        let client = self.client.clone();
        self.spawn(ctx, async move {
            let result = client.details(&ids).await;
            Msg::Details { ids, result }
        });
    }

    // ---- Editing the draft ----

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
    fn resolve_requirements(&mut self) -> usize {
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

    // ---- Checking ----

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

    fn finish_check(&mut self, ctx: &egui::Context) {
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

    // ---- Vouches ----

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

    fn fetch_scores(&mut self, ctx: &egui::Context) {
        if COMMUNITY_REPO.is_empty() || !self.settings.community {
            return;
        }
        self.spawn(ctx, async { Msg::Scores(community::fetch_scores(COMMUNITY_REPO).await) });
    }

    // ---- Settings ----

    /// Call after changing how the app looks.
    pub(crate) fn look_changed(&mut self, ctx: &egui::Context) {
        crate::theme::apply(ctx, &self.settings.look);
        self.write_settings();
    }

    pub(crate) fn save_settings(&mut self, ctx: &egui::Context) {
        self.settings.api_key = self.key_input.trim().to_string();
        self.settings.gmod_dir = self.gmod_input.trim().to_string();
        self.client = SteamClient::new(Some(self.settings.api_key.clone()));
        self.gmod_dir = local::find_gmod(&self.settings.gmod_dir);
        self.installed = self.gmod_dir.as_deref().map(local::installed_ids).unwrap_or_default();
        self.archives.clear();
        self.write_settings();
        self.key_status = KeyStatus::Unknown;
        if self.client.has_key() {
            self.key_status = KeyStatus::Checking;
            let client = self.client.clone();
            let key = self.settings.api_key.clone();
            self.spawn(ctx, async move {
                Msg::KeyChecked { key, result: client.validate_key().await }
            });
        }
    }

    pub(crate) fn write_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.error(format!("Could not save settings: {e}"));
        }
    }

    // ---- The draft library ----

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

    fn load_draft(&mut self, ctx: &egui::Context, path: PathBuf) {
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

fn load_votes() -> Votes {
    Settings::votes_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn discovery_path(key: &DiscoverKey) -> Option<PathBuf> {
    let (theme, anchor, pages) = key;
    let name: String =
        theme.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(60).collect();
    Some(Settings::cache_dir()?.join(format!("{name}_{}_{pages}.json", anchor.unwrap_or(0))))
}

fn load_discovery(key: &DiscoverKey) -> Option<SavedDiscovery> {
    let text = std::fs::read_to_string(discovery_path(key)?).ok()?;
    let saved: SavedDiscovery = serde_json::from_str(&text).ok()?;
    (now_unix().saturating_sub(saved.at) < DISCOVERY_KEPT).then_some(saved)
}

/// Best effort: a discovery that cannot be saved is simply asked for again.
fn save_discovery(key: &DiscoverKey, discovery: &Discovery, addons: &[Addon]) {
    let Some(path) = discovery_path(key) else { return };
    let saved = SavedDiscovery { at: now_unix(), discovery: discovery.clone(), addons: addons.to_vec() };
    if let (Some(dir), Ok(text)) = (path.parent(), serde_json::to_string(&saved)) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(path, text);
    }
}

fn new_library_path() -> Option<PathBuf> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    Some(Settings::drafts_dir()?.join(format!("draft-{millis}.json")))
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages(ctx);
        self.autosave(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn on_exit(&mut self) {
        self.save_now();
    }
}
