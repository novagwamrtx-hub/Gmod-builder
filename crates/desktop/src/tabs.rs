//! The window's frame: tab bar, status bar, the addon detail panel and the
//! keyboard shortcuts. Each tab's contents are in a `tab_*.rs` file.

use std::time::Duration;

use eframe::egui::{self, Align, Key, Layout, Margin, Modifiers, RichText};
use gmcb_core::AddonStatus;

use crate::app::{App, RelatedKind, Tab};
use crate::format;
use crate::theme;
use crate::widgets;

const STATUS_SECONDS: u64 = 6;

/// Something the user did to an addon in a list.
pub(crate) enum RowAction {
    Select(u64),
    SetInDraft(u64, bool),
    Maybe(u64),
}

impl App {
    pub(crate) fn show(&mut self, ui: &mut egui::Ui) {
        self.shortcuts(ui.ctx());
        let bar = egui::Frame::new().fill(theme::bar()).inner_margin(Margin::symmetric(14, 8));

        egui::Panel::top("tabs").frame(bar).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("GMod Collection Builder").strong().color(theme::text()));
                ui.add_space(10.0);
                let draft = match self.draft.ids.len() {
                    0 => "Draft".to_string(),
                    n => format!("Draft  {n}"),
                };
                let mut current = None;
                let mut tab_button = |app: &mut Self, ui: &mut egui::Ui, tab, label: &str| {
                    let response = theme::tab(ui, app.tab == tab, label);
                    if response.clicked() {
                        app.tab = tab;
                    }
                    if app.tab == tab {
                        current = Some(response.rect);
                    }
                };
                for (tab, label) in [
                    (Tab::Discover, "Discover"),
                    (Tab::Search, "Search"),
                    (Tab::Draft, draft.as_str()),
                    (Tab::Check, "Check"),
                    (Tab::Export, "Export"),
                ] {
                    tab_button(self, ui, tab, label);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    tab_button(self, ui, Tab::Settings, "Settings");
                });
                // A line under the open tab, which slides when the tab changes.
                if let Some(rect) = current {
                    let time = theme::animation_time() * 1.5;
                    let id = egui::Id::new("tab-underline");
                    let left = ui.ctx().animate_value_with_time(id.with("left"), rect.left(), time);
                    let width = ui.ctx().animate_value_with_time(id.with("width"), rect.width(), time);
                    let line = egui::Rect::from_min_size(
                        egui::pos2(left + 8.0, rect.bottom() + 4.0),
                        egui::vec2((width - 16.0).max(0.0), 2.0),
                    );
                    ui.painter().rect_filled(line, 1.0, theme::accent());
                }
            });
        });

        self.expire_status(ui.ctx());
        let status_bar = bar.inner_margin(Margin::symmetric(14, 4));
        egui::Panel::bottom("status").frame(status_bar).exact_size(28.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().interact_size.y = 18.0;
                ui.set_min_height(18.0);
                if self.busy > 0 {
                    ui.spinner();
                }
                match &self.status {
                    Some(status) if status.is_error => ui.colored_label(theme::bad(), &status.text),
                    Some(status) => ui.label(theme::muted_text(&status.text)),
                    None if self.busy > 0 => ui.label(theme::muted_text("Talking to Steam…")),
                    None => ui.label(""),
                };
            });
        });

        // The panel keeps showing the last addon while it slides shut.
        self.last_selected = self.selected.or(self.last_selected);
        if let Some(id) = self.last_selected {
            let mut open = self.selected.is_some();
            let side = egui::Frame::new().fill(theme::bar()).inner_margin(14);
            egui::Panel::right("detail")
                .frame(side)
                .exact_size(theme::detail_width())
                .resizable(false)
                .show_collapsible(ui, &mut open, |ui| self.detail_panel(ui, id));
        }

        // Each page fades in and settles upwards when its tab is opened.
        let now = ui.input(|input| input.time);
        if self.shown_tab != Some(self.tab) {
            self.shown_tab = Some(self.tab);
            self.tab_changed = now;
        }
        let time = f64::from(theme::animation_time()) * 1.6;
        let progress = if time > 0.0 { ((now - self.tab_changed) / time).clamp(0.0, 1.0) as f32 } else { 1.0 };
        let arrived = 1.0 - (1.0 - progress).powi(3);
        if progress < 1.0 {
            ui.ctx().request_repaint();
        }

        let page = egui::Frame::new().fill(theme::bg()).inner_margin(16);
        egui::CentralPanel::default().frame(page).show(ui, |ui| {
            ui.set_opacity(arrived);
            ui.add_space((1.0 - arrived) * 12.0);
            match self.tab {
                Tab::Discover => self.discover_tab(ui),
                Tab::Search => self.search_tab(ui),
                Tab::Draft => self.draft_tab(ui),
                Tab::Check => self.check_tab(ui),
                Tab::Export => self.export_tab(ui),
                Tab::Settings => self.settings_tab(ui),
            }
        });
    }

    /// The vouch score of an addon or collection, once enough builders have
    /// voted on it.
    pub(crate) fn vouch_score(&self, id: u64) -> Option<u32> {
        if self.settings.community {
            self.tally_of(id).score()
        } else {
            None
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
            if theme::filter_chip(ui, (mine == Some(true)).then_some(true), vouch).on_hover_text(hint).clicked() {
                verdict = Some((mine != Some(true)).then_some(true));
            }
            let warn = if mine == Some(false) { "Warned" } else { "Warn" };
            let hint = "It was broken, clashed with other addons, or was not what it says";
            if theme::filter_chip(ui, (mine == Some(false)).then_some(false), warn).on_hover_text(hint).clicked() {
                verdict = Some((mine != Some(false)).then_some(false));
            }
            let tally = self.tally_of(id);
            let said = match (tally.score(), tally.votes()) {
                (Some(score), votes) => format!("Vouch score {score} · {votes} builders"),
                (None, 0) => "No vouches yet".to_string(),
                (None, _) => format!("{} vouched, {} warned", tally.up, tally.down),
            };
            ui.label(theme::small(said));
        });
        verdict
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        // While typing, these keys belong to the text field.
        if ctx.memory(|memory| memory.focused().is_some()) {
            return;
        }
        let pressed = |modifiers, key| ctx.input_mut(|input| input.consume_key(modifiers, key));
        if pressed(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || pressed(Modifiers::COMMAND, Key::Y) {
            self.redo(ctx);
        } else if pressed(Modifiers::COMMAND, Key::Z) {
            self.undo(ctx);
        }
        if pressed(Modifiers::NONE, Key::Escape) {
            self.selected = None;
        }
    }

    /// Confirmations fade after a few seconds; errors stay until replaced.
    fn expire_status(&mut self, ctx: &egui::Context) {
        let Some(status) = &self.status else { return };
        if status.is_error {
            return;
        }
        let lifetime = Duration::from_secs(STATUS_SECONDS);
        match lifetime.checked_sub(status.since.elapsed()) {
            Some(left) => ctx.request_repaint_after(left),
            None => self.status = None,
        }
    }

    pub(crate) fn apply(&mut self, ctx: &egui::Context, action: Option<RowAction>) {
        match action {
            Some(RowAction::Select(id)) => self.select(ctx, id),
            Some(RowAction::SetInDraft(id, true)) => self.add_to_draft(ctx, id),
            Some(RowAction::SetInDraft(id, false)) => self.remove_from_draft(ctx, id),
            Some(RowAction::Maybe(id)) => self.toggle_maybe(ctx, id),
            None => {}
        }
    }

    /// Add and Maybe buttons for an addon being offered (laid out right to left).
    pub(crate) fn offer_buttons(&self, ui: &mut egui::Ui, id: u64, action: &mut Option<RowAction>) {
        let in_draft = self.draft.contains(id);
        if let Some(add) = widgets::add_button(ui, in_draft) {
            *action = Some(RowAction::SetInDraft(id, add));
        }
        if !in_draft && widgets::maybe_button(ui, self.draft.maybe.contains(&id)) {
            *action = Some(RowAction::Maybe(id));
        }
    }

    /// Everything about one addon, beside whichever tab is open.
    fn detail_panel(&mut self, ui: &mut egui::Ui, id: u64) {
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
