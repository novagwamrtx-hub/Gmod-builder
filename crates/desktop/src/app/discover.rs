//! Discovering addons from collections like the one being built, and keeping
//! the answers so that asking again is instant.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use eframe::egui;
use gmcb_core::discover::Discovery;
use gmcb_core::{Addon, Kind};

use super::{App, Msg};
use crate::files;
use crate::format::now_unix;
use crate::settings::Settings;

/// A saved discovery is reused for this long before the Workshop is asked again.
pub(super) const DISCOVERY_KEPT: Duration = Duration::from_secs(3 * 24 * 60 * 60);

/// Lower-cased theme, the addon the search was built around, and how many
/// hundred collections were read.
pub(super) type DiscoverKey = (String, Option<u64>, usize);

/// A discovery as kept on disk.
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedDiscovery {
    /// What was asked, to be sure the file answers the same question.
    key: DiscoverKey,
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

/// The file name starts with the theme, for anyone looking in the folder.
/// What tells two questions apart is the number after it: themes in other
/// alphabets all read "-----" otherwise.
fn discovery_file(key: &DiscoverKey) -> String {
    let (theme, anchor, pages) = key;
    let readable: String =
        theme.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(40).collect();
    let question = format!("{theme}\n{}\n{pages}", anchor.unwrap_or(0));
    format!("{readable}-{:016x}.json", files::fnv1a(question.as_bytes()))
}

fn load_discovery(key: &DiscoverKey) -> Option<SavedDiscovery> {
    let path = Settings::discoveries_dir()?.join(discovery_file(key));
    let saved: SavedDiscovery = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let fresh = now_unix().saturating_sub(saved.at) < DISCOVERY_KEPT.as_secs();
    (fresh && saved.key == *key).then_some(saved)
}

/// Best effort: a discovery that cannot be saved is simply asked for again.
pub(super) fn save_discovery(key: &DiscoverKey, discovery: &Discovery, addons: &[Addon]) {
    let Some(dir) = Settings::discoveries_dir() else { return };
    let saved = SavedDiscovery {
        key: key.clone(),
        at: now_unix(),
        discovery: discovery.clone(),
        addons: addons.to_vec(),
    };
    if let Ok(text) = serde_json::to_string(&saved) {
        let _ = files::write(&dir.join(discovery_file(key)), &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_question_gets_its_own_file() {
        let key = |theme: &str, anchor, pages| (theme.to_string(), anchor, pages);
        let zombie = discovery_file(&key("zombie survival", None, 3));
        assert!(zombie.starts_with("zombie-survival-") && zombie.ends_with(".json"), "{zombie}");
        assert_eq!(zombie, discovery_file(&key("zombie survival", None, 3)));
        assert_ne!(zombie, discovery_file(&key("zombie survival", None, 5)));
        assert_ne!(zombie, discovery_file(&key("zombie survival", Some(104482086), 3)));
        assert_ne!(zombie, discovery_file(&key("zombie-survival", None, 3)));
        assert_ne!(discovery_file(&key("зомби", None, 3)), discovery_file(&key("войны", None, 3)));
        let long = discovery_file(&key(&"a".repeat(500), None, 3));
        assert!(long.len() < 80, "{long}");
    }
}
