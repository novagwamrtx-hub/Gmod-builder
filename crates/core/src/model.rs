use serde::{Deserialize, Serialize};

pub const GMOD_APPID: u32 = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddonStatus {
    Ok,
    /// Deleted, or private to the point the API will not return it.
    Removed,
    Banned,
    /// Friends-only or private: other players and servers cannot download it.
    Hidden,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Addon {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub tags: Vec<String>,
    pub size_bytes: u64,
    pub time_created: u64,
    pub time_updated: u64,
    pub subscriptions: u64,
    pub favorited: u64,
    pub preview_url: Option<String>,
    /// Workshop "Required items". `None` when the data came from the keyless
    /// endpoint, which does not report them.
    pub required: Option<Vec<u64>>,
    pub status: AddonStatus,
    /// Zero when the endpoint that produced this did not report votes.
    #[serde(default)]
    pub votes_up: u64,
    #[serde(default)]
    pub votes_down: u64,
    /// SteamID64 of the author; 0 if unknown.
    #[serde(default)]
    pub creator: u64,
}

/// What an addon is, from the one type tag every GMod addon carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Kind {
    Gamemode,
    Map,
    Weapon,
    Npc,
    Vehicle,
    Entity,
    Tool,
    Model,
    Effects,
    ServerContent,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 11] = [
        Kind::Gamemode,
        Kind::Map,
        Kind::Weapon,
        Kind::Npc,
        Kind::Vehicle,
        Kind::Entity,
        Kind::Tool,
        Kind::Model,
        Kind::Effects,
        Kind::ServerContent,
        Kind::Other,
    ];

    /// The Workshop tag, lower case. Empty for [`Kind::Other`].
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Gamemode => "gamemode",
            Kind::Map => "map",
            Kind::Weapon => "weapon",
            Kind::Npc => "npc",
            Kind::Vehicle => "vehicle",
            Kind::Entity => "entity",
            Kind::Tool => "tool",
            Kind::Model => "model",
            Kind::Effects => "effects",
            Kind::ServerContent => "servercontent",
            Kind::Other => "",
        }
    }

    /// Plural heading, e.g. for grouping a collection.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Gamemode => "Gamemodes",
            Kind::Map => "Maps",
            Kind::Weapon => "Weapons",
            Kind::Npc => "NPCs",
            Kind::Vehicle => "Vehicles",
            Kind::Entity => "Entities",
            Kind::Tool => "Tools",
            Kind::Model => "Models",
            Kind::Effects => "Effects",
            Kind::ServerContent => "Server content",
            Kind::Other => "Other",
        }
    }
}

impl Addon {
    /// Placeholder for an id Steam would not give details for.
    pub fn unavailable(id: u64, status: AddonStatus) -> Self {
        Self {
            id,
            title: String::new(),
            description: String::new(),
            tags: Vec::new(),
            size_bytes: 0,
            time_created: 0,
            time_updated: 0,
            subscriptions: 0,
            favorited: 0,
            preview_url: None,
            required: None,
            status,
            votes_up: 0,
            votes_down: 0,
            creator: 0,
        }
    }

    /// Workshop tags are inconsistently cased ("Addon", "tool", "Fun").
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }

    pub fn kind(&self) -> Kind {
        Kind::ALL
            .into_iter()
            .find(|kind| *kind != Kind::Other && self.has_tag(kind.tag()))
            .unwrap_or(Kind::Other)
    }

    /// Share of votes that are positive, 0 to 100. `None` with too few votes
    /// to mean anything.
    pub fn rating(&self) -> Option<u32> {
        let total = self.votes_up + self.votes_down;
        (total >= 10).then(|| (self.votes_up * 100 / total) as u32)
    }

    pub fn url(&self) -> String {
        workshop_url(self.id)
    }

    /// Title, or a stand-in for addons Steam returned no title for.
    pub fn display_title(&self) -> String {
        if self.title.is_empty() {
            format!("Addon {}", self.id)
        } else {
            self.title.clone()
        }
    }
}

pub fn workshop_url(id: u64) -> String {
    format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}")
}

/// The list a user is building, saved to disk as JSON by the desktop app.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CollectionDraft {
    pub name: String,
    /// The Steam collection this draft is meant to fill, once it exists.
    pub collection_id: Option<u64>,
    pub ids: Vec<u64>,
    #[serde(default)]
    pub notes: String,
    /// Addons being considered but not in the collection.
    #[serde(default)]
    pub maybe: Vec<u64>,
    /// Keys of check findings the user chose to ignore (`Finding::key`).
    #[serde(default)]
    pub dismissed: Vec<String>,
    /// What the addons looked like when last reviewed, to show what changed.
    #[serde(default)]
    pub snapshot: Vec<Snap>,
    /// Unix time the snapshot was taken; 0 if never.
    #[serde(default)]
    pub snapshot_at: u64,
    /// Gamemode the collection is for, if the user said (`analysis::TARGETS`).
    #[serde(default)]
    pub target: Option<String>,
}

/// The little we remember about an addon between checks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snap {
    pub id: u64,
    pub title: String,
    pub time_updated: u64,
    pub status: AddonStatus,
}

impl CollectionDraft {
    /// Returns false if the addon was already in the draft.
    pub fn add(&mut self, id: u64) -> bool {
        if self.ids.contains(&id) {
            return false;
        }
        self.ids.push(id);
        self.maybe.retain(|&x| x != id);
        true
    }

    /// Title an addon had when last reviewed; survives its removal from Steam.
    pub fn remembered_title(&self, id: u64) -> Option<&str> {
        self.snapshot
            .iter()
            .find(|snap| snap.id == id && !snap.title.is_empty())
            .map(|snap| snap.title.as_str())
    }

    pub fn remove(&mut self, id: u64) {
        self.ids.retain(|&x| x != id);
    }

    pub fn contains(&self, id: u64) -> bool {
        self.ids.contains(&id)
    }
}

/// Accepts a bare id or any Workshop URL carrying `?id=`.
pub fn parse_workshop_id(input: &str) -> Option<u64> {
    let input = input.trim();
    if !input.is_empty() && input.bytes().all(|b| b.is_ascii_digit()) {
        return input.parse().ok();
    }
    input.split(['?', '&', '#']).find_map(|part| part.strip_prefix("id=")).and_then(|v| {
        let digits: String = v.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids_and_urls() {
        assert_eq!(parse_workshop_id(" 160250458 "), Some(160250458));
        assert_eq!(
            parse_workshop_id("https://steamcommunity.com/sharedfiles/filedetails/?id=160250458"),
            Some(160250458)
        );
        assert_eq!(
            parse_workshop_id(
                "https://steamcommunity.com/workshop/filedetails/?l=english&id=42&searchtext=x"
            ),
            Some(42)
        );
        assert_eq!(parse_workshop_id("https://steamcommunity.com/"), None);
        assert_eq!(parse_workshop_id(""), None);
    }

    #[test]
    fn draft_does_not_duplicate() {
        let mut draft = CollectionDraft::default();
        assert!(draft.add(1));
        assert!(!draft.add(1));
        draft.add(2);
        draft.remove(1);
        assert_eq!(draft.ids, vec![2]);
    }

    #[test]
    fn adding_takes_an_addon_off_the_maybe_pile() {
        let mut draft = CollectionDraft { maybe: vec![5, 6], ..Default::default() };
        draft.add(5);
        assert_eq!(draft.maybe, vec![6]);
    }

    #[test]
    fn kind_and_rating() {
        let mut addon = Addon::unavailable(1, AddonStatus::Ok);
        assert_eq!(addon.kind(), Kind::Other);
        assert_eq!(addon.rating(), None);
        addon.tags = vec!["Addon".into(), "Fun".into(), "NPC".into()];
        addon.votes_up = 90;
        addon.votes_down = 10;
        assert_eq!(addon.kind(), Kind::Npc);
        assert_eq!(addon.rating(), Some(90));
    }
}
