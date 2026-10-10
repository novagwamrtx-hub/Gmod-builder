//! The parts of Settings about how the app looks and behaves. The Steam key
//! and Garry's Mod folder cards are in `tab_export.rs`.

use eframe::egui::{self, Color32, Sense, Stroke};

use crate::app::{App, COMMUNITY_REPO};
use crate::theme::{self, Density, Look, Palette, Rgb, THEMES};

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
        let mut toggled = false;
        theme::card(ui, |ui| {
            ui.heading("Vouches");
            ui.label(theme::muted_text(
                "Vouch for addons and collections that worked well for you, and warn about ones that \
                 did not, from an addon's details. Enough builders agreeing gives it a vouch score \
                 from 0 to 100, which counts for more the more builders are behind it.",
            ));
            toggled = ui.checkbox(&mut self.settings.community, "Show vouch scores and buttons").changed();
            if !self.settings.community {
                return;
            }
            let vouched = self.votes.values().filter(|vouch| **vouch).count();
            let warned = self.votes.len() - vouched;
            ui.label(theme::small(format!(
                "On this computer: {vouched} vouched for, {warned} warned about."
            )));
            if COMMUNITY_REPO.is_empty() {
                ui.label(theme::small(
                    "Sharing opens once the project has its page on GitHub. Until then your votes stay \
                     on this computer and will be ready to send.",
                ));
                return;
            }
            ui.horizontal(|ui| {
                let label = if self.unshared_votes { "Share my vouches" } else { "Share my vouches again" };
                share = ui.add_enabled(!self.votes.is_empty(), theme::primary_button(label)).clicked();
                ui.label(theme::small(format!(
                    "Opens a filled-in page on GitHub for you to send (needs a GitHub account, which \
                     is what stops one person voting many times). {} builders' scores loaded.",
                    self.scores.len()
                )));
            });
        });
        if toggled {
            self.write_settings();
        }
        if share {
            self.share_votes(&ctx);
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
