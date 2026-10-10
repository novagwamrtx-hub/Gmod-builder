//! Draft: the collection being built, the maybe pile, and the library of
//! saved drafts.

use eframe::egui::{self, Align, Layout};
use gmcb_core::{Addon, AddonStatus, Kind};

use crate::app::{App, DraftView, Tab};
use crate::format;
use super::RowAction;
use super::theme;
use super::widgets::{self, RowOptions, HEADER_HEIGHT};

enum Row {
    Header(String, String),
    /// Index into the draft's addons.
    Item(usize),
    Maybe(u64),
}

/// What to do with the ticked addons.
enum Bulk {
    Clear,
    All,
    Maybe,
    Remove,
}

enum Menu {
    New,
    Open(std::path::PathBuf),
    OpenFile,
    SaveCopy,
    Delete,
}

impl App {
    pub(crate) fn draft_tab(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.draft_header(ui);
        if self.confirm_delete {
            let mut delete = false;
            widgets::finding_row(ui, theme::bad(), "Delete this draft? This cannot be undone.", |ui| {
                if ui.button("Cancel").clicked() {
                    self.confirm_delete = false;
                }
                delete = ui.button("Delete").clicked();
            });
            if delete {
                self.delete_draft();
            }
        }

        ui.horizontal(|ui| {
            let installed = self.installed.len();
            let reserved = if installed > 0 { 330.0 } else { 100.0 };
            let width = (ui.available_width() - reserved).clamp(160.0, 460.0);
            let input = ui.add(
                theme::text_field(&mut self.import_input)
                    .hint_text("Paste a collection link, or addon links, to import")
                    .desired_width(width),
            );
            let entered = input.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Import").clicked() || entered {
                let text = self.import_input.clone();
                if self.import(&ctx, &text) {
                    self.import_input.clear();
                }
            }
            if installed > 0 {
                let label = format!("Import my {installed} installed addons");
                let hint = "Adds every Workshop addon Garry's Mod has downloaded on this computer";
                if ui.button(label).on_hover_text(hint).clicked() {
                    self.import_installed(&ctx);
                }
            }
        });
        ui.add_space(4.0);

        if self.draft.ids.is_empty() && self.draft.maybe.is_empty() {
            let mut go = None;
            theme::empty_state(
                ui,
                "Your draft is empty",
                "Find addons in Discover or Search, or import an existing collection above.",
                |ui| {
                    ui.horizontal(|ui| {
                        // Centre the pair of buttons by hand.
                        ui.add_space((ui.available_width() - 250.0).max(0.0) / 2.0);
                        if ui.add(theme::primary_button("Go to Discover")).clicked() {
                            go = Some(Tab::Discover);
                        }
                        if ui.button("Go to Search").clicked() {
                            go = Some(Tab::Search);
                        }
                    });
                },
            );
            if let Some(tab) = go {
                self.tab = tab;
            }
            return;
        }

        let addons = self.draft_addons();
        let total: u64 = addons.iter().map(|a| a.size_bytes).sum();
        ui.horizontal(|ui| {
            ui.label(theme::small(format!("{} addons · {}", addons.len(), format::size(total))));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                egui::ComboBox::from_id_salt("draft-view")
                    .width(140.0)
                    .selected_text(self.draft_view.label())
                    .show_ui(ui, |ui| {
                        for view in DraftView::ALL {
                            ui.selectable_value(&mut self.draft_view, view, view.label());
                        }
                    });
                ui.label(theme::small("Show"));
            });
        });

        self.picked.retain(|id| self.draft.contains(*id));
        let mut bulk = None;
        if !self.picked.is_empty() {
            let message = format!("{} selected", self.picked.len());
            widgets::finding_row(ui, theme::accent(), &message, |ui| {
                if ui.button("Clear selection").clicked() {
                    bulk = Some(Bulk::Clear);
                }
                if ui.button("Select all").clicked() {
                    bulk = Some(Bulk::All);
                }
                if ui.button("Move to maybe").clicked() {
                    bulk = Some(Bulk::Maybe);
                }
                if ui.button(egui::RichText::new("Remove").color(theme::bad())).clicked() {
                    bulk = Some(Bulk::Remove);
                }
            });
        }

        let rows = self.draft_rows(&addons);
        let heights: Vec<f32> = rows
            .iter()
            .map(|row| if matches!(row, Row::Header(..)) { HEADER_HEIGHT } else { widgets::row_height() })
            .collect();
        let reorder = self.draft_view == DraftView::Order;
        let last = addons.len().saturating_sub(1);
        let now = format::now_unix();
        let mut action = None;
        let mut moved = None;
        let mut ticked = None;
        widgets::virtual_list(ui, "draft", &heights, |ui, i| match &rows[i] {
            Row::Header(title, detail) => widgets::list_header(ui, title, detail),
            Row::Item(index) => {
                let (index, addon) = (*index, &addons[*index]);
                let options = RowOptions { now, selected: self.selected == Some(addon.id), note: None, vouch: self.vouch_score(addon.id) };
                let clicked = widgets::addon_row(ui, addon, options, |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let mut picked = self.picked.contains(&addon.id);
                    let tick = ui.checkbox(&mut picked, "");
                    if tick.on_hover_text("Select, to remove or move several at once").changed() {
                        ticked = Some(addon.id);
                    }
                    if ui.button("Remove").clicked() {
                        action = Some(RowAction::SetInDraft(addon.id, false));
                    }
                    if reorder {
                        let down = ui.add_enabled(index < last, egui::Button::new("⬇"));
                        if down.on_hover_text("Move down").clicked() {
                            moved = Some((index, index + 1));
                        }
                        let up = ui.add_enabled(index > 0, egui::Button::new("⬆"));
                        if up.on_hover_text("Move up").clicked() {
                            moved = Some((index, index - 1));
                        }
                    }
                });
                if clicked {
                    action = Some(RowAction::Select(addon.id));
                }
            }
            Row::Maybe(id) => {
                let addon = self.addon_or_stand_in(*id);
                let options = RowOptions { now, selected: self.selected == Some(*id), note: None, vouch: self.vouch_score(*id) };
                let clicked = widgets::addon_row(ui, &addon, options, |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if ui.button("Drop").on_hover_text("Take off the maybe pile").clicked() {
                        action = Some(RowAction::Maybe(*id));
                    }
                    if ui.add(theme::primary_button("Add")).clicked() {
                        action = Some(RowAction::SetInDraft(*id, true));
                    }
                });
                if clicked {
                    action = Some(RowAction::Select(*id));
                }
            }
        });

        if let Some((from, to)) = moved {
            self.move_in_draft(from, to);
        }
        if let Some(id) = ticked {
            if !self.picked.remove(&id) {
                self.picked.insert(id);
            }
        }
        match bulk {
            Some(Bulk::Clear) => self.picked.clear(),
            Some(Bulk::All) => self.picked = self.draft.ids.iter().copied().collect(),
            Some(Bulk::Maybe) => self.clear_picked(&ctx, true),
            Some(Bulk::Remove) => self.clear_picked(&ctx, false),
            None => {}
        }
        self.apply(&ctx, action);
    }

    /// Name, undo and the drafts menu.
    fn draft_header(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut chosen = None;
        let mut undo = false;
        let mut redo = false;
        ui.horizontal(|ui| {
            ui.add(
                theme::text_field(&mut self.draft.name)
                    .font(egui::TextStyle::Heading)
                    .hint_text("Untitled collection")
                    .desired_width(340.0),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.menu_button("Drafts", |ui| {
                    ui.set_min_width(240.0);
                    if ui.button("New draft").clicked() {
                        chosen = Some(Menu::New);
                    }
                    if !self.library.is_empty() {
                        ui.separator();
                        ui.label(theme::small("Saved automatically"));
                    }
                    for entry in self.library.iter().take(20) {
                        let current = self.draft_path.as_ref() == Some(&entry.path);
                        let label = format!("{}  ({})", entry.name, entry.count);
                        if ui.selectable_label(current, label).clicked() && !current {
                            chosen = Some(Menu::Open(entry.path.clone()));
                        }
                    }
                    ui.separator();
                    if ui.button("Open a draft file…").clicked() {
                        chosen = Some(Menu::OpenFile);
                    }
                    if ui.button("Save a copy…").clicked() {
                        chosen = Some(Menu::SaveCopy);
                    }
                    if ui.add_enabled(self.draft_path.is_some(), egui::Button::new("Delete this draft…")).clicked() {
                        chosen = Some(Menu::Delete);
                    }
                    if chosen.is_some() {
                        ui.close();
                    }
                });
                redo = ui
                    .add_enabled(self.can_redo(), egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Y")
                    .clicked();
                undo = ui
                    .add_enabled(self.can_undo(), egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked();
            });
        });
        if undo {
            self.undo(&ctx);
        }
        if redo {
            self.redo(&ctx);
        }
        match chosen {
            Some(Menu::New) => self.new_draft(),
            Some(Menu::Open(path)) => self.open_draft(&ctx, path),
            Some(Menu::OpenFile) => {
                if let Some(path) = draft_dialog().pick_file() {
                    self.open_draft(&ctx, path);
                }
            }
            Some(Menu::SaveCopy) => {
                let name = match self.draft.name.trim() {
                    "" => "collection",
                    name => name,
                };
                if let Some(path) = draft_dialog().set_file_name(format!("{name}.json")).save_file() {
                    self.export_draft(path);
                }
            }
            Some(Menu::Delete) => self.confirm_delete = true,
            None => {}
        }
    }

    /// The list for the chosen view, with the maybe pile after it.
    fn draft_rows(&self, addons: &[Addon]) -> Vec<Row> {
        let mut order: Vec<usize> = (0..addons.len()).collect();
        let mut rows = Vec::new();
        match self.draft_view {
            DraftView::Order => rows.extend(order.into_iter().map(Row::Item)),
            DraftView::Largest => {
                order.sort_by_key(|&i| std::cmp::Reverse(addons[i].size_bytes));
                rows.extend(order.into_iter().map(Row::Item));
            }
            DraftView::Updated => {
                order.sort_by_key(|&i| std::cmp::Reverse(addons[i].time_updated));
                rows.extend(order.into_iter().map(Row::Item));
            }
            DraftView::ByType => {
                let mut group = |title: &str, members: Vec<usize>| {
                    if members.is_empty() {
                        return;
                    }
                    let size: u64 = members.iter().map(|&i| addons[i].size_bytes).sum();
                    let detail = format!("{} · {}", members.len(), format::size(size));
                    rows.push(Row::Header(title.to_string(), detail));
                    rows.extend(members.into_iter().map(Row::Item));
                };
                let ok = |i: &usize| addons[*i].status == AddonStatus::Ok;
                for kind in Kind::ALL {
                    let of_kind = |i: &usize| ok(i) && addons[*i].kind() == kind;
                    group(kind.label(), order.iter().copied().filter(of_kind).collect());
                }
                group("Unavailable", order.iter().copied().filter(|i| !ok(i)).collect());
            }
        }
        if !self.draft.maybe.is_empty() {
            let detail = format!("{} · not part of the collection", self.draft.maybe.len());
            rows.push(Row::Header("Maybe".to_string(), detail));
            rows.extend(self.draft.maybe.iter().copied().map(Row::Maybe));
        }
        rows
    }
}

fn draft_dialog() -> rfd::FileDialog {
    rfd::FileDialog::new().add_filter("Collection draft", &["json"])
}
