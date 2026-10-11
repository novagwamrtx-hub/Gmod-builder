//! Application state and background work. Each area of the app has its own
//! file in this folder; what is drawn on screen lives in `ui`.

mod check;
mod discover;
mod draft;
mod search;
mod vouches;

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

use eframe::egui;
use gmcb_core::analysis::Report;
use gmcb_core::changes::Change;
use gmcb_core::community::{Scores, Votes};
use gmcb_core::discover::Discovery;
use gmcb_core::gma::Archive;
use gmcb_core::{
    updates, Addon, AddonStatus, CollectionDraft, Error, SearchPage, SearchQuery, Sort, SteamClient,
};

use crate::files;
use crate::gmod::{self, Installed};
use crate::settings::Settings;
use crate::thumbs::{self, Thumbnails};

use discover::DiscoverKey;
pub use discover::DiscoverState;
pub use draft::{DraftView, LibraryEntry};
pub use search::SearchState;

/// The project's GitHub repository ("owner/name"): where new versions are
/// released, where shared vouches are sent, and where the scores made from
/// them are published.
pub const PROJECT_REPO: &str = "novagwamrtx-hub/Gmod-builder";

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
    Search {
        generation: u64,
        result: Result<SearchPage, Error>,
    },
    Details {
        ids: Vec<u64>,
        result: Result<Vec<Addon>, Error>,
    },
    Imported {
        id: u64,
        result: Result<Vec<u64>, Error>,
    },
    Discovered {
        generation: u64,
        key: DiscoverKey,
        result: Result<(Discovery, Vec<Addon>), Error>,
    },
    Related {
        id: u64,
        kind: RelatedKind,
        result: Result<Vec<Addon>, Error>,
    },
    KeyChecked {
        key: String,
        result: Result<(), Error>,
    },
    Scores(Result<Scores, String>),
    /// The tag of the project's latest release, if it could be asked.
    LatestRelease(Option<String>),
}

#[derive(Default)]
pub struct Related {
    pub author: Option<Vec<u64>>,
    pub built_for: Option<Vec<u64>>,
    pub loading: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub enum KeyStatus {
    Unknown,
    Checking,
    Valid,
    Invalid(String),
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
    /// Only draft addons whose name contains this are listed.
    pub(crate) draft_filter: String,

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
    pub(crate) installed: Installed,
    pub(crate) archives: HashMap<u64, Archive>,

    /// The user's own vouches (true) and warnings (false).
    pub(crate) votes: Votes,
    /// The verdicts that have been sent to the project, as they were sent.
    pub(crate) shared: Votes,
    /// Everyone's totals, as published by the project.
    pub(crate) scores: Scores,
    /// The tag of a release newer than this build, once one is known.
    pub(crate) update: Option<String>,
    pub(crate) thumbnails: Arc<Thumbnails>,

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
        let gmod_dir = gmod::find(&settings.gmod_dir);
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to start the async runtime");
        let thumbnails = Arc::new(Thumbnails::new(rt.handle().clone(), Settings::thumbnails_dir()));
        thumbnails.install(ctx);
        rt.spawn_blocking(tidy_caches);
        let (votes, shared) = vouches::load_votes();
        let mut app = Self {
            rt,
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
            draft_filter: String::new(),
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
            installed: gmod_dir.as_deref().map(gmod::installed).unwrap_or_default(),
            gmod_dir,
            archives: HashMap::new(),
            votes,
            shared,
            scores: Scores::new(),
            update: None,
            thumbnails,
            tab_changed: 0.0,
            shown_tab: None,
            last_selected: None,
            busy: 0,
            status: None,
            settings,
        };
        crate::ui::theme::apply(ctx, &app.settings.look);
        app.fetch_scores(ctx);
        app.look_for_update(ctx);
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
                            discover::save_discovery(&key, &discovery, &addons);
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
                Msg::LatestRelease(tag) => {
                    self.update = tag.filter(|tag| updates::is_newer(tag, env!("CARGO_PKG_VERSION")));
                }
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

    /// Asks GitHub whether a newer version has been released. Not being
    /// able to ask is not worth telling the user about.
    fn look_for_update(&mut self, ctx: &egui::Context) {
        if self.settings.check_updates {
            self.spawn(ctx, async { Msg::LatestRelease(updates::latest(PROJECT_REPO).await.ok().flatten()) });
        }
    }

    // ---- Settings ----

    /// Call after changing how the app looks.
    pub(crate) fn look_changed(&mut self, ctx: &egui::Context) {
        crate::ui::theme::apply(ctx, &self.settings.look);
        self.write_settings();
    }

    pub(crate) fn save_settings(&mut self, ctx: &egui::Context) {
        self.settings.api_key = self.key_input.trim().to_string();
        self.settings.gmod_dir = self.gmod_input.trim().to_string();
        self.client = SteamClient::new(Some(self.settings.api_key.clone()));
        self.gmod_dir = gmod::find(&self.settings.gmod_dir);
        self.installed = self.gmod_dir.as_deref().map(gmod::installed).unwrap_or_default();
        self.archives.clear();
        self.write_settings();
        self.key_status = KeyStatus::Unknown;
        if self.client.has_key() {
            self.key_status = KeyStatus::Checking;
            let client = self.client.clone();
            let key = self.settings.api_key.clone();
            self.spawn(ctx, async move { Msg::KeyChecked { key, result: client.validate_key().await } });
        }
    }

    pub(crate) fn write_settings(&mut self) {
        if let Err(e) = self.settings.save() {
            self.error(format!("Could not save settings: {e}"));
        }
    }
}

/// Deletes cached files that have outlived their use. Runs in the
/// background when the app starts.
fn tidy_caches() {
    if let Some(dir) = Settings::thumbnails_dir() {
        files::prune(&dir, thumbs::KEPT_ON_DISK, thumbs::DISK_BUDGET);
    }
    if let Some(dir) = Settings::discoveries_dir() {
        files::prune(&dir, discover::DISCOVERY_KEPT, u64::MAX);
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages(ctx);
        self.autosave(ctx);
        self.thumbnails.trim(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    fn on_exit(&mut self) {
        self.save_now();
    }
}
