//! Drives the app through its main flows against the live Workshop and
//! renders each screen to a PNG, without opening a window. For reviewing UI
//! changes and checking that the flows still work end to end.
//!
//! Needs a GPU, network access and a saved Steam key, so it only runs when
//! asked:
//!
//!     cargo test -p gmcb-desktop render_previews -- --ignored --nocapture
//!
//! Images land in `target/ui-preview/`. Nothing is saved to the user's
//! drafts, settings or vouches.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui;
use egui_kittest::Harness;
use gmcb_core::{CollectionDraft, SteamClient};

use crate::app::{App, DraftView, RelatedKind, Tab};
use crate::theme;

const SAMPLE_COLLECTION: u64 = 3812775577;
const SIZE: [f32; 2] = [1100.0, 760.0];

fn render(app: &mut App, tab: Tab, name: &str) {
    render_at(app, tab, name, SIZE);
}

fn render_at(app: &mut App, tab: Tab, name: &str, size: [f32; 2]) {
    app.tab = tab;
    let look = app.settings.look.clone();
    let mut harness = Harness::builder()
        .with_size(size)
        .with_max_steps(40)
        .wgpu()
        .build_ui(|ui| app.show(ui));
    egui_extras::install_image_loaders(&harness.ctx);
    theme::apply(&harness.ctx, &look);
    // Thumbnails arrive over the network; give them a few rounds.
    for _ in 0..8 {
        harness.run_ok();
        std::thread::sleep(Duration::from_millis(400));
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-preview");
    std::fs::create_dir_all(&dir).unwrap();
    harness.render().unwrap().save(dir.join(format!("{name}.png"))).unwrap();
}

/// Waits for the app's background work to finish.
fn settle(app: &mut App, ctx: &egui::Context) {
    let started = Instant::now();
    loop {
        app.handle_messages(ctx);
        if app.busy == 0 {
            return;
        }
        assert!(started.elapsed() < Duration::from_secs(180), "background work did not finish");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
#[ignore = "needs a GPU, network access and a saved Steam key"]
fn render_previews() {
    let ctx = egui::Context::default();
    let mut app = App::new(&ctx, None);
    settle(&mut app, &ctx);
    // Start from nothing, whatever the user had open.
    app.draft = CollectionDraft::default();
    app.draft_path = None;
    let key = app.settings.api_key.clone();
    assert!(!key.is_empty(), "save a Steam key in the app first");

    app.client = SteamClient::new(None);
    app.key_input.clear();
    render(&mut app, Tab::Discover, "discover-no-key");
    render(&mut app, Tab::Draft, "draft-empty");
    render(&mut app, Tab::Settings, "settings");
    app.client = SteamClient::new(Some(key.clone()));
    app.key_input = key;
    render(&mut app, Tab::Discover, "discover-empty");

    // Discover, then look at the top suggestion.
    app.discover.theme = "Zombie Survival".into();
    app.start_discover(&ctx, true);
    settle(&mut app, &ctx);
    let discovery = app.discover.result.clone().expect("discovery failed");
    // Asking again is answered from disk, without the Workshop.
    app.forget_discoveries();
    app.discover.result = None;
    app.start_discover(&ctx, false);
    assert_eq!(app.busy, 0, "a saved discovery was fetched again");
    let ids = |d: &gmcb_core::discover::Discovery| d.suggestions.iter().map(|s| s.id).collect::<Vec<_>>();
    assert_eq!(app.discover.result.as_ref().map(ids), Some(ids(&discovery)));
    assert!(app.discover.result_from.is_some());
    println!("discover: {} collections, {} suggestions", discovery.sample_size, discovery.suggestions.len());
    for suggestion in discovery.suggestions.iter().take(12) {
        let addon = &app.addons[&suggestion.id];
        println!("  {:>3}  {:<14} {}", suggestion.in_sample, addon.kind().label(), addon.title);
    }
    render(&mut app, Tab::Discover, "discover");
    app.discover.ideas_open = true;
    render(&mut app, Tab::Discover, "discover-ideas");
    app.discover.theme = "star".into();
    app.discover.ideas_open = true;
    render(&mut app, Tab::Discover, "discover-ideas-typed");
    app.discover.theme = "Zombie Survival".into();
    app.discover.ideas_open = false;

    let top = discovery.suggestions[0].id;
    app.select(&ctx, top);
    app.votes.insert(top, true);
    app.load_related(&ctx, top, RelatedKind::BuiltFor);
    settle(&mut app, &ctx);
    let built_for = app.related[&top].built_for.clone().unwrap_or_default();
    println!("built for {}: {} addons, e.g. {:?}", app.title_of(top), built_for.len(),
        built_for.iter().take(3).map(|&id| app.title_of(id)).collect::<Vec<_>>());
    render(&mut app, Tab::Discover, "discover-detail");
    app.selected = None;

    // Adding something built on a base brings the base along.
    app.add_to_draft(&ctx, built_for[0]);
    settle(&mut app, &ctx);
    assert!(app.draft.contains(top), "the required base was not added");
    app.undo(&ctx);
    assert!(app.draft.ids.is_empty(), "undo did not take the addon and its base back out");

    // Search for addons and for collections.
    app.search.text = "wiremod".into();
    app.start_search(&ctx, false);
    settle(&mut app, &ctx);
    render(&mut app, Tab::Search, "search");
    app.search.text = "zombie".into();
    app.search.collections = true;
    app.search.sort = gmcb_core::Sort::TopRated;
    app.start_search(&ctx, false);
    settle(&mut app, &ctx);
    render(&mut app, Tab::Search, "search-collections");
    let collection = app.search.results[0].clone();
    app.browse_collection(&ctx, &collection);
    settle(&mut app, &ctx);
    render(&mut app, Tab::Search, "search-browse");

    // Build a draft: suggestions (which pull in what they require), an
    // imported collection and a maybe pile.
    app.draft.name = "Zombie night".into();
    for suggestion in discovery.suggestions.iter().take(6) {
        app.add_to_draft(&ctx, suggestion.id);
        settle(&mut app, &ctx);
    }
    println!("6 suggestions became {} addons with their requirements", app.draft.ids.len());
    assert!(app.import(&ctx, &SAMPLE_COLLECTION.to_string()));
    settle(&mut app, &ctx);
    for suggestion in discovery.suggestions.iter().skip(6).take(2) {
        app.toggle_maybe(&ctx, suggestion.id);
    }
    settle(&mut app, &ctx);
    app.picked.extend(app.draft.ids.iter().take(2).copied());
    render(&mut app, Tab::Draft, "draft");
    let before = app.draft.ids.len();
    app.clear_picked(&ctx, true);
    assert_eq!((app.draft.ids.len(), app.draft.maybe.len()), (before - 2, 4));
    app.undo(&ctx);
    assert_eq!(app.draft.ids.len(), before);
    app.draft_view = DraftView::ByType;
    render(&mut app, Tab::Draft, "draft-by-type");

    render(&mut app, Tab::Check, "check-before");
    app.draft.target = Some("sandbox".into());
    app.run_check(&ctx, true);
    settle(&mut app, &ctx);
    render(&mut app, Tab::Check, "check");

    // The addons installed here have their files on disk, so this exercises
    // conflict detection and the trimmed workshop.lua on real archives.
    if !app.installed.is_empty() {
        app.draft = CollectionDraft { name: "Installed".into(), ..Default::default() };
        app.import_installed(&ctx);
        settle(&mut app, &ctx);
        app.run_check(&ctx, true);
        settle(&mut app, &ctx);
        let report = app.report.clone().unwrap();
        println!("installed: {} addons, files of {} compared, {} findings",
            report.addon_count, report.files_compared, report.findings.len());
        let conflicts = report
            .findings
            .iter()
            .filter(|f| matches!(f, gmcb_core::analysis::Finding::FileConflict { .. }));
        for finding in conflicts.take(8) {
            println!("  {:?}: {}", finding.severity(), finding.message());
        }
        let lua = gmcb_core::export::workshop_lua(&app.draft_addons(), &app.archives);
        println!("workshop.lua: {} sent, {} maps on demand, {} script-only left out",
            lua.matches("\nresource.AddWorkshop").count(),
            lua.matches("\n    [\"").count(),
            lua.matches("\n--   ").count());
        render(&mut app, Tab::Check, "check-installed");
    }

    app.collection_input = SAMPLE_COLLECTION.to_string();
    app.draft.collection_id = Some(SAMPLE_COLLECTION);
    render(&mut app, Tab::Export, "export");
    // Saving and reopening, on a file outside the user's drafts.
    let remembered = app.settings.last_draft.clone();
    let file = std::env::temp_dir().join("gmcb-preview-draft.json");
    app.draft_path = Some(file.clone());
    app.save_now();
    let written: CollectionDraft =
        serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
    let before = app.draft.clone();
    app.new_draft();
    let blank = app.draft.clone();
    app.open_draft(&ctx, file.clone());
    // Put the user's settings back before anything can fail.
    app.settings.last_draft = remembered;
    app.write_settings();
    std::fs::remove_file(file).unwrap();
    assert_eq!(written, before);
    assert!(!written.snapshot.is_empty(), "the check did not leave a snapshot to compare against");
    assert_eq!(blank, CollectionDraft::default());
    assert_eq!(app.draft, written);

    app.key_status = crate::app::KeyStatus::Valid;
    render_at(&mut app, Tab::Settings, "settings-with-key", [1100.0, 1500.0]);

    // Every theme, and the other densities. Only in memory: not saved.
    app.start_discover(&ctx, false);
    for (i, (name, _)) in theme::THEMES.iter().enumerate().skip(1) {
        app.settings.look.theme = name.to_string();
        app.settings.look.density = [theme::Density::Compact, theme::Density::Roomy][i % 2];
        let file = format!("theme-{}", name.to_lowercase().replace(' ', "-"));
        render(&mut app, if i % 2 == 0 { Tab::Discover } else { Tab::Draft }, &file);
    }
    app.settings.look = theme::Look { thumbnails: false, animations: false, rounding: 0, scale: 1.25, ..Default::default() };
    render(&mut app, Tab::Discover, "look-plain");
}
