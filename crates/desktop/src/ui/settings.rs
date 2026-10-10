//! Settings: the Steam key, how the app looks and behaves, vouches, where
//! Garry's Mod is on this computer, and what this app is.

use eframe::egui::{self, Color32, Sense, Stroke};
use gmcb_core::updates;

use super::theme::{self, Density, Look, Palette, Rgb, THEMES};
use crate::app::{App, KeyStatus, PROJECT_REPO};
use crate::settings::Settings;

const API_KEY_PAGE: &str = "https://steamcommunity.com/dev/apikey";
/// Seconds between looks at the clipboard while waiting for a key.
const CLIPBOARD_INTERVAL: f64 = 1.0;
const SWATCH: [f32; 2] = [150.0, 54.0];

/// What a control did this frame: `changed` to show it at once, `settled`
/// once the user has let go, to save it.
#[derive(Default)]
struct Edit {
    changed: bool,
    settled: bool,
}

impl Edit {
    fn note(&mut self, response: &egui::Response) {
        self.changed |= response.changed();
        self.settled |= response.drag_stopped() || (response.changed() && !response.dragged());
    }
}

impl App {
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
                        .hint_text("Only if it was not found: the GarrysMod folder in your Steam library")
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

        ui.add_space(4.0);
        self.about_card(ui);

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

    pub(crate) fn appearance_card(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut edit = Edit::default();
        // The size is applied on release: resizing everything under the
        // pointer while it drags the slider makes the slider jump about.
        let mut resized = false;

        theme::card(ui, |ui| {
            ui.heading("Appearance");
            let look = &mut self.settings.look;

            ui.horizontal_wrapped(|ui| {
                for (name, palette) in THEMES {
                    let on = look.colors.is_none() && look.theme == name;
                    if theme_swatch(ui, name, palette, on).clicked() {
                        look.theme = name.to_string();
                        look.colors = None;
                        edit.changed = true;
                        edit.settled = true;
                    }
                }
            });

            egui::CollapsingHeader::new("Change individual colours").id_salt("colours").show(ui, |ui| {
                let mut colors = look.palette();
                let mut recolored = Edit::default();
                egui::Grid::new("colour-grid").num_columns(4).spacing([10.0, 6.0]).show(ui, |ui| {
                    let slots: [(&str, &mut Rgb); 13] = [
                        ("Background", &mut colors.bg),
                        ("Top and side bars", &mut colors.bar),
                        ("Cards", &mut colors.card),
                        ("Card under the pointer", &mut colors.card_hover),
                        ("Borders", &mut colors.border),
                        ("Text", &mut colors.text),
                        ("Quiet text", &mut colors.muted),
                        ("Accent", &mut colors.accent),
                        ("Good", &mut colors.good),
                        ("Warning", &mut colors.warn),
                        ("Problem", &mut colors.bad),
                        ("Buttons", &mut colors.button),
                        ("Text fields", &mut colors.field),
                    ];
                    for (i, (label, slot)) in slots.into_iter().enumerate() {
                        let picker = ui.color_edit_button_srgb(slot);
                        recolored.changed |= picker.changed();
                        ui.label(theme::small(label));
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
                if recolored.changed {
                    look.colors = Some(colors);
                    edit.changed = true;
                    edit.settled = true;
                }
                if look.colors.is_some() && ui.button(format!("Back to {}", look.theme)).clicked() {
                    look.colors = None;
                    edit.changed = true;
                    edit.settled = true;
                }
            });

            egui::Grid::new("look-grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label(theme::muted_text("Size"));
                let mut percent = (look.scale * 100.0).round();
                let size = ui.add(egui::Slider::new(&mut percent, 80.0..=150.0).suffix("%").step_by(5.0).fixed_decimals(0));
                look.scale = percent / 100.0;
                resized |= size.drag_stopped() || (size.changed() && !size.dragged());
                ui.end_row();

                ui.label(theme::muted_text("Rows"));
                ui.horizontal(|ui| {
                    for density in Density::ALL {
                        if theme::toggle_chip(ui, look.density == density, density.label()) {
                            look.density = density;
                            edit.changed = true;
                            edit.settled = true;
                        }
                    }
                });
                ui.end_row();

                ui.label(theme::muted_text("Corners"));
                edit.note(&ui.add(egui::Slider::new(&mut look.rounding, 0..=14).suffix(" px")));
                ui.end_row();

                ui.label(theme::muted_text("Detail panel"));
                edit.note(&ui.add(
                    egui::Slider::new(&mut look.detail_width, 260.0..=480.0).suffix(" px").step_by(10.0),
                ));
                ui.end_row();

                ui.label(theme::muted_text("Motion"));
                ui.horizontal(|ui| {
                    edit.note(&ui.checkbox(&mut look.animations, "Animations"));
                    let speed = egui::Slider::new(&mut look.animation_speed, 0.4..=3.0)
                        .logarithmic(true)
                        .custom_formatter(|speed, _| match speed {
                            s if s < 0.75 => "Relaxed".to_string(),
                            s if s > 1.5 => "Snappy".to_string(),
                            _ => "Normal".to_string(),
                        });
                    edit.note(&ui.add_enabled(look.animations, speed));
                });
                ui.end_row();

                ui.label(theme::muted_text("Pictures"));
                edit.note(&ui.checkbox(&mut look.thumbnails, "Show addon thumbnails"));
                ui.end_row();
            });

            if *look != Look::default() && ui.button("Reset appearance").clicked() {
                *look = Look::default();
                edit.changed = true;
                edit.settled = true;
                resized = true;
            }
        });

        if edit.changed || resized {
            theme::apply(&ctx, &self.settings.look);
        }
        if edit.settled || resized {
            self.look_changed(&ctx);
        }
    }

    pub(crate) fn behaviour_card(&mut self, ui: &mut egui::Ui) {
        let mut edit = Edit::default();
        theme::card(ui, |ui| {
            ui.heading("Finding and checking");
            let settings = &mut self.settings;
            egui::Grid::new("behaviour-grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label(theme::muted_text("Adding"));
                let label = "Also add the items an addon requires";
                edit.note(&ui.checkbox(&mut settings.auto_requirements, label));
                ui.end_row();

                ui.label(theme::muted_text("Discover reads"));
                ui.horizontal(|ui| {
                    let mut collections = settings.discover_pages * 100;
                    let slider = egui::Slider::new(&mut collections, 100..=500).step_by(100.0);
                    edit.note(&ui.add(slider.suffix(" collections")));
                    settings.discover_pages = collections / 100;
                    ui.label(theme::small("More is steadier but slower."));
                });
                ui.end_row();

                ui.label(theme::muted_text("Discover shows"));
                let per_type = egui::Slider::new(&mut settings.discover_per_type, 3..=40);
                edit.note(&ui.add(per_type.suffix(" per type")));
                ui.end_row();

                ui.label(theme::muted_text("Check flags"));
                let stale = egui::Slider::new(&mut settings.stale_after_years, 0..=15).custom_formatter(
                    |years, _| match years as u64 {
                        0 => "no addon for its age".to_string(),
                        years => format!("addons untouched for {years} years"),
                    },
                );
                edit.note(&ui.add(stale));
                ui.end_row();
            });
        });
        if edit.settled {
            self.write_settings();
        }
    }

    pub(crate) fn community_card(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let mut share = false;
        let mut start_over = false;
        let mut toggled = false;
        theme::card(ui, |ui| {
            ui.heading("Vouches");
            ui.label(theme::muted_text(
                "Builders telling each other what works. Click the name of an addon and press Vouch \
                 if it worked well in a collection of yours, or Warn if it was broken or clashed \
                 with other addons. In Draft you can tick several addons and vouch for them \
                 together. Once three builders have voted on something it gets a vouch score from \
                 0 to 100, which counts for more the more builders are behind it.",
            ));
            toggled = ui.checkbox(&mut self.settings.community, "Show vouch scores and buttons").changed();
            if !self.settings.community {
                return;
            }
            let vouched = self.votes.values().filter(|vouch| **vouch).count();
            let warned = self.votes.len() - vouched;
            let (next, waiting) = self.unshared_votes();
            let mut mine = format!("On this computer: {vouched} vouched for, {warned} warned about.");
            if waiting > 0 {
                mine.push_str(&format!(" {waiting} not shared yet."));
            }
            ui.label(theme::small(mine));
            ui.horizontal(|ui| {
                let label = match next {
                    0 => "Nothing new to share".to_string(),
                    next if next < waiting => format!("Share the next {next}"),
                    next => format!("Share {next}"),
                };
                share = ui.add_enabled(next > 0, theme::primary_button(&label)).clicked();
                if !self.shared.is_empty() {
                    let hint = "Forgets what has been shared, so that everything is sent again. For \
                                when you closed the GitHub page without pressing Create.";
                    start_over = ui.button("Start over").on_hover_text(hint).clicked();
                }
            });
            ui.label(theme::small(format!(
                "Sharing opens a filled-in page on GitHub: press Create there to send it. It needs a \
                 GitHub account, which is what stops one person voting many times, and what you \
                 send is public under that account. Scores are loaded for {} addons and collections.",
                self.scores.len()
            )));
        });
        if toggled {
            self.write_settings();
        }
        if share {
            self.share_votes(&ctx);
        }
        if start_over {
            self.share_from_scratch();
        }
    }

    fn about_card(&mut self, ui: &mut egui::Ui) {
        let mut toggled = false;
        theme::card(ui, |ui| {
            ui.heading("About");
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::muted_text(concat!("GMod Collection Builder ", env!("CARGO_PKG_VERSION"))));
                if let Some(tag) = &self.update {
                    let newer = format!("Version {} is out.", tag.trim_start_matches(['v', 'V']));
                    ui.colored_label(theme::good(), newer);
                    ui.hyperlink_to("Download it", updates::release_page(PROJECT_REPO));
                }
            });
            let label = "Look for a newer version when the app starts";
            toggled = ui.checkbox(&mut self.settings.check_updates, label).changed();
            ui.horizontal_wrapped(|ui| {
                let project = format!("https://github.com/{PROJECT_REPO}");
                ui.hyperlink_to("Project page", &project);
                ui.hyperlink_to("Report a problem", format!("{project}/issues"));
                ui.hyperlink_to("All versions", format!("{project}/releases"));
            });
            ui.label(theme::small(
                "Free and open source, under the Apache 2.0 licence. Made by fans: not affiliated \
                 with Valve or Facepunch Studios.",
            ));
        });
        if toggled {
            self.write_settings();
        }
    }
}

/// A theme to pick: its name on its own colours, with its accents as dots.
fn theme_swatch(ui: &mut egui::Ui, name: &str, palette: Palette, selected: bool) -> egui::Response {
    let rgb = |c: Rgb| Color32::from_rgb(c[0], c[1], c[2]);
    let (rect, response) = ui.allocate_exact_size(SWATCH.into(), Sense::click());
    let lit = theme::fade(ui.ctx(), response.id.with("lit"), selected || response.hovered());
    let edge = theme::mix(rgb(palette.border), rgb(palette.accent), lit);
    let painter = ui.painter();
    painter.rect_filled(rect, theme::card_radius(), rgb(palette.bg));
    painter.rect_stroke(
        rect,
        theme::card_radius(),
        Stroke::new(if selected { 2.0 } else { 1.0 }, edge),
        egui::StrokeKind::Inside,
    );
    painter.text(
        rect.left_top() + egui::vec2(12.0, 9.0),
        egui::Align2::LEFT_TOP,
        name,
        egui::FontId::proportional(13.5),
        rgb(palette.text),
    );
    for (i, dot) in [palette.accent, palette.good, palette.warn, palette.bad, palette.muted].into_iter().enumerate() {
        let centre = rect.left_bottom() + egui::vec2(18.0 + 16.0 * i as f32, -14.0);
        painter.circle_filled(centre, 5.0, rgb(dot));
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
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
