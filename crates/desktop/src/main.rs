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

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 760.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };
    eframe::run_native(
        "GMod Collection Builder",
        options,
        Box::new(|cc| {
            // A draft file can be passed on the command line (e.g. "Open with").
            let draft = std::env::args_os().nth(1).map(std::path::PathBuf::from);
            Ok(Box::new(app::App::new(&cc.egui_ctx, draft)))
        }),
    )
}
