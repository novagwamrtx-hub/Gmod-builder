//! Colours, sizes, motion and the handful of custom widgets that give the app
//! its look. What the look is comes from the user's settings ([`Look`]); the
//! rest of the app asks for it through the functions here.

use std::sync::RwLock;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, Margin, RichText, Stroke, TextStyle, Vec2,
};
use serde::{Deserialize, Serialize};

pub type Rgb = [u8; 3];

/// The colours a theme is made of. Everything else is derived from these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Palette {
    pub bg: Rgb,
    pub bar: Rgb,
    pub card: Rgb,
    pub card_hover: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub accent: Rgb,
    pub good: Rgb,
    pub warn: Rgb,
    pub bad: Rgb,
    pub button: Rgb,
    pub field: Rgb,
}

const fn rgb(hex: u32) -> Rgb {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

#[rustfmt::skip]
const fn palette(c: [u32; 13]) -> Palette {
    Palette {
        bg: rgb(c[0]), bar: rgb(c[1]), card: rgb(c[2]), card_hover: rgb(c[3]), border: rgb(c[4]),
        text: rgb(c[5]), muted: rgb(c[6]), accent: rgb(c[7]), good: rgb(c[8]), warn: rgb(c[9]),
        bad: rgb(c[10]), button: rgb(c[11]), field: rgb(c[12]),
    }
}

/// The app's own theme first, then six well-loved editor themes.
#[rustfmt::skip]
pub const THEMES: [(&str, Palette); 7] = [
    ("Midnight", palette([0x15171c, 0x1b1e25, 0x20242d, 0x272c37, 0x2e3440, 0xdce0e8, 0x8a93a3, 0x4a9eff, 0x62c487, 0xe6b450, 0xec6a70, 0x2b303b, 0x101216])),
    ("One Dark", palette([0x282c34, 0x21252b, 0x2c313a, 0x333842, 0x3e4451, 0xc8ccd4, 0x7f848e, 0x61afef, 0x98c379, 0xe5c07b, 0xe06c75, 0x3a3f4b, 0x1d2025])),
    ("Dracula", palette([0x282a36, 0x21222c, 0x303341, 0x3a3d4e, 0x44475a, 0xf8f8f2, 0x8f9ac6, 0xbd93f9, 0x50fa7b, 0xf1fa8c, 0xff5555, 0x44475a, 0x1e1f29])),
    ("Catppuccin Mocha", palette([0x1e1e2e, 0x181825, 0x28283b, 0x313244, 0x45475a, 0xcdd6f4, 0x9399b2, 0xcba6f7, 0xa6e3a1, 0xf9e2af, 0xf38ba8, 0x313244, 0x11111b])),
    ("Tokyo Night", palette([0x1a1b26, 0x16161e, 0x1f2335, 0x292e42, 0x3b4261, 0xc0caf5, 0x7982a9, 0x7aa2f7, 0x9ece6a, 0xe0af68, 0xf7768e, 0x292e42, 0x13141c])),
    ("Gruvbox", palette([0x282828, 0x1d2021, 0x32302f, 0x3c3836, 0x504945, 0xebdbb2, 0xa89984, 0xfe8019, 0xb8bb26, 0xfabd2f, 0xfb4934, 0x3c3836, 0x1d2021])),
    ("Nord", palette([0x2e3440, 0x272c36, 0x3b4252, 0x434c5e, 0x4c566a, 0xeceff4, 0x9aa5b8, 0x88c0d0, 0xa3be8c, 0xebcb8b, 0xbf616a, 0x434c5e, 0x242933])),
];

pub fn preset(name: &str) -> Palette {
    THEMES.iter().find(|(theme, _)| *theme == name).unwrap_or(&THEMES[0]).1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Density {
    Compact,
    Comfortable,
    Roomy,
}

impl Density {
    pub const ALL: [Density; 3] = [Density::Compact, Density::Comfortable, Density::Roomy];

    pub fn label(self) -> &'static str {
        match self {
            Density::Compact => "Compact",
            Density::Comfortable => "Comfortable",
            Density::Roomy => "Roomy",
        }
    }

    /// Height of an addon row and the side of its thumbnail.
    fn row(self) -> (f32, f32) {
        match self {
            Density::Compact => (62.0, 46.0),
            Density::Comfortable => (80.0, 64.0),
            Density::Roomy => (98.0, 82.0),
        }
    }
}

/// How the app looks and moves, as chosen in Settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    /// One of [`THEMES`] by name.
    pub theme: String,
    /// The user's own colours, replacing the theme's.
    pub colors: Option<Palette>,
    /// Size of everything, 1.0 being normal.
    pub scale: f32,
    pub density: Density,
    /// Corner roundness of buttons and fields, in points.
    pub rounding: u8,
    pub animations: bool,
    /// 1.0 is normal; higher is snappier.
    pub animation_speed: f32,
    pub thumbnails: bool,
    /// Width of the addon detail panel.
    pub detail_width: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            theme: THEMES[0].0.to_string(),
            colors: None,
            scale: 1.0,
            density: Density::Comfortable,
            rounding: 6,
            animations: true,
            animation_speed: 1.0,
            thumbnails: true,
            detail_width: 320.0,
        }
    }
}

impl Look {
    pub fn palette(&self) -> Palette {
        self.colors.unwrap_or_else(|| preset(&self.theme))
    }

    /// Seconds a transition takes; zero with animations off.
    fn animation_time(&self) -> f32 {
        if self.animations {
            0.16 / self.animation_speed.clamp(0.25, 4.0)
        } else {
            0.0
        }
    }
}

/// The look in use, resolved to what the widgets need.
#[derive(Clone, Copy)]
struct Active {
    palette: Palette,
    radius: u8,
    row_height: f32,
    thumbnail: f32,
    animation_time: f32,
    thumbnails: bool,
    detail_width: f32,
}

static ACTIVE: RwLock<Active> = RwLock::new(Active {
    palette: THEMES[0].1,
    radius: 6,
    row_height: 80.0,
    thumbnail: 64.0,
    animation_time: 0.16,
    thumbnails: true,
    detail_width: 320.0,
});

fn active() -> Active {
    *ACTIVE.read().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn color(rgb: Rgb) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// `a` moved `t` of the way (0 to 1) towards `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgba_premultiplied(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
        channel(a.a(), b.a()),
    )
}

macro_rules! palette_colors {
    ($($name:ident),*) => {
        $(pub fn $name() -> Color32 { color(active().palette.$name) })*
    };
}
palette_colors!(bg, bar, card_hover, border, text, muted, accent, good, warn, bad);

fn button() -> Color32 {
    color(active().palette.button)
}

fn button_hover() -> Color32 {
    mix(button(), text(), 0.08)
}

/// The accent, toned down to sit behind text.
pub fn accent_dim() -> Color32 {
    mix(bg(), accent(), 0.3)
}

/// Text on top of the accent colour: whichever of dark or white reads better.
fn on_accent() -> Color32 {
    let [r, g, b] = active().palette.accent;
    let luminance = 0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b);
    if luminance > 140.0 {
        mix(bg(), Color32::BLACK, 0.4)
    } else {
        Color32::WHITE
    }
}

pub fn radius() -> u8 {
    active().radius
}

pub fn card_radius() -> u8 {
    active().radius.saturating_add(2)
}

pub fn row_height() -> f32 {
    active().row_height
}

pub fn thumbnail_size() -> f32 {
    active().thumbnail
}

pub fn thumbnails() -> bool {
    active().thumbnails
}

pub fn detail_width() -> f32 {
    active().detail_width
}

pub fn animation_time() -> f32 {
    active().animation_time
}

/// 0 to 1, easing towards 1 while `on` and back while not.
pub fn fade(ctx: &egui::Context, id: egui::Id, on: bool) -> f32 {
    ctx.animate_bool_with_time(id, on, animation_time())
}

pub fn apply(ctx: &egui::Context, look: &Look) {
    let (row_height, thumbnail) = look.density.row();
    *ACTIVE.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = Active {
        palette: look.palette(),
        radius: look.rounding.min(16),
        row_height,
        thumbnail,
        animation_time: look.animation_time(),
        thumbnails: look.thumbnails,
        detail_width: look.detail_width.clamp(240.0, 520.0),
    };
    ctx.set_zoom_factor(look.scale.clamp(0.7, 1.6));
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::proportional(20.0)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Button, FontId::proportional(14.5)),
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 6.0);
        style.spacing.interact_size.y = 30.0;
        style.interaction.selectable_labels = false;
        style.animation_time = animation_time();

        let v = &mut style.visuals;
        v.panel_fill = bg();
        v.window_fill = bar();
        v.window_stroke = Stroke::new(1.0, border());
        v.extreme_bg_color = color(active().palette.field);
        v.faint_bg_color = card_fill();
        v.hyperlink_color = accent();
        v.warn_fg_color = warn();
        v.error_fg_color = bad();
        v.weak_text_color = Some(muted());
        v.selection.bg_fill = accent_dim();
        v.selection.stroke = Stroke::new(1.0, accent());
        v.window_corner_radius = CornerRadius::same(card_radius());
        v.menu_corner_radius = CornerRadius::same(radius());

        let corners = CornerRadius::same(radius());
        let bright = mix(text(), Color32::WHITE, 0.6);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = bg();
        w.noninteractive.bg_stroke = Stroke::new(1.0, border());
        w.noninteractive.fg_stroke = Stroke::new(1.0, text());
        w.inactive.bg_fill = button_hover();
        w.inactive.weak_bg_fill = button();
        w.inactive.bg_stroke = Stroke::new(1.0, border());
        w.inactive.fg_stroke = Stroke::new(1.0, text());
        w.hovered.bg_fill = button_hover();
        w.hovered.weak_bg_fill = button_hover();
        w.hovered.bg_stroke = Stroke::new(1.0, border());
        w.hovered.fg_stroke = Stroke::new(1.0, bright);
        w.active.bg_fill = mix(button(), text(), 0.16);
        w.active.weak_bg_fill = mix(button(), text(), 0.16);
        w.active.bg_stroke = Stroke::new(1.0, accent());
        w.active.fg_stroke = Stroke::new(1.0, bright);
        w.open = w.hovered;
        for visuals in [
            &mut w.noninteractive,
            &mut w.inactive,
            &mut w.hovered,
            &mut w.active,
            &mut w.open,
        ] {
            visuals.corner_radius = corners;
            visuals.expansion = 0.0;
        }
    });
}

pub fn muted_text(text: impl Into<String>) -> RichText {
    RichText::new(text).color(muted())
}

pub fn small(text: impl Into<String>) -> RichText {
    RichText::new(text).small().color(muted())
}

/// The one button per screen that is the obvious next step.
pub fn primary_button<'a>(text: &str) -> egui::Button<'a> {
    egui::Button::new(RichText::new(text).color(on_accent()).strong()).fill(accent())
}

/// A tab in the top bar. The selected one is marked by the sliding
/// underline drawn in `tabs.rs`; the label brightens as it is approached.
pub fn tab(ui: &mut egui::Ui, selected: bool, label: &str) -> egui::Response {
    let id = ui.next_auto_id();
    let hovered = ui.ctx().read_response(id).is_some_and(|response| response.hovered());
    let lit = fade(ui.ctx(), id.with("lit"), selected || hovered);
    let label = RichText::new(label).color(mix(muted(), mix(text(), Color32::WHITE, 0.5), lit));
    let fill = mix(Color32::TRANSPARENT, accent_dim(), fade(ui.ctx(), id.with("on"), selected));
    ui.add(egui::Button::new(label).fill(fill).stroke(Stroke::NONE))
}

/// A single-line text box with the app's padding.
pub fn text_field(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).margin(Margin::symmetric(8, 6))
}

/// A small read-only pill, used for an addon's tags.
pub fn chip(ui: &mut egui::Ui, text: &str) {
    egui::Frame::new()
        .fill(button())
        .corner_radius(9)
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| ui.label(small(text)));
}

/// A pill that toggles, used for search filters. Returns true when clicked.
pub fn toggle_chip(ui: &mut egui::Ui, on: bool, text: &str) -> bool {
    filter_chip(ui, on.then_some(true), text).clicked()
}

/// A search-filter pill with three states: off (`None`), must have
/// (`Some(true)`) and must not have (`Some(false)`).
pub fn filter_chip(ui: &mut egui::Ui, state: Option<bool>, label: &str) -> egui::Response {
    let id = ui.next_auto_id();
    let on = fade(ui.ctx(), id.with("on"), state == Some(true));
    let off = fade(ui.ctx(), id.with("off"), state == Some(false));
    let fill = mix(button(), accent_dim(), on);
    let ink = mix(mix(muted(), mix(text(), Color32::WHITE, 0.5), on), bad(), off);
    let edge = mix(mix(Color32::TRANSPARENT, accent(), on), bad(), off);
    let label = if state == Some(false) { format!("− {label}") } else { label.to_string() };
    let button = egui::Button::new(RichText::new(label).size(13.0).color(ink))
        .fill(fill)
        .stroke(Stroke::new(1.0, edge))
        .corner_radius(12)
        .min_size(Vec2::new(0.0, 24.0));
    ui.add(button)
}

/// A full-width panel grouping related controls.
pub fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(card_fill())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(card_radius())
        .inner_margin(14)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

/// The colour of cards; `card` is the widget.
pub fn card_fill() -> Color32 {
    color(active().palette.card)
}

/// A big number with a caption, for summaries.
pub fn stat(ui: &mut egui::Ui, value: &str, label: &str, color: Color32) {
    egui::Frame::new()
        .fill(card_fill())
        .stroke(Stroke::new(1.0, border()))
        .corner_radius(card_radius())
        .inner_margin(Margin::symmetric(16, 10))
        .show(ui, |ui| {
            ui.set_min_width(110.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.label(RichText::new(value).size(22.0).strong().color(color));
                ui.label(small(label));
            });
        });
}

/// Centred message for a screen with nothing to show yet.
pub fn empty_state(ui: &mut egui::Ui, title: &str, body: &str, add_action: impl FnOnce(&mut egui::Ui)) {
    ui.with_layout(Layout::top_down(Align::Center), |ui| {
        ui.add_space((ui.available_height() * 0.28).max(24.0));
        ui.label(RichText::new(title).heading().color(text()));
        ui.label(muted_text(body));
        ui.add_space(4.0);
        add_action(ui);
    });
}

/// A coloured dot, used to mark a finding's severity.
pub fn dot(ui: &mut egui::Ui, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.5, color);
}
