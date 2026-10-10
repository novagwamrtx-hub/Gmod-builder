//! Building blocks shared by the tabs: addon rows, the scrolling list they
//! sit in, and report lines.

use eframe::egui::{self, Align, Layout, Margin, RichText, Sense, UiBuilder};
use gmcb_core::{Addon, AddonStatus};

use super::theme;
use crate::{format, thumbs};

pub const HEADER_HEIGHT: f32 = 30.0;

/// Height of an addon row, which follows the density chosen in Settings.
pub fn row_height() -> f32 {
    theme::row_height()
}
const LABEL_COLUMN: [f32; 2] = [62.0, 24.0];

/// A scrolling list of rows of differing heights that only lays out the rows
/// in view, so long lists of thumbnails stay cheap.
pub fn virtual_list(
    ui: &mut egui::Ui,
    id: &str,
    heights: &[f32],
    mut add_row: impl FnMut(&mut egui::Ui, usize),
) {
    let spacing = ui.spacing().item_spacing.y;
    let total: f32 = heights.iter().map(|h| h + spacing).sum();
    egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show_viewport(ui, |ui, viewport| {
        let origin = ui.max_rect().min;
        let width = ui.available_width();
        ui.set_height(total);
        let mut y = 0.0;
        for (i, &height) in heights.iter().enumerate() {
            if y + height >= viewport.min.y && y <= viewport.max.y {
                let rect = egui::Rect::from_min_size(
                    origin + egui::vec2(0.0, y),
                    egui::vec2(width, height),
                );
                let mut row = ui.new_child(UiBuilder::new().max_rect(rect).id_salt(i));
                add_row(&mut row, i);
            }
            y += height + spacing;
        }
    });
}

/// Heading between groups of rows in a list, with a quieter detail after it.
pub fn list_header(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), HEADER_HEIGHT),
        Layout::left_to_right(Align::Max),
        |ui| {
            ui.add_space(2.0);
            ui.label(RichText::new(title).strong().color(theme::text()));
            ui.label(theme::small(detail));
        },
    );
}

/// Left-hand caption that lines up rows of controls.
pub fn row_label(ui: &mut egui::Ui, text: &str) {
    ui.allocate_ui_with_layout(LABEL_COLUMN.into(), Layout::left_to_right(Align::Center), |ui| {
        ui.set_min_size(LABEL_COLUMN.into());
        ui.label(theme::small(text));
    });
}

/// A fixed-height card that lights up under the pointer. Contents are laid
/// out right to left.
fn row_card(ui: &mut egui::Ui, selected: bool, add_contents: impl FnOnce(&mut egui::Ui)) {
    let size = egui::vec2(ui.available_width(), row_height());
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let id = ui.id().with("row");
    let lit = theme::fade(ui.ctx(), id, selected || ui.rect_contains_pointer(rect));
    let fill = theme::mix(theme::card_fill(), theme::card_hover(), lit);
    ui.painter().rect_filled(rect, theme::card_radius(), fill);
    let chosen = theme::fade(ui.ctx(), id.with("selected"), selected);
    if chosen > 0.0 {
        let stroke = egui::Stroke::new(1.0, theme::accent().gamma_multiply(chosen));
        ui.painter().rect_stroke(rect, theme::card_radius(), stroke, egui::StrokeKind::Inside);
    }
    let inner = rect.shrink2(egui::vec2(10.0, 8.0));
    let builder = UiBuilder::new().max_rect(inner).layout(Layout::right_to_left(Align::Center));
    ui.scope_builder(builder, add_contents);
}

pub struct RowOptions<'a> {
    pub now: u64,
    /// Highlight as the addon shown in the detail panel.
    pub selected: bool,
    /// Shown in place of the tags, e.g. why the addon is suggested.
    pub note: Option<&'a str>,
    /// What builders have said about the addon, if anything.
    pub vouch: Option<String>,
}

/// Thumbnail, title, stats and tags, with the caller's buttons on the right
/// (laid out right to left). Returns true if the title was clicked.
pub fn addon_row(
    ui: &mut egui::Ui,
    addon: &Addon,
    options: RowOptions,
    buttons: impl FnOnce(&mut egui::Ui),
) -> bool {
    let mut clicked = false;
    row_card(ui, options.selected, |ui| {
        buttons(ui);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            thumbnail(ui, addon, theme::thumbnail_size());
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                let title = RichText::new(addon.display_title()).size(15.0).strong().color(theme::text());
                clicked = ui
                    .add(egui::Label::new(title).truncate().sense(Sense::click()))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .on_hover_text("Show details")
                    .clicked();
                match addon.status {
                    AddonStatus::Ok if addon.time_updated == 0 => {
                        ui.label(theme::small("Loading details…"));
                    }
                    AddonStatus::Ok => {
                        let mut stats = stats_line(addon, options.now);
                        if let Some(vouch) = &options.vouch {
                            stats.push_str(" · ");
                            stats.push_str(vouch);
                        }
                        ui.add(egui::Label::new(theme::small(stats)).truncate());
                        match options.note {
                            Some(note) => {
                                ui.add(egui::Label::new(RichText::new(note).small().color(theme::accent())).truncate());
                            }
                            None => {
                                ui.horizontal(|ui| {
                                    for tag in addon.tags.iter().filter(|t| *t != "Addon").take(5) {
                                        theme::chip(ui, tag);
                                    }
                                });
                            }
                        }
                    }
                    AddonStatus::Removed => {
                        ui.colored_label(theme::bad(), "Removed from the Workshop, or private");
                    }
                    AddonStatus::Banned => {
                        ui.colored_label(theme::bad(), "Banned");
                    }
                    AddonStatus::Hidden => {
                        ui.colored_label(theme::bad(), "Not public");
                    }
                }
            });
        });
    });
    clicked
}

/// A row for a collection in search results.
pub fn collection_row(ui: &mut egui::Ui, collection: &Addon, buttons: impl FnOnce(&mut egui::Ui)) {
    row_card(ui, false, |ui| {
        buttons(ui);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            thumbnail(ui, collection, theme::thumbnail_size());
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
                let title = RichText::new(collection.display_title()).size(15.0).strong().color(theme::text());
                ui.add(egui::Label::new(title).truncate());
                let mut stats = format!(
                    "{} addons · {} favorites",
                    collection.required.as_ref().map_or(0, Vec::len),
                    format::count(collection.favorited)
                );
                if let Some(rating) = collection.rating() {
                    stats.push_str(&format!(" · {rating}% liked"));
                }
                ui.label(theme::small(stats));
                let description: String = collection.description.chars().take(160).collect();
                ui.add(egui::Label::new(theme::small(description.replace(['\n', '\r'], " "))).truncate());
            });
        });
    });
}

pub fn stats_line(addon: &Addon, now: u64) -> String {
    let mut stats = format!(
        "{} · {} subscribers",
        format::size(addon.size_bytes),
        format::count(addon.subscriptions)
    );
    if let Some(rating) = addon.rating() {
        stats.push_str(&format!(" · {rating}% liked"));
    }
    stats.push_str(&format!(" · updated {}", format::ago(now, addon.time_updated)));
    stats
}

/// The addon's picture in a square of `side`, or a quiet square while it
/// loads and when there is none.
pub fn thumbnail(ui: &mut egui::Ui, addon: &Addon, side: f32) {
    if !theme::thumbnails() {
        return;
    }
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let texture = addon.preview_url.as_ref().and_then(|url| {
        let image = egui::Image::new(thumbs::uri(url, side * ui.pixels_per_point()));
        match image.load_for_size(ui.ctx(), rect.size()) {
            Ok(egui::load::TexturePoll::Ready { texture }) => Some(texture),
            _ => None,
        }
    });
    match texture {
        Some(texture) => {
            // Whole and centred, whatever its shape.
            let scale = (side / texture.size.x).min(side / texture.size.y);
            let fitted = egui::Rect::from_center_size(rect.center(), texture.size * scale);
            egui::Image::from_texture(texture).corner_radius(theme::radius()).paint_at(ui, fitted);
        }
        None => {
            ui.painter().rect_filled(rect, theme::radius(), theme::border());
        }
    }
}

/// One line of a report: coloured dot, message, actions on the right.
pub fn finding_row(
    ui: &mut egui::Ui,
    color: egui::Color32,
    message: &str,
    actions: impl FnOnce(&mut egui::Ui),
) {
    egui::Frame::new()
        .fill(theme::card_fill())
        .corner_radius(theme::radius())
        .inner_margin(Margin::symmetric(12, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    actions(ui);
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        theme::dot(ui, color);
                        ui.add(egui::Label::new(message).wrap());
                    });
                });
            });
        });
}

/// The Add / Added button used wherever addons are offered. Returns the new
/// state if clicked.
pub fn add_button(ui: &mut egui::Ui, in_draft: bool) -> Option<bool> {
    let clicked = if in_draft {
        ui.button(RichText::new("✔ Added").color(theme::good()))
            .on_hover_text("Click to remove from the draft")
            .clicked()
    } else {
        ui.add(theme::primary_button("Add")).clicked()
    };
    clicked.then_some(!in_draft)
}

/// Toggle for the maybe pile. Returns true if clicked.
pub fn maybe_button(ui: &mut egui::Ui, on_pile: bool) -> bool {
    let (label, hint) = if on_pile {
        (RichText::new("★ Maybe").color(theme::warn()), "Take off the maybe pile")
    } else {
        (RichText::new("☆ Maybe"), "Keep for later without adding it")
    };
    ui.button(label).on_hover_text(hint).clicked()
}
