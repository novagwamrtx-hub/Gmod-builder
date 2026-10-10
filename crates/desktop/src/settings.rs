use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::theme::Look;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub api_key: String,
    /// Adding an addon also adds the items it lists as required.
    #[serde(default = "yes")]
    pub auto_requirements: bool,
    /// The `garrysmod` folder, if the user had to point at it; empty to
    /// look in the usual places.
    #[serde(default)]
    pub gmod_dir: String,
    /// The draft that was open when the app last closed.
    #[serde(default)]
    pub last_draft: Option<PathBuf>,
    #[serde(default)]
    pub look: Look,
    /// How many hundred collections a discovery reads.
    #[serde(default = "three")]
    pub discover_pages: usize,
    /// Suggestions shown per addon type.
    #[serde(default = "ten")]
    pub discover_per_type: usize,
    /// Check flags addons not updated for this many years; 0 for never.
    #[serde(default = "eight")]
    pub stale_after_years: u64,
    /// Show vouch scores and offer to share votes.
    #[serde(default = "yes")]
    pub community: bool,
}

fn yes() -> bool {
    true
}

fn three() -> usize {
    3
}

fn ten() -> usize {
    10
}

fn eight() -> u64 {
    8
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            auto_requirements: true,
            gmod_dir: String::new(),
            last_draft: None,
            look: Look::default(),
            discover_pages: three(),
            discover_per_type: ten(),
            stale_after_years: eight(),
            community: true,
        }
    }
}

impl Settings {
    pub fn path() -> Option<PathBuf> {
        dirs().map(|dirs| dirs.config_dir().join("settings.json"))
    }

    /// Where drafts are kept unless the user saves one elsewhere.
    pub fn drafts_dir() -> Option<PathBuf> {
        dirs().map(|dirs| dirs.data_dir().join("drafts"))
    }

    /// Where discoveries are kept so repeating one is instant.
    pub fn cache_dir() -> Option<PathBuf> {
        dirs().map(|dirs| dirs.cache_dir().join("discoveries"))
    }

    /// The user's own vouches and warnings.
    pub fn votes_path() -> Option<PathBuf> {
        dirs().map(|dirs| dirs.data_dir().join("vouches.json"))
    }

    /// Missing or unreadable settings are treated as a first run.
    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::path().ok_or_else(|| std::io::Error::other("no config directory"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)
    }
}

fn dirs() -> Option<directories::ProjectDirs> {
    directories::ProjectDirs::from("", "", "gmod-collection-builder")
}
