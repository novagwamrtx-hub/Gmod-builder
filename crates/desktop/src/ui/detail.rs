//! The panel beside every tab that shows one addon the way its Workshop
//! page does: its pictures, its numbers and its description, and with them
//! what it requires, what is built for it, and the vouch buttons.

use eframe::egui::{self, Align, Layout, RichText, Sense};
use gmcb_core::bbcode::{self, Block, Span};
use gmcb_core::community::MIN_VOTES;
use gmcb_core::AddonStatus;

use super::{theme, widgets, RowAction};
use crate::app::{App, RelatedKind, Tab};
use crate::{format, thumbs};

/// As wide as the panel gets when it is asked to be wide.
pub(super) const WIDE: f32 = 760.0;
/// What a wide panel leaves for the tab beside it.
pub(super) const BESIDE_WIDE: f32 = 520.0;
/// A description of more lines than this starts out folded to this many.
const FOLDED_LINES: usize = 12;
/// Not worth folding for the sake of this many lines.
const FOLD_SLACK: usize = 4;
const STRIP_PICTURE: egui::Vec2 = egui::vec2(64.0, 36.0);

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
        let mut widen = false;
        let mut picked = None;
        let mut fold = None;
        let page = self.pages.get(&id);
        let pictures = match page {
            Some(page) if !page.pictures.is_empty() => page.pictures.as_slice(),
            _ => addon.preview_url.as_slice(),
        };

        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                close = ui.button("Close").on_hover_text("Esc").clicked();
                let (label, hint) = match self.detail_wide {
                    true => ("»", "Back to the narrow panel"),
                    false => ("«", "Wider, with room for the pictures and the description"),
                };
                widen = ui.button(label).on_hover_text(hint).clicked();
                ui.with_layout(Layout::top_down(Align::Min), |ui| {
                    ui.add(egui::Label::new(RichText::new(addon.display_title()).heading()).wrap());
                });
            });
        });

        egui::ScrollArea::vertical().id_salt("detail").auto_shrink(false).show(ui, |ui| {
            picked = gallery(ui, pictures, self.picture);
            let videos = page.map(|page| page.videos.as_slice()).unwrap_or_default();
            if !videos.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(theme::small("Videos on YouTube"));
                    for (i, video) in videos.iter().enumerate() {
                        let address = format!("https://www.youtube.com/watch?v={video}");
                        ui.hyperlink_to((i + 1).to_string(), address).on_hover_text("Opens in your browser");
                    }
                });
            }
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
                    if let Some(page) = page {
                        let mut seen = format!(
                            "{} views · {} comments",
                            format::count(page.views),
                            format::count(page.comments)
                        );
                        if !page.author.is_empty() {
                            seen = format!("By {} · {seen}", page.author);
                        }
                        ui.add(egui::Label::new(theme::small(seen)).wrap());
                    }
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
            // Until the page is here, the few lines that came with the listing.
            let blocks = bbcode::parse(page.map_or(&addon.description, |page| &page.description));
            if !blocks.is_empty() {
                ui.add_space(2.0);
                let long = blocks.len() > FOLDED_LINES + FOLD_SLACK;
                let folded = long && !self.description_open;
                description(ui, if folded { &blocks[..FOLDED_LINES] } else { &blocks });
                if long {
                    let label = if folded { "Show the whole description" } else { "Show less" };
                    if ui.button(label).clicked() {
                        fold = Some(folded);
                    }
                }
            }
            if self.page_is_loading(id) {
                ui.label(theme::small("Loading the rest of the page…"));
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
        if widen {
            self.detail_wide = !self.detail_wide;
        }
        if let Some(picked) = picked {
            self.picture = picked;
        }
        if let Some(open) = fold {
            self.description_open = open;
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

/// The addon's pictures: one large and, when there are several, the others
/// in a strip to pick from. Returns the picture that was picked, if any.
fn gallery(ui: &mut egui::Ui, pictures: &[String], shown: usize) -> Option<usize> {
    if !theme::thumbnails() || pictures.is_empty() {
        return None;
    }
    let shown = shown.min(pictures.len() - 1);
    let several = pictures.len() > 1;
    let mut picked = None;

    let width = ui.available_width();
    let size = egui::vec2(width, (width * 9.0 / 16.0).round());
    let (rect, response) =
        ui.allocate_exact_size(size, if several { Sense::click() } else { Sense::hover() });
    let backdrop = theme::mix(theme::bg(), egui::Color32::BLACK, 0.5);
    ui.painter().rect_filled(rect, theme::radius(), backdrop);
    widgets::paint_picture(ui, rect, &pictures[shown]);
    if !several {
        return None;
    }
    let hint = format!("Picture {} of {}. Click for the next one", shown + 1, pictures.len());
    if response.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(hint).clicked() {
        picked = Some((shown + 1) % pictures.len());
    }
    egui::ScrollArea::horizontal().id_salt("pictures").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, url) in pictures.iter().enumerate() {
                let response = widgets::picture(ui, url, STRIP_PICTURE, Sense::click());
                if i == shown {
                    let stroke = egui::Stroke::new(2.0, theme::accent());
                    ui.painter().rect_stroke(
                        response.rect,
                        theme::radius(),
                        stroke,
                        egui::StrokeKind::Inside,
                    );
                }
                if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    picked = Some(i);
                }
            }
        });
    });
    picked
}

/// A description, laid out the way its markup asks.
fn description(ui: &mut egui::Ui, blocks: &[Block]) {
    // In a scope of its own: `line` changes the spacing.
    ui.scope(|ui| description_blocks(ui, blocks));
}

fn description_blocks(ui: &mut egui::Ui, blocks: &[Block]) {
    for (i, block) in blocks.iter().enumerate() {
        match block {
            Block::Text(spans) => line(ui, spans, false, false),
            Block::Heading(spans) => {
                ui.add_space(4.0);
                line(ui, spans, true, false);
            }
            Block::Item(spans) => line(ui, spans, false, true),
            Block::Quote(spans) => {
                ui.indent(("quote", i), |ui| line(ui, spans, false, false));
            }
            Block::Code(code) => {
                egui::Frame::new()
                    .fill(theme::card_fill())
                    .corner_radius(theme::radius())
                    .inner_margin(6)
                    .show(ui, |ui| {
                        let code = RichText::new(code).monospace().color(theme::muted());
                        ui.add(egui::Label::new(code).wrap());
                    });
            }
            Block::Image(address) => described_picture(ui, address),
            Block::Rule => {
                ui.separator();
            }
            Block::Gap => ui.add_space(6.0),
        }
    }
}

/// One line of a description. It wraps, and its links can be clicked.
fn line(ui: &mut egui::Ui, spans: &[Span], heading: bool, item: bool) {
    // Rows are as tall as a button unless told otherwise, which is too
    // far apart for running text.
    ui.spacing_mut().interact_size.y = 0.0;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        if item {
            ui.label(theme::muted_text("•  "));
        }
        for span in spans {
            let mut text = RichText::new(&span.text);
            if heading {
                text = text.size(16.0);
            }
            if span.italic {
                text = text.italics();
            }
            if span.underline {
                text = text.underline();
            }
            if span.strike {
                text = text.strikethrough();
            }
            match &span.link {
                // Shown with where it leads, which the text need not say.
                Some(address) => {
                    ui.hyperlink_to(text, address).on_hover_text(address);
                }
                None => {
                    let strong = heading || span.bold;
                    ui.label(text.color(if strong { theme::text() } else { theme::muted() }));
                }
            }
        }
    });
}

/// A picture inside a description. Only Steam's own servers are asked for
/// pictures; one kept anywhere else is a link to it instead.
fn described_picture(ui: &mut egui::Ui, address: &str) {
    if !theme::thumbnails() || !thumbs::is_on_steam(address) {
        ui.hyperlink_to("Picture", address).on_hover_text(address);
        return;
    }
    // How large a picture is shows only once it has loaded. Until then it
    // gets a little room, and is not asked for before that room is in view.
    let id = egui::Id::new(("described picture", address));
    let known: Option<egui::Vec2> = ui.data(|data| data.get_temp(id));
    let width = ui.available_width();
    let size = match known {
        Some(pixels) => {
            let natural = pixels / ui.pixels_per_point();
            natural * (width / natural.x).min(1.0)
        }
        None => egui::vec2(width, 40.0),
    };
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let image = egui::Image::new(thumbs::uri(address, width * ui.pixels_per_point()));
    match image.load_for_size(ui.ctx(), rect.size()) {
        Ok(egui::load::TexturePoll::Ready { texture }) if known.is_some() => {
            egui::Image::from_texture(texture).corner_radius(theme::radius()).paint_at(ui, rect);
        }
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            ui.data_mut(|data| data.insert_temp(id, texture.size));
            ui.ctx().request_repaint();
        }
        Ok(_) => {
            ui.painter().rect_filled(rect, theme::radius(), theme::card_fill());
        }
        Err(_) => {}
    }
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(title).strong().color(theme::text()));
}
