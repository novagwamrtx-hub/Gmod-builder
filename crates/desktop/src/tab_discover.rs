//! Discover: say what you are building and see what people who built
//! similar collections put in theirs, grouped by type.

use eframe::egui;
use gmcb_core::{parse_workshop_id, AddonStatus, Kind};

use crate::app::{App, Tab};
use crate::format;
use crate::ideas;
use crate::tabs::RowAction;
use crate::theme;
use crate::widgets::{self, RowOptions, HEADER_HEIGHT};

/// Tallest the list of ideas under the search field gets.
const IDEAS_HEIGHT: f32 = 340.0;

enum Row {
    Header(Kind, usize),
    Suggestion(usize),
}

impl App {
    pub(crate) fn discover_tab(&mut self, ui: &mut egui::Ui) {
        if !self.client.has_key() {
            let mut open_settings = false;
            theme::empty_state(
                ui,
                "Add your Steam key to discover addons",
                "Discovering reads Workshop collections through Steam's Web API, which needs a free key.",
                |ui| open_settings = ui.add(theme::primary_button("Open Settings")).clicked(),
            );
            if open_settings {
                self.tab = Tab::Settings;
            }
            return;
        }

        let ctx = ui.ctx().clone();
        let mut submit = false;
        let mut focus_field = false;
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 130.0).max(160.0);
            let field = ui.add(
                theme::text_field(&mut self.discover.theme)
                    .hint_text("What are you building? Type it, or pick from the list")
                    .desired_width(width),
            );
            submit |= field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(idea) = self.ideas_list(&field) {
                self.discover.theme = idea.to_string();
                submit = !self.discover.loading;
            }
            let find = theme::primary_button("Find addons");
            submit |= ui.add_enabled(!self.discover.loading, find).clicked();
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            widgets::row_label(ui, "Ideas");
            for idea in ideas::POPULAR {
                let on = self.discover.theme.eq_ignore_ascii_case(idea);
                if theme::toggle_chip(ui, on, idea) && !self.discover.loading {
                    self.discover.theme = idea.to_string();
                    submit = true;
                }
            }
            if theme::toggle_chip(ui, false, &format!("All {} ideas", ideas::count())) {
                self.discover.theme.clear();
                focus_field = true;
            }
        });
        if focus_field {
            self.discover.ideas_open = true;
        }
        ui.horizontal(|ui| {
            widgets::row_label(ui, "Around");
            let input = ui.add(
                theme::text_field(&mut self.discover.anchor_input)
                    .hint_text("Optional: paste the link of an addon you want to build around")
                    .desired_width(400.0),
            );
            if input.changed() {
                self.discover.anchor = parse_workshop_id(&self.discover.anchor_input);
            }
            submit |= input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if let Some(id) = self.discover.anchor {
                ui.colored_label(theme::good(), "✔");
                ui.add(egui::Label::new(theme::muted_text(self.title_of(id))).truncate());
            } else if !self.discover.anchor_input.trim().is_empty() {
                ui.colored_label(theme::bad(), "Not an addon link");
            }
        });
        if let Some(id) = self.discover.anchor.filter(|&id| self.needs_details(id)) {
            self.fetch_details(&ctx, vec![id]);
        }
        if submit {
            self.start_discover(&ctx, false);
        }
        ui.add_space(4.0);

        if self.discover.loading {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.spinner();
                ui.label(theme::muted_text("Reading a few hundred collections…"));
                ui.label(theme::small("This takes up to a minute the first time for each search."));
            });
            return;
        }
        let Some(discovery) = &self.discover.result else {
            theme::empty_state(
                ui,
                "Find addons for what you are building",
                "Looks at collections like yours and shows what they have in common, by type.",
                |_| {},
            );
            return;
        };

        // (suggestion index, kind) for every suggestion that can be offered.
        let usable: Vec<(usize, Kind)> = discovery
            .suggestions
            .iter()
            .enumerate()
            .filter_map(|(i, suggestion)| {
                let addon = self.addons.get(&suggestion.id)?;
                (addon.status == AddonStatus::Ok).then(|| (i, addon.kind()))
            })
            .collect();
        if usable.is_empty() {
            theme::empty_state(
                ui,
                "Not enough to go on",
                "Too few collections matched. Try a broader description, or drop the addon link.",
                |_| {},
            );
            return;
        }
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.label(theme::small(format!(
                "From {} {}. Click a type to hide it.",
                discovery.sample_size, self.discover.result_label
            )));
            if let Some(at) = self.discover.result_from {
                ui.label(theme::small(format!("Saved {}.", format::ago(format::now_unix(), at))));
                refresh = ui.link(egui::RichText::new("Ask again").small()).clicked();
            }
        });
        let per_kind = self.settings.discover_per_type.clamp(3, 40);
        let kinds: Vec<Kind> =
            Kind::ALL.into_iter().filter(|kind| usable.iter().any(|(_, k)| k == kind)).collect();
        let mut toggled = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            for &kind in &kinds {
                if theme::toggle_chip(ui, !self.discover.hidden.contains(&kind), kind.label()) {
                    toggled = Some(kind);
                }
            }
        });

        let mut rows = Vec::new();
        for &kind in kinds.iter().filter(|kind| !self.discover.hidden.contains(kind)) {
            let of_kind: Vec<usize> =
                usable.iter().filter(|(_, k)| *k == kind).map(|(i, _)| *i).collect();
            rows.push(Row::Header(kind, of_kind.len().min(per_kind)));
            rows.extend(of_kind.into_iter().take(per_kind).map(Row::Suggestion));
        }
        let heights: Vec<f32> = rows
            .iter()
            .map(|row| if matches!(row, Row::Header(..)) { HEADER_HEIGHT } else { widgets::row_height() })
            .collect();
        let now = format::now_unix();
        let mut action = None;
        widgets::virtual_list(ui, "discover", &heights, |ui, i| match rows[i] {
            Row::Header(kind, count) => {
                widgets::list_header(ui, kind.label(), &format!("top {count}"));
            }
            Row::Suggestion(index) => {
                let suggestion = &discovery.suggestions[index];
                let addon = &self.addons[&suggestion.id];
                let note = format!(
                    "In {} of the {} collections",
                    suggestion.in_sample, discovery.sample_size
                );
                let options = RowOptions {
                    now,
                    selected: self.selected == Some(addon.id),
                    note: Some(&note),
                    vouch: self.vouch_score(addon.id),
                };
                if widgets::addon_row(ui, addon, options, |ui| {
                    self.offer_buttons(ui, addon.id, &mut action);
                }) {
                    action = Some(RowAction::Select(addon.id));
                }
            }
        });

        if let Some(kind) = toggled {
            if !self.discover.hidden.remove(&kind) {
                self.discover.hidden.insert(kind);
            }
        }
        self.apply(&ctx, action);
        if refresh {
            self.start_discover(&ctx, true);
        }
    }

    /// The list of ideas that drops down from the search field, narrowed by
    /// what has been typed. Returns the idea picked from it, if any.
    fn ideas_list(&mut self, field: &egui::Response) -> Option<&'static str> {
        let typed = self.discover.theme.trim().to_string();
        // With an idea already chosen, show the others rather than only it.
        let filter = if ideas::named(&typed).is_some() { "" } else { typed.as_str() };
        let groups = ideas::matching(filter);
        let mut open = self.discover.ideas_open && !groups.is_empty();
        let mut picked = None;
        egui::Popup::from_response(field)
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(field.rect.width())
            .gap(4.0)
            .show(|ui| {
                ui.set_min_width(field.rect.width() - 16.0);
                let shown: usize = groups.iter().map(|(_, ideas)| ideas.len()).sum();
                let hint = if filter.is_empty() {
                    format!("{shown} ideas. Type to narrow them down, or search for anything else.")
                } else {
                    format!("{shown} ideas match. Press Enter to search for \"{typed}\" itself.")
                };
                ui.label(theme::small(hint));
                egui::ScrollArea::vertical().max_height(IDEAS_HEIGHT).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (category, ideas) in &groups {
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(*category).small().strong().color(theme::accent()));
                        for idea in ideas {
                            let row = egui::Button::new(idea.name)
                                .frame_when_inactive(false)
                                .min_size(egui::vec2(ui.available_width(), 24.0));
                            if ui.add(row).clicked() {
                                picked = Some(idea.name);
                            }
                        }
                    }
                });
            });
        // Clicking the field counts as clicking outside the list, so it is
        // reopened here rather than left to the popup.
        if field.gained_focus() || field.clicked() || field.changed() {
            open = true;
        }
        if picked.is_some() || field.ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
        self.discover.ideas_open = open;
        picked
    }
}
