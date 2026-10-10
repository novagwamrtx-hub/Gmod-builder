//! Export: to Steam through the browser extension, to a server as files,
//! and as text for a description.

use std::collections::HashMap;

use eframe::egui;
use gmcb_core::{export, handoff, parse_workshop_id};

use super::theme;
use crate::app::App;

/// Windows refuses to open much longer URLs; beyond this the ids travel by
/// clipboard instead of in the link.
const MAX_HANDOFF_URL: usize = 2000;
const COLLECTIONS_PAGE: &str = "https://steamcommunity.com/workshop/browse/?appid=4000&section=collections";

impl App {
    pub(crate) fn export_tab(&mut self, ui: &mut egui::Ui) {
        let collection = self.draft.collection_id;
        let has_addons = !self.draft.ids.is_empty();

        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            theme::card(ui, |ui| {
                ui.heading("Send to Steam");
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::muted_text("1.  Create a collection on Steam, or use one you have."));
                    ui.hyperlink_to("Open Workshop collections", COLLECTIONS_PAGE);
                });
                ui.horizontal(|ui| {
                    ui.label(theme::muted_text("2.  Paste its link"));
                    let input = ui.add(
                        theme::text_field(&mut self.collection_input)
                            .hint_text("https://steamcommunity.com/sharedfiles/filedetails/?id=…")
                            .desired_width(400.0),
                    );
                    if input.changed() {
                        self.draft.collection_id = parse_workshop_id(&self.collection_input);
                    }
                    if self.draft.collection_id.is_some() {
                        ui.colored_label(theme::good(), "✔");
                    } else if !self.collection_input.trim().is_empty() {
                        ui.colored_label(theme::bad(), "Not a collection link");
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(theme::muted_text("3. "));
                    let send = theme::primary_button("Send to Steam");
                    if ui.add_enabled(collection.is_some() && has_addons, send).clicked() {
                        if let Some(collection) = collection {
                            let with_ids = handoff::manage_url(collection, &self.draft.ids);
                            let fits = with_ids.len() <= MAX_HANDOFF_URL;
                            let url = if fits { with_ids } else { handoff::manage_page_url(collection) };
                            ui.ctx().copy_text(export::id_list(&self.draft.ids));
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            self.info(if fits {
                                "Opened the collection in your browser"
                            } else {
                                "Opened the collection. The list is too long for a link: paste it into the extension's panel"
                            });
                        }
                    }
                    if !has_addons {
                        ui.label(theme::muted_text("Your draft is empty."));
                    }
                });
                ui.label(theme::small(
                    "Opens the collection's edit page, where the browser extension shows what to add \
                     and what the collection has that your draft does not. You must be logged into \
                     Steam there as the collection's owner. The ids are also copied to the clipboard, \
                     to paste into the extension's panel when a draft is too long to travel in the link.",
                ));
            });

            ui.add_space(4.0);
            theme::card(ui, |ui| {
                ui.heading("For a server");
                ui.add_enabled_ui(has_addons, |ui| self.server_exports(ui));
            });

            ui.add_space(4.0);
            theme::card(ui, |ui| {
                ui.heading("Describe it");
                ui.label(theme::muted_text(
                    "The addons as a list grouped by type, to paste into the collection's description or a chat.",
                ));
                ui.add_enabled_ui(has_addons, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Copy for Steam").clicked() {
                            ui.ctx().copy_text(export::description_bbcode(&self.draft_addons()));
                            self.info("Description copied, formatted for Steam");
                        }
                        if ui.button("Copy for Discord").clicked() {
                            ui.ctx().copy_text(export::description_markdown(&self.draft_addons()));
                            self.info("Description copied, formatted for Discord");
                        }
                    });
                });
            });
        });
    }

    fn server_exports(&mut self, ui: &mut egui::Ui) {
        let collection = self.draft.collection_id;
        egui::Grid::new("server").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            if ui.button("Save workshop.lua…").clicked() {
                self.load_archives();
                let dialog = rfd::FileDialog::new().add_filter("Lua", &["lua"]).set_file_name("workshop.lua");
                if let Some(path) = dialog.save_file() {
                    let none = HashMap::new();
                    let archives = if self.trim_lua { &self.archives } else { &none };
                    let lua = export::workshop_lua(&self.draft_addons(), archives);
                    match std::fs::write(&path, lua) {
                        Ok(()) => self.info(format!("Saved {}", path.display())),
                        Err(e) => self.error(format!("Could not save file: {e}")),
                    }
                }
            }
            ui.label(theme::muted_text(
                "Makes joining players download the addons. Goes in garrysmod/lua/autorun/server/",
            ));
            ui.end_row();

            ui.label("");
            let installed = self.draft.ids.iter().filter(|&&id| self.installed.has_files(id)).count();
            let hint = format!(
                "Leaves out script-only addons, which players get from the server anyway, and sends \
                 each map only while the server is on it. Works for the {installed} of {} addons \
                 that Garry's Mod has downloaded on this computer; the rest are always sent.",
                self.draft.ids.len()
            );
            ui.add_enabled(
                installed > 0,
                egui::Checkbox::new(&mut self.trim_lua, theme::muted_text("Only send what players need")),
            )
            .on_hover_text(hint)
            .on_disabled_hover_text("Needs the addons to be downloaded by Garry's Mod on this computer");
            ui.end_row();

            let copy = egui::Button::new("Copy launch option");
            if ui.add_enabled(collection.is_some(), copy).clicked() {
                if let Some(collection) = collection {
                    ui.ctx().copy_text(export::launch_options(collection));
                    self.info("Launch option copied");
                }
            }
            ui.label(theme::muted_text(
                "Makes the server download the collection. Needs the collection link above.",
            ));
            ui.end_row();

            if ui.button("Copy addon ids").clicked() {
                ui.ctx().copy_text(export::id_list(&self.draft.ids));
                self.info("Addon ids copied");
            }
            ui.label(theme::muted_text("One id per line."));
            ui.end_row();
        });
    }
}
