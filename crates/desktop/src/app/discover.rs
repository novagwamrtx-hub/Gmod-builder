//! Discovering addons from collections like the one being built, and keeping
//! the answers so that asking again is instant.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use eframe::egui;
use gmcb_core::discover::Discovery;
use gmcb_core::{Addon, Kind};

use super::{App, Msg};
use crate::format::now_unix;
use crate::settings::Settings;

/// A saved discovery is reused for this long before the Workshop is asked again.
const DISCOVERY_KEPT: u64 = 3 * 24 * 60 * 60;

/// Lower-cased theme, the addon the search was built around, and how many
/// hundred collections were read.
pub(super) type DiscoverKey = (String, Option<u64>, usize);

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
    pub(super) cache: HashMap<DiscoverKey, Discovery>,
    pub(super) generation: u64,
}

impl App {
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
pub(super) fn save_discovery(key: &DiscoverKey, discovery: &Discovery, addons: &[Addon]) {
    let Some(path) = discovery_path(key) else { return };
    let saved = SavedDiscovery { at: now_unix(), discovery: discovery.clone(), addons: addons.to_vec() };
    if let (Some(dir), Ok(text)) = (path.parent(), serde_json::to_string(&saved)) {
        let _ = std::fs::create_dir_all(dir);
        let _ = std::fs::write(path, text);
    }
}
