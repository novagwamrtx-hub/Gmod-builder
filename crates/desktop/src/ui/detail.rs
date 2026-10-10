//! The panel beside every tab that shows one addon: what it is, what it
//! requires, what is built for it, and the vouch buttons.

use eframe::egui::{self, Align, Layout, RichText};
use gmcb_core::community::MIN_VOTES;
use gmcb_core::AddonStatus;

use super::{theme, widgets, RowAction};
use crate::app::{App, RelatedKind, Tab};
use crate::format;

impl App {
    /// What a row says about vouches: the score, once enough builders have
    /// voted, and the user's own verdict.
    pub(crate) fn vouch_note(&self, id: u64) -> Option<String> {
        if !self.settings.community {
            return None;
        }
        let score = self.tally_of(id).score().map(|score| format!("vouch score {score}"));
        let mine = self.votes.get(&id).map(|&vouch| if vouch { "you vouched" } else { "you warned" });
        match (score, mine) {
            (Some(score), Some(mine)) => Some(format!("{score} · {mine}")),
            (Some(score), None) => Some(score),
            (None, mine) => mine.map(str::to_string),
        }
    }

    /// Vouch and Warn buttons with what builders have said so far. Returns
    /// the user's new verdict if they changed it.
    pub(crate) fn vouch_row(&self, ui: &mut egui::Ui, id: u64) -> Option<Option<bool>> {
        if !self.settings.community {
            return None;
        }
        let mine = self.votes.get(&id).copied();
        let mut verdict = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let vouch = if mine == Some(true) { "✔ Vouched" } else { "Vouch" };
            let hint = "It worked well for you and you would put it in a collection again";
            if theme::filter_chip(ui, (mine == Some(true)).then_some(true), vouch)
                .on_hover_text(hint)
                .clicked()
            {
                verdict = Some((mine != Some(true)).then_some(true));
            }
            let warn = if mine == Some(false) { "Warned" } else { "Warn" };
            let hint = "It was broken, clashed with other addons, or was not what it says";
            if theme::filter_chip(ui, (mine == Some(false)).then_some(false), warn)
                .on_hover_text(hint)
                .clicked()
            {
                verdict = Some((mine != Some(false)).then_some(false));
            }
            let tally = self.tally_of(id);
            let said = match (tally.score(), tally.votes()) {
                (Some(score), votes) => format!("Vouch score {score} · {votes} builders"),
                (None, 0) => "No vouches yet".to_string(),
                (None, _) => {
                    format!("{} vouched, {} warned · needs {MIN_VOTES} for a score", tally.up, tally.down)
                }
            };
            // Kept whole: in a narrow panel it goes under the buttons.
            ui.add(egui::Label::new(theme::small(said)).wrap_mode(egui::TextWrapMode::Extend));
        });
        verdict
    }

    /// Everything about one addon, beside whichever tab is open.
    pub(super) fn detail_panel(&mut self, ui: &mut egui::Ui, id: u64) {
        let addon = self.addon_or_stand_in(id);
        let now = format::now_unix();
        let mut action = None;
        let mut close = false;
        let mut load = None;
        let mut discover = false;
        let mut verdict = None;

        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                close = ui.button("Close").on_hover_text("Esc").clicked();
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.add(egui::Label::new(RichText::new(addon.display_title()).heading()).wrap());
                });
            });
        });

        egui::ScrollArea::vertical().id_salt("detail").auto_shrink(false).show(ui, |ui| {
            ui.vertical_centered(|ui| widgets::thumbnail(ui, &addon, 200.0));
            match addon.status {
                AddonStatus::Ok if addon.time_updated == 0 => {
                    ui.label(theme::muted_text("Loading details…"));
                }
                AddonStatus::Ok => {
                    ui.label(theme::small(format!(
                        "{} · {} subscribers",
                        format::size(addon.size_bytes),
                        format::count(addon.subscriptions)
                    )));
                    if let Some(rating) = addon.rating() {
                        ui.label(theme::small(format!(
                            "{rating}% liked · {} votes",
                            format::count(addon.votes_up + addon.votes_down)
                        )));
                    }
                    ui.label(theme::small(format!(
                        "Updated {} · published {}",
                        format::ago(now, addon.time_updated),
                        format::ago(now, addon.time_created)
                    )));
                    ui.horizontal_wrapped(|ui| {
                        for tag in addon.tags.iter().filter(|t| *t != "Addon") {
                            theme::chip(ui, tag);
                        }
                    });
                }
                _ => {
                    ui.colored_label(theme::bad(), "No longer available on the Workshop");
                }
            }
            ui.horizontal(|ui| {
                // `offer_buttons` is written for right-to-left rows.
                let in_draft = self.draft.contains(id);
                if let Some(add) = widgets::add_button(ui, in_draft) {
                    action = Some(RowAction::SetInDraft(id, add));
                }
                if !in_draft && widgets::maybe_button(ui, self.draft.maybe.contains(&id)) {
                    action = Some(RowAction::Maybe(id));
                }
            });
            if addon.status == AddonStatus::Ok {
                verdict = self.vouch_row(ui, id);
                if self.settings.community && self.votes.is_empty() {
                    ui.label(theme::small(
                        "Vouch if it worked well in a collection of yours; warn if it was broken or \
                         clashed with other addons. Settings has more about vouches.",
                    ));
                }
            }
            ui.hyperlink_to("Open on the Workshop", addon.url());
            if !addon.description.trim().is_empty() {
                ui.add_space(2.0);
                ui.add(egui::Label::new(theme::muted_text(addon.description.trim())).wrap());
            }

            if let Some(required) = addon.required.as_ref().filter(|r| !r.is_empty()) {
                section(ui, "Requires");
                for &other in required {
                    self.related_row(ui, other, &mut action);
                }
            }
            let dependents: Vec<u64> = self
                .draft
                .ids
                .iter()
                .copied()
                .filter(|other| {
                    let required = self.addons.get(other).and_then(|a| a.required.as_ref());
                    required.is_some_and(|r| r.contains(&id))
                })
                .collect();
            if !dependents.is_empty() {
                section(ui, "Required by, in your draft");
                for other in dependents {
                    self.related_row(ui, other, &mut action);
                }
            }

            if self.client.has_key() && addon.status == AddonStatus::Ok {
                let related = self.related.get(&id);
                let loading = related.is_some_and(|r| r.loading);
                for (kind, title, button, found) in [
                    (
                        RelatedKind::BuiltFor,
                        "Built for this",
                        "Find addons built for this",
                        related.and_then(|r| r.built_for.as_ref()),
                    ),
                    (
                        RelatedKind::Author,
                        "More from this author",
                        "More from this author",
                        related.and_then(|r| r.author.as_ref()),
                    ),
                ] {
                    match found {
                        Some(found) => {
                            section(ui, title);
                            if found.is_empty() {
                                ui.label(theme::small("Nothing found."));
                            }
                            for &other in found {
                                self.related_row(ui, other, &mut action);
                            }
                        }
                        None => {
                            ui.add_space(4.0);
                            if ui.add_enabled(!loading, egui::Button::new(button)).clicked() {
                                load = Some(kind);
                            }
                        }
                    }
                }
                ui.add_space(4.0);
                discover = ui
                    .button("Discover around this")
                    .on_hover_text("See what collections containing this addon also include")
                    .clicked();
            }
        });

        let ctx = ui.ctx().clone();
        if let Some(kind) = load {
            self.load_related(&ctx, id, kind);
        }
        if discover {
            self.discover.anchor = Some(id);
            self.discover.anchor_input = addon.url();
            self.tab = Tab::Discover;
            self.start_discover(&ctx, false);
        }
        if close {
            self.selected = None;
        }
        if let Some(verdict) = verdict {
            self.vote(id, verdict);
        }
        self.apply(&ctx, action);
    }

    /// A one-line addon in the detail panel: click the name to look at it.
    fn related_row(&self, ui: &mut egui::Ui, id: u64, action: &mut Option<RowAction>) {
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().button_padding = egui::vec2(6.0, 2.0);
                if self.draft.contains(id) {
                    ui.colored_label(theme::good(), "✔").on_hover_text("In your draft");
                } else if ui.button("+").on_hover_text("Add to the draft").clicked() {
                    *action = Some(RowAction::SetInDraft(id, true));
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let title = egui::Label::new(self.title_of(id)).truncate().sense(egui::Sense::click());
                    if ui.add(title).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        *action = Some(RowAction::Select(id));
                    }
                });
            });
        });
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong().color(theme::text()));
}
