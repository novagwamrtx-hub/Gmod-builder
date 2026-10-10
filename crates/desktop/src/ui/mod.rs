//! Everything drawn on screen. This file is the window's frame: the tab bar,
//! the status bar, the page transitions and the keyboard shortcuts. Each tab
//! has its own file, and `detail` is the addon panel beside them.

mod check;
mod detail;
mod discover;
mod draft;
mod export;
mod search;
mod settings;
pub mod theme;
pub mod widgets;

use std::time::Duration;

use eframe::egui::{self, Align, Key, Layout, Margin, Modifiers, RichText};
use gmcb_core::updates;

use crate::app::{App, Tab, PROJECT_REPO};

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
                    if let Some(tag) = &self.update {
                        let label = format!("Version {} is out", tag.trim_start_matches(['v', 'V']));
                        ui.hyperlink_to(label, updates::release_page(PROJECT_REPO))
                            .on_hover_text("Opens the page to download it from");
                    }
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
        let progress =
            if time > 0.0 { ((now - self.tab_changed) / time).clamp(0.0, 1.0) as f32 } else { 1.0 };
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
}
