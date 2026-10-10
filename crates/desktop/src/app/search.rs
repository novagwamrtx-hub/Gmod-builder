//! Searching the Workshop for addons or collections.

use eframe::egui;
use gmcb_core::{Addon, SearchQuery, Sort};

use super::{App, Msg};

const SEARCH_PAGE_SIZE: u32 = 50;

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
    pub(super) generation: u64,
}

pub struct Browse {
    pub id: u64,
    pub title: String,
    pub ids: Vec<u64>,
}

impl App {
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
}
