#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod files;
mod format;
mod gmod;
mod ideas;
#[cfg(test)]
mod preview;
mod settings;
mod thumbs;
mod ui;

use eframe::egui;

/// Shown on the window and in the taskbar.
const ICON: &[u8] = include_bytes!("../../../assets/icon-256.png");

fn main() -> eframe::Result {
    let mut viewport =
        egui::ViewportBuilder::default().with_inner_size([1100.0, 760.0]).with_min_inner_size([720.0, 480.0]);
    // Failing that, the window keeps the icon the toolkit gives every app.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        viewport = viewport.with_icon(icon);
    }
    eframe::run_native(
        "GMod Collection Builder",
        eframe::NativeOptions { viewport, ..Default::default() },
        Box::new(|cc| {
            // A draft file can be passed on the command line (e.g. "Open with").
            let draft = std::env::args_os().nth(1).map(std::path::PathBuf::from);
            Ok(Box::new(app::App::new(&cc.egui_ctx, draft)))
        }),
    )
}
