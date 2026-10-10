//! Search: the Workshop by name, tags and sort order, for addons or for
//! collections to pick addons out of.

use eframe::egui::{self, Align, Layout, RichText};
use gmcb_core::{Addon, AddonStatus, Sort};

use crate::app::{App, Tab};
use crate::format;
use super::RowAction;
use super::theme;
use super::widgets::{self, RowOptions};

/// GMod Workshop tags, as (tag, label).
const TYPE_TAGS: [(&str, &str); 10] = [
    ("gamemode", "Gamemode"),
    ("map", "Map"),
    ("weapon", "Weapon"),
    ("vehicle", "Vehicle"),
    ("npc", "NPC"),
    ("entity", "Entity"),
    ("tool", "Tool"),
    ("effects", "Effects"),
    ("model", "Model"),
    ("servercontent", "Server content"),
];
const CATEGORY_TAGS: [(&str, &str); 9] = [
    ("fun", "Fun"),
    ("roleplay", "Roleplay"),
    ("scenic", "Scenic"),
    ("movie", "Movie"),
    ("realism", "Realism"),
    ("cartoon", "Cartoon"),
    ("water", "Water"),
    ("comic", "Comic"),
    ("build", "Build"),
];
const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;

impl App {
    pub(crate) fn search_tab(&mut self, ui: &mut egui::Ui) {
        if !self.client.has_key() {
            let mut open_settings = false;
            theme::empty_state(
                ui,
                "Add your Steam key to search",
                "Searching the Workshop uses Steam's Web API, which needs a free key.",
                |ui| open_settings = ui.add(theme::primary_button("Open Settings")).clicked(),
            );
            if open_settings {
                self.tab = Tab::Settings;
            }
            return;
        }
        if self.search.browse.is_some() {
            return self.browse_view(ui);
        }

        let ctx = ui.ctx().clone();
        let mut submit = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for (collections, label) in [(false, "Addons"), (true, "Collections")] {
                if theme::toggle_chip(ui, self.search.collections == collections, label)
                    && self.search.collections != collections
                {
                    self.search.collections = collections;
                    submit = true;
                }
            }
            ui.add_space(2.0);
            let width = (ui.available_width() - 276.0).max(140.0);
            let hint = if self.search.collections {
                "Search collections, then open one to pick addons from it"
            } else {
                "Search the Workshop"
            };
            let text = ui.add(
                theme::text_field(&mut self.search.text).hint_text(hint).desired_width(width),
            );
            submit |= text.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            egui::ComboBox::from_id_salt("sort")
                .width(150.0)
                .selected_text(self.search.sort.label())
                .show_ui(ui, |ui| {
                    for sort in Sort::ALL {
                        submit |=
                            ui.selectable_value(&mut self.search.sort, sort, sort.label()).changed();
                    }
                });
            submit |= ui.add(theme::primary_button("Search")).clicked();
        });
        if !self.search.collections {
            submit |= self.search_filters(ui);
        }
        if submit {
            self.start_search(&ctx, false);
        }
        ui.add_space(4.0);

        if !self.search.searched {
            if self.search.loading {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.spinner();
                });
            } else {
                theme::empty_state(
                    ui,
                    "Find addons",
                    "Search by name, or pick tags and a sort order to browse.",
                    |_| {},
                );
            }
            return;
        }

        let now = format::now_unix();
        let max_size = u64::from(self.search.max_size_mb) * 1024 * 1024;
        let max_age = u64::from(self.search.max_age_years) * SECONDS_PER_YEAR;
        let collections = self.search.results_are_collections;
        let shown: Vec<&Addon> = self
            .search
            .results
            .iter()
            .filter(|a| a.status == AddonStatus::Ok)
            .filter(|a| collections || max_size == 0 || a.size_bytes <= max_size)
            .filter(|a| collections || max_age == 0 || now.saturating_sub(a.time_updated) <= max_age)
            .filter(|a| collections || !self.search.hide_in_draft || !self.draft.contains(a.id))
            .collect();
        if shown.is_empty() && !self.search.loading && self.search.next_cursor.is_none() {
            theme::empty_state(ui, "Nothing found", "Try fewer tags or a different search.", |_| {});
            return;
        }
        let hidden = self.search.results.len() - shown.len();
        let mut summary = format!("{} matches", format::count(self.search.total));
        if hidden > 0 {
            summary.push_str(&format!(" · {hidden} loaded results hidden by filters"));
        }
        ui.label(theme::small(summary));

        let can_load_more = self.search.next_cursor.is_some() && !self.search.loading;
        let loading = self.search.loading;
        let mut action = None;
        let mut open = None;
        let mut load_more = false;
        // One extra row at the end for the "Load more" button.
        let heights = vec![widgets::row_height(); shown.len() + 1];
        widgets::virtual_list(ui, "results", &heights, |ui, row| {
            let Some(addon) = shown.get(row) else {
                ui.vertical_centered(|ui| {
                    ui.add_space(20.0);
                    if loading {
                        ui.spinner();
                    } else if can_load_more {
                        load_more = ui.button("Load more").clicked();
                    }
                });
                return;
            };
            if collections {
                widgets::collection_row(ui, addon, |ui| {
                    if ui.add(theme::primary_button("Open")).clicked() {
                        open = Some(row);
                    }
                });
            } else {
                let options =
                    RowOptions { now, selected: self.selected == Some(addon.id), note: None, vouch: self.vouch_score(addon.id) };
                if widgets::addon_row(ui, addon, options, |ui| {
                    self.offer_buttons(ui, addon.id, &mut action);
                }) {
                    action = Some(RowAction::Select(addon.id));
                }
            }
        });

        if let Some(collection) = open.map(|row| shown[row].clone()) {
            self.browse_collection(&ctx, &collection);
        }
        self.apply(&ctx, action);
        if load_more {
            self.start_search(&ctx, true);
        }
    }

    /// Tag chips and the client-side filters. Returns true if the search
    /// should run again.
    fn search_filters(&mut self, ui: &mut egui::Ui) -> bool {
        let mut submit = false;
        for (heading, tags) in [("Type", &TYPE_TAGS[..]), ("Category", &CATEGORY_TAGS[..])] {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                widgets::row_label(ui, heading);
                for &(tag, label) in tags {
                    let wanted = self.search.tags.iter().any(|t| t == tag);
                    let excluded = self.search.excluded.iter().any(|t| t == tag);
                    let state = if wanted { Some(true) } else { excluded.then_some(false) };
                    let chip = theme::filter_chip(ui, state, label)
                        .on_hover_text("Click to require this tag, right-click to exclude it");
                    if chip.clicked() || chip.secondary_clicked() {
                        self.search.tags.retain(|t| t != tag);
                        self.search.excluded.retain(|t| t != tag);
                        if chip.clicked() && state.is_none() {
                            self.search.tags.push(tag.to_string());
                        } else if chip.secondary_clicked() && state != Some(false) {
                            self.search.excluded.push(tag.to_string());
                        }
                        submit = true;
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            widgets::row_label(ui, "Filters");
            ui.label(theme::muted_text("Max size"));
            ui.add(
                egui::DragValue::new(&mut self.search.max_size_mb)
                    .range(0..=100_000)
                    .speed(5)
                    .custom_formatter(|v, _| any_or(v, "MB")),
            );
            ui.add_space(8.0);
            ui.label(theme::muted_text("Updated within"));
            ui.add(
                egui::DragValue::new(&mut self.search.max_age_years)
                    .range(0..=20)
                    .speed(0.1)
                    .custom_formatter(|v, _| any_or(v, "yr")),
            );
            ui.add_space(8.0);
            ui.checkbox(&mut self.search.hide_in_draft, theme::muted_text("Hide what I already have"));
            ui.add_space(8.0);
            let label = theme::muted_text("Include dupes and saves");
            submit |= ui.checkbox(&mut self.search.include_non_addons, label).changed();
        });
        submit
    }

    /// The contents of a collection opened from the results.
    fn browse_view(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(browse) = &self.search.browse else { return };
        let addons: Vec<Addon> = browse.ids.iter().map(|&id| self.addon_or_stand_in(id)).collect();
        let missing = addons
            .iter()
            .filter(|a| a.status == AddonStatus::Ok && !self.draft.contains(a.id))
            .count();

        let mut back = false;
        let mut add_all = false;
        ui.horizontal(|ui| {
            back = ui.button("← Back to results").clicked();
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let label = format!("Add all {missing}");
                add_all = ui.add_enabled(missing > 0, theme::primary_button(&label)).clicked();
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let title = RichText::new(&browse.title).heading();
                    ui.add(egui::Label::new(title).truncate());
                });
            });
        });
        let collection = browse.id;
        let mut verdict = None;
        ui.horizontal(|ui| {
            ui.label(theme::small(format!("{} addons in this collection", addons.len())));
            verdict = self.vouch_row(ui, collection);
        });

        let now = format::now_unix();
        let mut action = None;
        let heights = vec![widgets::row_height(); addons.len()];
        widgets::virtual_list(ui, "browse", &heights, |ui, row| {
            let addon = &addons[row];
            let options = RowOptions { now, selected: self.selected == Some(addon.id), note: None, vouch: self.vouch_score(addon.id) };
            if widgets::addon_row(ui, addon, options, |ui| {
                self.offer_buttons(ui, addon.id, &mut action);
            }) {
                action = Some(RowAction::Select(addon.id));
            }
        });

        if add_all {
            let ids = addons.iter().filter(|a| a.status == AddonStatus::Ok).map(|a| a.id).collect();
            let added = self.add_many(&ctx, ids, true);
            self.info(format!("Added {added} addon(s)"));
        }
        if back {
            self.search.browse = None;
        }
        if let Some(verdict) = verdict {
            self.vote(collection, verdict);
        }
        self.apply(&ctx, action);
    }
}

/// DragValue text for limits where 0 means "no limit".
fn any_or(value: f64, unit: &str) -> String {
    if value == 0.0 {
        "Any".to_string()
    } else {
        format!("{value:.0} {unit}")
    }
}
