//! JSON shapes returned by Steam, and their conversion to our model.
//!
//! The two API families disagree on types (`file_size` is a string in one and
//! a number in the other, `banned` is a bool or 0/1), so fields are read
//! leniently.

use serde::{Deserialize, Deserializer};

use crate::model::{Addon, AddonStatus};

const RESULT_OK: u32 = 1;
const FILE_TYPE_COLLECTION: u32 = 2;
const VISIBILITY_FRIENDS_ONLY: u32 = 1;
const VISIBILITY_PRIVATE: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct SearchPage {
    /// Total matches across all pages (0 for plain detail lookups).
    pub total: u64,
    pub addons: Vec<Addon>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectionChild {
    pub id: u64,
    /// A collection linked inside this collection.
    pub is_collection: bool,
}

#[derive(Deserialize)]
struct Envelope<T> {
    response: T,
}

#[derive(Deserialize)]
struct FileList {
    #[serde(default)]
    total: u64,
    #[serde(default)]
    publishedfiledetails: Vec<RawFile>,
    #[serde(default)]
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
struct RawFile {
    #[serde(deserialize_with = "lenient_u64")]
    publishedfileid: u64,
    #[serde(default = "result_ok")]
    result: u32,
    #[serde(default)]
    title: String,
    #[serde(default)]
    short_description: Option<String>,
    #[serde(default)]
    file_description: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default, deserialize_with = "lenient_u64")]
    file_size: u64,
    #[serde(default)]
    preview_url: Option<String>,
    #[serde(default)]
    time_created: u64,
    #[serde(default)]
    time_updated: u64,
    #[serde(default)]
    visibility: u32,
    #[serde(default, deserialize_with = "lenient_bool")]
    banned: bool,
    #[serde(default)]
    subscriptions: u64,
    #[serde(default)]
    favorited: u64,
    #[serde(default)]
    tags: Vec<RawTag>,
    /// Absent from the keyless endpoint, and from keyed ones when empty.
    #[serde(default)]
    children: Option<Vec<RawChild>>,
    /// Only the keyed endpoints send this; its presence tells the two apart.
    #[serde(default)]
    file_type: Option<u32>,
    #[serde(default)]
    vote_data: Option<RawVotes>,
    #[serde(default, deserialize_with = "lenient_u64")]
    creator: u64,
}

#[derive(Deserialize)]
struct RawVotes {
    #[serde(default)]
    votes_up: u64,
    #[serde(default)]
    votes_down: u64,
}

#[derive(Deserialize)]
struct RawTag {
    tag: String,
}

#[derive(Deserialize)]
struct RawChild {
    #[serde(deserialize_with = "lenient_u64")]
    publishedfileid: u64,
    #[serde(default, alias = "filetype")]
    file_type: u32,
}

#[derive(Deserialize)]
struct CollectionList {
    #[serde(default)]
    collectiondetails: Vec<RawCollection>,
}

#[derive(Deserialize)]
struct RawCollection {
    #[serde(default = "result_ok")]
    result: u32,
    #[serde(default)]
    children: Vec<RawChild>,
}

fn result_ok() -> u32 {
    RESULT_OK
}

impl From<RawFile> for Addon {
    fn from(raw: RawFile) -> Self {
        let status = if raw.result != RESULT_OK {
            AddonStatus::Removed
        } else if raw.banned {
            AddonStatus::Banned
        } else if matches!(raw.visibility, VISIBILITY_FRIENDS_ONLY | VISIBILITY_PRIVATE) {
            AddonStatus::Hidden
        } else {
            AddonStatus::Ok
        };
        // Keyed endpoints omit `children` when there are none, so an absent
        // list only means "unknown" if the response is the keyless kind.
        let required = match (raw.children, raw.file_type) {
            (Some(children), _) => Some(children.into_iter().map(|c| c.publishedfileid).collect()),
            (None, Some(_)) => Some(Vec::new()),
            (None, None) => None,
        };
        Addon {
            id: raw.publishedfileid,
            title: raw.title,
            description: raw
                .short_description
                .or(raw.file_description)
                .or(raw.description)
                .unwrap_or_default(),
            tags: raw.tags.into_iter().map(|t| t.tag).collect(),
            size_bytes: raw.file_size,
            time_created: raw.time_created,
            time_updated: raw.time_updated,
            subscriptions: raw.subscriptions,
            favorited: raw.favorited,
            preview_url: raw.preview_url.filter(|u| !u.is_empty()),
            required,
            status,
            votes_up: raw.vote_data.as_ref().map_or(0, |v| v.votes_up),
            votes_down: raw.vote_data.as_ref().map_or(0, |v| v.votes_down),
            creator: raw.creator,
        }
    }
}

/// Parses `QueryFiles`, `GetDetails` and `GetPublishedFileDetails` responses.
pub fn parse_file_list(json: &str) -> Result<SearchPage, serde_json::Error> {
    let list = serde_json::from_str::<Envelope<FileList>>(json)?.response;
    Ok(SearchPage {
        total: list.total,
        addons: list.publishedfiledetails.into_iter().map(Addon::from).collect(),
        next_cursor: list.next_cursor.filter(|c| !c.is_empty()),
    })
}

/// Parses a single-collection `GetCollectionDetails` response. `None` means
/// Steam has no such collection.
pub fn parse_collection(json: &str) -> Result<Option<Vec<CollectionChild>>, serde_json::Error> {
    let list = serde_json::from_str::<Envelope<CollectionList>>(json)?.response;
    Ok(list
        .collectiondetails
        .into_iter()
        .next()
        .filter(|c| c.result == RESULT_OK)
        .map(|c| {
            c.children
                .into_iter()
                .map(|child| CollectionChild {
                    id: child.publishedfileid,
                    is_collection: child.file_type == FILE_TYPE_COLLECTION,
                })
                .collect()
        }))
}

#[derive(Deserialize)]
#[serde(untagged)]
enum NumOrStr {
    Num(u64),
    Str(String),
}

fn lenient_u64<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    match NumOrStr::deserialize(d)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => s.parse().map_err(serde::de::Error::custom),
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum BoolOrNum {
    Bool(bool),
    Num(u64),
}

fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    Ok(match BoolOrNum::deserialize(d)? {
        BoolOrNum::Bool(b) => b,
        BoolOrNum::Num(n) => n != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Recorded from the live keyless endpoints.
    const KEYLESS: &str = include_str!("../../tests/fixtures/file_details_keyless.json");
    const COLLECTION: &str = include_str!("../../tests/fixtures/collection_details.json");
    // Hand-written to the documented QueryFiles shape; replace with a recorded
    // response once a key is available.
    const QUERY: &str = include_str!("../../tests/fixtures/query_files.json");

    #[test]
    fn keyless_details() {
        let page = parse_file_list(KEYLESS).unwrap();
        assert_eq!(page.addons.len(), 2);

        let wiremod = &page.addons[0];
        assert_eq!(wiremod.id, 160250458);
        assert_eq!(wiremod.title, "Wiremod");
        assert_eq!(wiremod.size_bytes, 20686053);
        assert_eq!(wiremod.status, AddonStatus::Ok);
        assert!(wiremod.has_tag("Tool"));
        assert_eq!(wiremod.required, None);
        assert!(wiremod.preview_url.is_some());

        assert_eq!(page.addons[1].id, 1);
        assert_eq!(page.addons[1].status, AddonStatus::Removed);
    }

    #[test]
    fn collection_children() {
        let children = parse_collection(COLLECTION).unwrap().unwrap();
        assert!(!children.is_empty());
        assert_eq!(children[0].id, 3812203759);
        assert!(!children[0].is_collection);
    }

    #[test]
    fn missing_collection() {
        let json = r#"{"response":{"result":1,"resultcount":1,"collectiondetails":[{"publishedfileid":"1","result":9}]}}"#;
        assert_eq!(parse_collection(json).unwrap(), None);
    }

    #[test]
    fn query_page() {
        let page = parse_file_list(QUERY).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.next_cursor.as_deref(), Some("AoJ4abc"));

        let with_deps = &page.addons[0];
        assert_eq!(with_deps.required, Some(vec![160250458]));
        assert_eq!(with_deps.description, "Needs Wiremod.");

        assert_eq!((with_deps.votes_up, with_deps.votes_down), (40, 2));
        assert_eq!(with_deps.creator, 76561197960000001);

        let plain = &page.addons[1];
        assert_eq!(plain.votes_up, 0);
        assert_eq!(plain.required, Some(vec![]));
        assert_eq!(plain.status, AddonStatus::Banned);
    }
}
