//! Export (to Steam, to a server, as text) and Settings.

use std::collections::HashMap;

use eframe::egui;
use gmcb_core::{export, handoff, parse_workshop_id};

use crate::app::{App, KeyStatus};
use crate::settings::Settings;
use crate::theme;

/// Windows refuses to open much longer URLs; beyond this the ids travel by
/// clipboard instead of in the link.
const MAX_HANDOFF_URL: usize = 2000;
const COLLECTIONS_PAGE: &str =
    "https://steamcommunity.com/workshop/browse/?appid=4000&section=collections";
const API_KEY_PAGE: &str = "https://steamcommunity.com/dev/apikey";
/// Seconds between looks at the clipboard while waiting for a key.
const CLIPBOARD_INTERVAL: f64 = 1.0;

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
                            let url = if with_ids.len() <= MAX_HANDOFF_URL {
                                with_ids
                            } else {
                                handoff::manage_page_url(collection)
                            };
                            ui.ctx().copy_text(export::id_list(&self.draft.ids));
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                            self.info("Opened the collection in your browser; addon ids copied");
                        }
                    }
                    if !has_addons {
                        ui.label(theme::muted_text("Your draft is empty."));
                    }
                });
                ui.label(theme::small(
                    "Opens the collection's edit page, where the browser extension shows what to add \
                     and what the collection has that your draft does not. You must be logged into \
                     Steam there as the collection's owner. The ids are also copied to the clipboard.",
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
                let dialog =
                    rfd::FileDialog::new().add_filter("Lua", &["lua"]).set_file_name("workshop.lua");
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
            let installed =
                self.draft.ids.iter().filter(|id| self.installed.binary_search(id).is_ok()).count();
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

    pub(crate) fn settings_tab(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .id_salt("settings")
            .auto_shrink(false)
            .show(ui, |ui| self.settings_cards(ui));
    }

    fn settings_cards(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.watch_clipboard_for_key(&ctx);
        let mut save = false;

        theme::card(ui, |ui| {
            ui.heading("Steam Web API key");
            ui.label(theme::muted_text(
                "Discovering, searching and required-item checks use Steam's Web API, which needs a \
                 free key tied to your Steam account. Importing and inspecting collections works without one.",
            ));
            ui.horizontal(|ui| {
                ui.add(
                    theme::text_field(&mut self.key_input)
                        .password(true)
                        .hint_text("Paste your key")
                        .desired_width(340.0),
                );
                save |= ui.add(theme::primary_button("Save")).clicked();
                if self.key_input.trim() == self.settings.api_key && self.client.has_key() {
                    match &self.key_status {
                        KeyStatus::Unknown => ui.colored_label(theme::good(), "✔ Key saved"),
                        KeyStatus::Checking => ui.label(theme::muted_text("Checking with Steam…")),
                        KeyStatus::Valid => ui.colored_label(theme::good(), "✔ Steam accepts this key"),
                        KeyStatus::Invalid(why) => ui.colored_label(theme::bad(), why),
                    };
                }
            });
            if let Some(key) = self.clipboard_key.clone() {
                ui.horizontal(|ui| {
                    ui.colored_label(theme::good(), "There is a key on your clipboard.");
                    if ui.button("Use it").clicked() {
                        self.key_input = key;
                        self.clipboard_key = None;
                        save = true;
                    }
                });
            }
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to("Get your key from Steam", API_KEY_PAGE);
                ui.label(theme::small(
                    "Any domain name works, for example localhost. Copy the key and come back here.",
                ));
            });
            if let Some(path) = Settings::path() {
                ui.label(theme::small(format!("Stored unencrypted in {}", path.display())));
            }
        });

        ui.add_space(4.0);
        self.appearance_card(ui);
        ui.add_space(4.0);
        self.behaviour_card(ui);
        ui.add_space(4.0);
        self.community_card(ui);

        ui.add_space(4.0);
        theme::card(ui, |ui| {
            ui.heading("Garry's Mod on this computer");
            ui.label(theme::muted_text(
                "Used to import the addons you have installed and to compare addon files for conflicts.",
            ));
            match &self.gmod_dir {
                Some(dir) => {
                    ui.colored_label(
                        theme::good(),
                        format!("✔ Found, with {} downloaded addons", self.installed.len()),
                    );
                    ui.label(theme::small(dir.display().to_string()));
                }
                None => {
                    ui.colored_label(theme::warn(), "Not found in Steam's usual folders.");
                }
            }
            ui.horizontal(|ui| {
                ui.add(
                    theme::text_field(&mut self.gmod_input)
                        .hint_text("Only if it was not found: path of the garrysmod folder")
                        .desired_width(460.0),
                );
                if ui.button("Browse…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        self.gmod_input = dir.display().to_string();
                        save = true;
                    }
                }
                save |= ui.button("Save").clicked();
            });
        });

        if save {
            self.save_settings(&ctx);
        }
    }

    /// While the key field is empty, notices a Web API key being copied, so
    /// the user does not have to find the field to paste into.
    fn watch_clipboard_for_key(&mut self, ctx: &egui::Context) {
        if !self.key_input.trim().is_empty() {
            self.clipboard_key = None;
            return;
        }
        let now = ctx.input(|i| i.time);
        if now - self.clipboard_checked >= CLIPBOARD_INTERVAL {
            self.clipboard_checked = now;
            self.clipboard_key = arboard::Clipboard::new()
                .and_then(|mut clipboard| clipboard.get_text())
                .ok()
                .map(|text| text.trim().to_string())
                .filter(|text| looks_like_key(text));
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(CLIPBOARD_INTERVAL));
    }
}

/// Steam Web API keys are 32 hexadecimal characters.
fn looks_like_key(text: &str) -> bool {
    text.len() == 32 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::looks_like_key;

    #[test]
    fn recognises_keys() {
        assert!(looks_like_key("0123456789ABCDEF0123456789abcdef"));
        assert!(!looks_like_key("0123456789ABCDEF0123456789abcde"));
        assert!(!looks_like_key("https://steamcommunity.com/dev/a"));
    }
}
