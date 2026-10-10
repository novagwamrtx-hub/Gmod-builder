//! Steam Web API client for Workshop data.
//!
//! Search and required-item lookups go through `IPublishedFileService`, which
//! needs a Web API key. Collection contents and basic file details go through
//! `ISteamRemoteStorage`, which does not.

mod wire;

use std::collections::{HashMap, HashSet, VecDeque};

use futures_util::stream::{self, StreamExt};

use crate::discover::{self, Discovery};
use crate::model::{Addon, AddonStatus, GMOD_APPID};

pub use wire::{parse_collection, parse_file_list, CollectionChild, SearchPage};

const API: &str = "https://api.steampowered.com";
/// Both details endpoints accept many ids per call; stay well under URL limits.
const DETAILS_CHUNK: usize = 100;
const MAX_NESTED_COLLECTIONS: usize = 50;
/// Collections sampled for a discovery: this many pages of 100.
const DISCOVER_CANDIDATES: usize = 80;
const PARALLEL_REQUESTS: usize = 8;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("unexpected response from Steam: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("a Steam Web API key is required for this")]
    MissingKey,
    #[error("Steam rejected the Web API key")]
    InvalidKey,
    #[error("Steam is rate limiting requests, try again shortly")]
    RateLimited,
    #[error("Steam returned HTTP {0}")]
    Status(u16),
    #[error("no collection with id {0}")]
    NotFound(u64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Text relevance when there is search text, trending otherwise.
    #[default]
    Relevance,
    Trending,
    MostSubscribed,
    TopRated,
    Newest,
    RecentlyUpdated,
}

impl Sort {
    pub const ALL: [Sort; 6] = [
        Sort::Relevance,
        Sort::Trending,
        Sort::MostSubscribed,
        Sort::TopRated,
        Sort::Newest,
        Sort::RecentlyUpdated,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Relevance => "Relevance",
            Sort::Trending => "Trending",
            Sort::MostSubscribed => "Most subscribed",
            Sort::TopRated => "Top rated",
            Sort::Newest => "Newest",
            Sort::RecentlyUpdated => "Recently updated",
        }
    }

    /// `EPublishedFileQueryType`
    fn query_type(self, has_text: bool) -> u32 {
        match self {
            Sort::Relevance if has_text => 12,
            Sort::Relevance | Sort::Trending => 3,
            Sort::MostSubscribed => 9,
            Sort::TopRated => 0,
            Sort::Newest => 1,
            Sort::RecentlyUpdated => 21,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    pub text: String,
    /// Addons must carry every one of these tags.
    pub tags: Vec<String>,
    pub sort: Sort,
    /// Addons must carry none of these.
    pub excluded_tags: Vec<String>,
    /// Search collections instead of addons. A collection comes back as an
    /// [`Addon`] whose `required` lists its members.
    pub collections: bool,
    /// Only results that reference this item: collections containing it, or
    /// addons listing it as required.
    pub containing: Option<u64>,
    /// `next_cursor` of the previous page; `None` for the first page.
    pub cursor: Option<String>,
    pub per_page: u32,
}

impl SearchQuery {
    fn params(&self, key: &str) -> Vec<(String, String)> {
        let text = self.text.trim();
        let query_type = self.sort.query_type(!text.is_empty());
        let per_page = if self.per_page == 0 { 30 } else { self.per_page.min(100) };
        let mut p = vec![
            ("key".to_string(), key.to_string()),
            ("appid".to_string(), GMOD_APPID.to_string()),
            ("query_type".to_string(), query_type.to_string()),
            ("cursor".to_string(), self.cursor.clone().unwrap_or_else(|| "*".into())),
            ("numperpage".to_string(), per_page.to_string()),
            ("filetype".to_string(), if self.collections { "1" } else { "0" }.to_string()),
            ("match_all_tags".to_string(), "true".to_string()),
            ("return_tags".to_string(), "true".to_string()),
            ("return_children".to_string(), "true".to_string()),
            ("return_vote_data".to_string(), "true".to_string()),
            ("return_short_description".to_string(), "true".to_string()),
            ("strip_description_bbcode".to_string(), "true".to_string()),
        ];
        if !text.is_empty() {
            p.push(("search_text".to_string(), text.to_string()));
        }
        if query_type == 3 {
            p.push(("days".to_string(), "7".to_string()));
        }
        for (i, tag) in self.tags.iter().enumerate() {
            p.push((format!("requiredtags[{i}]"), tag.clone()));
        }
        for (i, tag) in self.excluded_tags.iter().enumerate() {
            p.push((format!("excludedtags[{i}]"), tag.clone()));
        }
        if let Some(id) = self.containing {
            p.push(("child_publishedfileid".to_string(), id.to_string()));
        }
        p
    }
}

#[derive(Clone)]
pub struct SteamClient {
    http: reqwest::Client,
    key: Option<String>,
}

impl SteamClient {
    pub fn new(key: Option<String>) -> Self {
        let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
        Self { http: reqwest::Client::new(), key }
    }

    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }

    fn key(&self) -> Result<&str, Error> {
        self.key.as_deref().ok_or(Error::MissingKey)
    }

    /// Search the GMod Workshop. Needs a key.
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchPage, Error> {
        let params = query.params(self.key()?);
        let url = format!("{API}/IPublishedFileService/QueryFiles/v1/");
        let body = read(self.http.get(url).query(&params).send().await?).await?;
        Ok(parse_file_list(&body)?)
    }

    /// How many results a search has, without fetching any.
    pub async fn count(&self, query: &SearchQuery) -> Result<u64, Error> {
        let mut params = query.params(self.key()?);
        params.push(("totalonly".to_string(), "true".to_string()));
        let url = format!("{API}/IPublishedFileService/QueryFiles/v1/");
        let body = read(self.http.get(url).query(&params).send().await?).await?;
        Ok(parse_file_list(&body)?.total)
    }

    /// Asks Steam something trivial to find out whether the key is accepted.
    pub async fn validate_key(&self) -> Result<(), Error> {
        self.count(&SearchQuery::default()).await.map(|_| ())
    }

    /// An author's public addons, most recent first. Needs a key.
    pub async fn user_files(&self, steam_id: u64) -> Result<Vec<Addon>, Error> {
        let params = [
            ("key", self.key()?.to_string()),
            ("steamid", steam_id.to_string()),
            ("appid", GMOD_APPID.to_string()),
            ("numperpage", "50".to_string()),
            ("return_tags", "true".to_string()),
            ("return_children", "true".to_string()),
            ("return_vote_data", "true".to_string()),
            ("return_short_description", "true".to_string()),
            ("strip_description_bbcode", "true".to_string()),
        ];
        let url = format!("{API}/IPublishedFileService/GetUserFiles/v1/");
        let body = read(self.http.get(url).query(&params).send().await?).await?;
        Ok(parse_file_list(&body)?.addons)
    }

    /// Finds what people building this kind of collection put in it: samples
    /// collections matching `theme` (and containing `anchor`, if given),
    /// counts their members, and ranks them by how much more common they are
    /// there than across all collections. Needs a key; costs about 85 requests.
    pub async fn discover(
        &self,
        theme: &str,
        anchor: Option<u64>,
        pages: usize,
    ) -> Result<Discovery, Error> {
        let mut sample = SearchQuery {
            text: theme.to_string(),
            sort: Sort::TopRated,
            collections: true,
            containing: anchor,
            per_page: 100,
            ..Default::default()
        };
        let mut found = Vec::new();
        for _ in 0..pages.max(1) {
            let page = self.search(&sample).await?;
            found.extend(page.addons);
            match page.next_cursor {
                Some(cursor) => sample.cursor = Some(cursor),
                None => break,
            }
        }
        // Steam's text search drifts: for some phrases most of the top
        // results match one word only. Prefer collections that say what was
        // asked for, when there are enough of them to count.
        let on_theme: Vec<bool> =
            found.iter().map(|c| discover::on_theme(theme, &c.title, &c.description)).collect();
        let focused = on_theme.iter().filter(|&&hit| hit).count() >= discover::MIN_FOCUSED;
        let collections: Vec<Vec<u64>> = found
            .into_iter()
            .zip(on_theme)
            .filter(|(_, hit)| *hit || !focused)
            .filter_map(|(collection, _)| collection.required)
            .collect();
        let (sample_size, counts) = discover::count_members(&collections);
        let candidates: Vec<(u64, u32)> = counts
            .into_iter()
            .filter(|&(id, _)| Some(id) != anchor)
            .take(DISCOVER_CANDIDATES)
            .collect();

        let every_collection = SearchQuery { collections: true, ..Default::default() };
        let total_collections = self.count(&every_collection).await?;
        let baselines: Vec<Result<(u64, u32, u64), Error>> = stream::iter(candidates)
            .map(|(id, in_sample)| {
                let query = SearchQuery { containing: Some(id), ..every_collection.clone() };
                async move { Ok((id, in_sample, self.count(&query).await?)) }
            })
            .buffer_unordered(PARALLEL_REQUESTS)
            .collect()
            .await;
        let baselines = baselines.into_iter().collect::<Result<Vec<_>, _>>()?;
        Ok(Discovery {
            sample_size,
            suggestions: discover::rank(sample_size, total_collections, &baselines),
        })
    }

    /// Details for each id, in the order asked. Ids Steam knows nothing about
    /// come back as [`AddonStatus::Removed`] placeholders. Without a key the
    /// result has no required-item information (`Addon::required` is `None`).
    pub async fn details(&self, ids: &[u64]) -> Result<Vec<Addon>, Error> {
        let mut found: HashMap<u64, Addon> = HashMap::with_capacity(ids.len());
        for chunk in ids.chunks(DETAILS_CHUNK) {
            let body = match &self.key {
                Some(key) => self.details_keyed(key, chunk).await?,
                None => self.details_keyless(chunk).await?,
            };
            for addon in parse_file_list(&body)?.addons {
                found.insert(addon.id, addon);
            }
        }
        Ok(ids
            .iter()
            .map(|id| {
                found
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| Addon::unavailable(*id, AddonStatus::Removed))
            })
            .collect())
    }

    async fn details_keyed(&self, key: &str, ids: &[u64]) -> Result<String, Error> {
        let mut params = vec![
            ("key".to_string(), key.to_string()),
            ("appid".to_string(), GMOD_APPID.to_string()),
            ("includechildren".to_string(), "true".to_string()),
            ("includetags".to_string(), "true".to_string()),
            ("includevotes".to_string(), "true".to_string()),
            ("short_description".to_string(), "true".to_string()),
            ("strip_description_bbcode".to_string(), "true".to_string()),
        ];
        for (i, id) in ids.iter().enumerate() {
            params.push((format!("publishedfileids[{i}]"), id.to_string()));
        }
        let url = format!("{API}/IPublishedFileService/GetDetails/v1/");
        read(self.http.get(url).query(&params).send().await?).await
    }

    async fn details_keyless(&self, ids: &[u64]) -> Result<String, Error> {
        let mut params = vec![("itemcount".to_string(), ids.len().to_string())];
        for (i, id) in ids.iter().enumerate() {
            params.push((format!("publishedfileids[{i}]"), id.to_string()));
        }
        let url = format!("{API}/ISteamRemoteStorage/GetPublishedFileDetails/v1/");
        read(self.http.post(url).form(&params).send().await?).await
    }

    /// Direct members of a collection. Works without a key.
    pub async fn collection_children(&self, id: u64) -> Result<Vec<CollectionChild>, Error> {
        let params = [
            ("collectioncount".to_string(), "1".to_string()),
            ("publishedfileids[0]".to_string(), id.to_string()),
        ];
        let url = format!("{API}/ISteamRemoteStorage/GetCollectionDetails/v1/");
        let body = read(self.http.post(url).form(&params).send().await?).await?;
        parse_collection(&body)?.ok_or(Error::NotFound(id))
    }

    /// Every addon in a collection, following linked collections.
    pub async fn collection_items(&self, id: u64) -> Result<Vec<u64>, Error> {
        let mut items = Vec::new();
        let mut seen_items = HashSet::new();
        let mut seen_collections = HashSet::from([id]);
        let mut queue = VecDeque::from([id]);
        while let Some(collection) = queue.pop_front() {
            for child in self.collection_children(collection).await? {
                if child.is_collection {
                    if seen_collections.len() < MAX_NESTED_COLLECTIONS
                        && seen_collections.insert(child.id)
                    {
                        queue.push_back(child.id);
                    }
                } else if seen_items.insert(child.id) {
                    items.push(child.id);
                }
            }
        }
        Ok(items)
    }
}

async fn read(resp: reqwest::Response) -> Result<String, Error> {
    let status = resp.status().as_u16();
    match status {
        200..=299 => Ok(resp.text().await?),
        401 | 403 => Err(Error::InvalidKey),
        429 => Err(Error::RateLimited),
        other => Err(Error::Status(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_params_follow_the_query() {
        let q = SearchQuery {
            text: " wire ".into(),
            tags: vec!["tool".into(), "Build".into()],
            ..Default::default()
        };
        let p: HashMap<_, _> = q.params("K").into_iter().collect();
        assert_eq!(p["query_type"], "12");
        assert_eq!(p["search_text"], "wire");
        assert_eq!(p["cursor"], "*");
        assert_eq!(p["numperpage"], "30");
        assert_eq!(p["requiredtags[1]"], "Build");
        assert!(!p.contains_key("days"));

        assert_eq!(p["filetype"], "0");
        assert!(!p.contains_key("child_publishedfileid"));

        let containing = SearchQuery {
            collections: true,
            containing: Some(7),
            excluded_tags: vec!["map".into()],
            ..Default::default()
        };
        let p: HashMap<_, _> = containing.params("K").into_iter().collect();
        assert_eq!(p["filetype"], "1");
        assert_eq!(p["child_publishedfileid"], "7");
        assert_eq!(p["excludedtags[0]"], "map");

        let trending = SearchQuery::default().params("K");
        let p: HashMap<_, _> = trending.into_iter().collect();
        assert_eq!(p["query_type"], "3");
        assert_eq!(p["days"], "7");
        assert!(!p.contains_key("search_text"));
    }

    #[test]
    fn blank_key_counts_as_no_key() {
        assert!(!SteamClient::new(Some("  ".into())).has_key());
        assert!(SteamClient::new(Some("abc".into())).has_key());
    }
}
