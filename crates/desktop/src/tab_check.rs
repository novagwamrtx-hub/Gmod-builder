//! Check: problems with the draft, and what changed since it was last
//! looked at.

use eframe::egui::{self, RichText};
use gmcb_core::analysis::{self, Finding, Severity};
use gmcb_core::changes::Change;
use gmcb_core::model::workshop_url;
use gmcb_core::AddonStatus;

use crate::app::App;
use crate::format;
use crate::theme;
use crate::widgets;

/// Changes listed before "and N more".
const MAX_CHANGES: usize = 30;

enum Action {
    Add(Vec<u64>),
    Remove(u64),
    Select(u64),
    Dismiss(String),
    RestoreDismissed,
    MarkSeen,
}

impl App {
    pub(crate) fn check_tab(&mut self, ui: &mut egui::Ui) {
        if self.draft.ids.is_empty() {
            theme::empty_state(
                ui,
                "Nothing to check yet",
                "Add some addons to your draft first.",
                |_| {},
            );
            return;
        }

        let ctx = ui.ctx().clone();
        let mut recheck = false;
        ui.horizontal(|ui| {
            let label = if self.report.is_some() { "Check again" } else { "Run check" };
            if ui.add_enabled(!self.checking, theme::primary_button(label)).clicked() {
                self.run_check(&ctx, true);
            }
            ui.add_space(8.0);
            ui.label(theme::muted_text("This collection is for"));
            let current = analysis::TARGETS
                .iter()
                .find(|(key, ..)| Some(*key) == self.draft.target.as_deref())
                .map_or("Any gamemode", |(_, label, _)| label);
            egui::ComboBox::from_id_salt("target").width(140.0).selected_text(current).show_ui(ui, |ui| {
                let before = self.draft.target.clone();
                ui.selectable_value(&mut self.draft.target, None, "Any gamemode");
                for (key, label, _) in analysis::TARGETS {
                    ui.selectable_value(&mut self.draft.target, Some(key.to_string()), label);
                }
                recheck = self.draft.target != before;
            });
            if self.checking {
                ui.spinner();
                ui.label(theme::muted_text("Fetching addon details…"));
            } else if !self.client.has_key() {
                ui.colored_label(theme::warn(), "No Steam key: required items will not be checked.");
            }
        });
        if recheck && self.report.is_some() {
            self.run_check(&ctx, false);
        }

        let Some(report) = &self.report else {
            theme::empty_state(
                ui,
                "Check your collection",
                "Finds removed or private addons, missing required items, addons that overwrite each other, and what changed since last time.",
                |_| {},
            );
            return;
        };

        let dismissed = |finding: &Finding| self.draft.dismissed.contains(&finding.key());
        let open: Vec<&Finding> = report.findings.iter().filter(|f| !dismissed(f)).collect();
        let ignored = report.findings.len() - open.len();
        let count = |severity| open.iter().filter(|f| f.severity() == severity).count();
        let (errors, warnings) = (count(Severity::Error), count(Severity::Warning));
        let tint = |n, color| if n > 0 { color } else { theme::text() };
        ui.horizontal(|ui| {
            theme::stat(ui, &report.addon_count.to_string(), "addons", theme::text());
            theme::stat(ui, &format::size(report.total_size_bytes), "to download", theme::text());
            theme::stat(ui, &errors.to_string(), "errors", tint(errors, theme::bad()));
            theme::stat(ui, &warnings.to_string(), "warnings", tint(warnings, theme::warn()));
        });

        let mut action = None;
        let mut missing: Vec<u64> = Vec::new();
        for finding in &open {
            if let Finding::MissingRequirement { missing: id, .. } = finding {
                if !missing.contains(id) {
                    missing.push(*id);
                }
            }
        }
        if !missing.is_empty() {
            let label = format!("Add all {} missing required addons", missing.len());
            if ui.button(label).clicked() {
                action = Some(Action::Add(missing));
            }
        }

        let now = format::now_unix();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;

            if !self.changes.is_empty() {
                ui.horizontal(|ui| {
                    let since = format::ago(now, self.draft.snapshot_at);
                    ui.label(RichText::new(format!("Changed since {since}")).strong().color(theme::text()));
                    if ui.button("Mark as seen").clicked() {
                        action = Some(Action::MarkSeen);
                    }
                });
                for change in self.changes.iter().take(MAX_CHANGES) {
                    let (color, message) = match change {
                        Change::Updated { title, time_updated, .. } => {
                            (theme::accent(), format!("{title} was updated {}", format::ago(now, *time_updated)))
                        }
                        Change::Lost { title, status, .. } => {
                            let why = match status {
                                AddonStatus::Banned => "was banned",
                                AddonStatus::Hidden => "was made private",
                                _ => "was removed from the Workshop",
                            };
                            (theme::bad(), format!("{title} {why}"))
                        }
                        Change::Restored { title, .. } => (theme::good(), format!("{title} is available again")),
                    };
                    widgets::finding_row(ui, color, &message, |ui| {
                        if ui.button("Details").clicked() {
                            action = Some(Action::Select(change.id()));
                        }
                    });
                }
                if self.changes.len() > MAX_CHANGES {
                    ui.label(theme::small(format!("…and {} more", self.changes.len() - MAX_CHANGES)));
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Problems").strong().color(theme::text()));
            }

            if open.is_empty() {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("✔").size(36.0).color(theme::good()));
                    ui.label(RichText::new("No problems found").heading());
                });
                ui.add_space(8.0);
            }
            let shown = report.findings.iter().filter(|f| self.show_dismissed || !dismissed(f));
            for finding in shown {
                let is_dismissed = dismissed(finding);
                let color = match finding.severity() {
                    _ if is_dismissed => theme::border(),
                    Severity::Error => theme::bad(),
                    Severity::Warning => theme::warn(),
                    Severity::Info => theme::muted(),
                };
                let message = match finding {
                    // Name the missing addon once its details have arrived.
                    Finding::MissingRequirement { addon_title, missing, .. } => {
                        match self.addons.get(missing).filter(|a| !a.title.is_empty()) {
                            Some(required) => format!(
                                "{addon_title} requires {}, which is not in the collection",
                                required.title
                            ),
                            None => finding.message(),
                        }
                    }
                    _ => finding.message(),
                };
                widgets::finding_row(ui, color, &message, |ui| {
                    if is_dismissed {
                        ui.label(theme::small("Ignored"));
                        return;
                    }
                    if ui.button("Ignore").on_hover_text("Stop reporting this").clicked() {
                        action = Some(Action::Dismiss(finding.key()));
                    }
                    match finding {
                        Finding::MissingRequirement { missing, .. } => {
                            if ui.button("Add").clicked() {
                                action = Some(Action::Add(vec![*missing]));
                            }
                            ui.hyperlink_to("View", workshop_url(*missing));
                        }
                        Finding::Unavailable { id, .. } => {
                            if ui.button("Remove").clicked() {
                                action = Some(Action::Remove(*id));
                            }
                        }
                        Finding::Stale { id, .. } | Finding::OtherGamemode { id, .. } => {
                            if ui.button("Details").clicked() {
                                action = Some(Action::Select(*id));
                            }
                        }
                        Finding::FileConflict { a, b, .. } => {
                            for (id, label) in [(b, "Second"), (a, "First")] {
                                if ui.button(label).on_hover_text("Show this addon").clicked() {
                                    action = Some(Action::Select(*id));
                                }
                            }
                        }
                        Finding::RequirementsUnknown { .. } | Finding::MultipleGamemodes { .. } => {}
                    }
                });
            }

            ui.add_space(4.0);
            if ignored > 0 {
                ui.horizontal(|ui| {
                    ui.label(theme::small(format!("{ignored} ignored")));
                    let toggle = if self.show_dismissed { "Hide" } else { "Show" };
                    if ui.small_button(toggle).clicked() {
                        self.show_dismissed = !self.show_dismissed;
                    }
                    if ui.small_button("Stop ignoring").clicked() {
                        action = Some(Action::RestoreDismissed);
                    }
                });
            }
            let files = match (report.files_compared, self.gmod_dir.is_some()) {
                (0, false) => "Garry's Mod was not found on this computer, so addon files could not be compared.".to_string(),
                (0, true) => "None of these addons are downloaded by Garry's Mod on this computer, so their files could not be compared.".to_string(),
                (n, _) => format!(
                    "Compared the files of {n} of {} addons: the ones Garry's Mod has downloaded on this computer.",
                    report.addon_count
                ),
            };
            ui.label(theme::small(files));
        });

        match action {
            Some(Action::Add(ids)) => {
                self.add_many(&ctx, ids, true);
            }
            Some(Action::Remove(id)) => self.remove_from_draft(&ctx, id),
            Some(Action::Select(id)) => self.select(&ctx, id),
            Some(Action::Dismiss(key)) => self.dismiss(key),
            Some(Action::RestoreDismissed) => self.restore_dismissed(),
            Some(Action::MarkSeen) => self.mark_changes_seen(),
            None => {}
        }
    }
}
