//! Checks on a draft collection.
//!
//! Most checks use Workshop metadata only. Where the caller can supply the
//! file lists of addons (read from `.gma` archives on disk), addons that
//! overwrite each other's files are reported too.

use std::collections::{HashMap, HashSet};

use crate::gma;
use crate::model::{Addon, AddonStatus};

const SECONDS_PER_YEAR: u64 = 365 * 24 * 60 * 60;
/// A path shipped by more addons than this is a shared library file, not a
/// conflict worth listing pair by pair.
const MAX_OWNERS: usize = 6;
const MAX_CONFLICTS: usize = 40;

/// Gamemodes a collection can be declared to be for: (key, label, phrases
/// that mark an addon's title as made for it).
pub const TARGETS: [(&str, &str, &[&str]); 7] = [
    ("sandbox", "Sandbox", &[]),
    ("ttt", "TTT", &["ttt", "ttt2", "trouble in terrorist town"]),
    ("darkrp", "DarkRP", &["darkrp", "dark rp"]),
    ("prophunt", "Prop Hunt", &["prophunt", "prop hunt"]),
    ("deathrun", "Deathrun", &["deathrun"]),
    ("jailbreak", "Jailbreak", &["jailbreak", "jb"]),
    ("zs", "Zombie Survival", &["zombie survival", "zs"]),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// Removed, banned or hidden: subscribers and servers cannot download it.
    Unavailable { id: u64, title: String, status: AddonStatus },
    /// An addon lists a required item that is not in the collection.
    MissingRequirement { addon: u64, addon_title: String, missing: u64 },
    /// Details were fetched without a key, so requirements were not checked.
    RequirementsUnknown { count: usize },
    /// Two addons ship files at the same paths; one silently wins.
    FileConflict { a: u64, a_title: String, b: u64, b_title: String, files: usize, example: String, scripts: bool },
    /// More than one gamemode; usually a mistake in a server collection.
    MultipleGamemodes { ids: Vec<u64> },
    /// The title says it is for a gamemode other than the collection's.
    OtherGamemode { id: u64, title: String, made_for: &'static str },
    Stale { id: u64, title: String, years: u64 },
}

impl Finding {
    pub fn severity(&self) -> Severity {
        match self {
            Finding::Unavailable { .. } => Severity::Error,
            Finding::MissingRequirement { .. }
            | Finding::RequirementsUnknown { .. }
            | Finding::FileConflict { scripts: true, .. } => Severity::Warning,
            Finding::FileConflict { .. }
            | Finding::MultipleGamemodes { .. }
            | Finding::OtherGamemode { .. }
            | Finding::Stale { .. } => Severity::Info,
        }
    }

    /// Stable name for this finding, so a user's "ignore this" survives
    /// re-running the check.
    pub fn key(&self) -> String {
        match self {
            Finding::Unavailable { id, .. } => format!("unavailable:{id}"),
            Finding::MissingRequirement { addon, missing, .. } => format!("missing:{addon}:{missing}"),
            Finding::RequirementsUnknown { .. } => "requirements-unknown".to_string(),
            Finding::FileConflict { a, b, .. } => format!("conflict:{a}:{b}"),
            Finding::MultipleGamemodes { .. } => "gamemodes".to_string(),
            Finding::OtherGamemode { id, .. } => format!("other-gamemode:{id}"),
            Finding::Stale { id, .. } => format!("stale:{id}"),
        }
    }

    pub fn message(&self) -> String {
        match self {
            Finding::Unavailable { title, status, .. } => {
                let why = match status {
                    AddonStatus::Removed => "was removed from the Workshop or is private",
                    AddonStatus::Banned => "is banned",
                    AddonStatus::Hidden => "is not public",
                    AddonStatus::Ok => "is unavailable",
                };
                format!("{title} {why}")
            }
            Finding::MissingRequirement { addon_title, missing, .. } => {
                format!("{addon_title} requires addon {missing}, which is not in the collection")
            }
            Finding::RequirementsUnknown { count } => format!(
                "Required items were not checked for {count} addon(s): add a Steam Web API key to enable this"
            ),
            Finding::FileConflict { a_title, b_title, files, example, .. } => {
                let (noun, more) = match files {
                    1 => ("file", String::new()),
                    n => ("files", format!(" and {} more", n - 1)),
                };
                format!("{a_title} and {b_title} contain the same {noun}: {example}{more}")
            }
            Finding::MultipleGamemodes { ids } => {
                format!("The collection contains {} gamemodes", ids.len())
            }
            Finding::OtherGamemode { title, made_for, .. } => {
                format!("{title} looks made for {made_for}")
            }
            Finding::Stale { title, years, .. } => {
                format!("{title} has not been updated in {years} years")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Flag addons not updated for this many years. 0 disables the check.
    pub stale_after_years: u64,
    /// Key from [`TARGETS`]: the gamemode the collection is for.
    pub target: Option<String>,
    /// File lists of the addons they are known for.
    pub files: HashMap<u64, Vec<String>>,
}

impl Default for Options {
    fn default() -> Self {
        Self { stale_after_years: 8, target: None, files: HashMap::new() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Most severe first.
    pub findings: Vec<Finding>,
    pub addon_count: usize,
    /// Download size of the available addons.
    pub total_size_bytes: u64,
    /// Addons whose files were compared for conflicts.
    pub files_compared: usize,
}

/// `addons` is the full contents of the collection; `now` is Unix seconds.
pub fn analyze(addons: &[Addon], now: u64, options: &Options) -> Report {
    let present: HashSet<u64> = addons.iter().map(|a| a.id).collect();
    let mut findings = Vec::new();
    let mut unknown_requirements = 0;
    let mut gamemodes = Vec::new();
    let mut total_size_bytes = 0;

    for addon in addons {
        if addon.status != AddonStatus::Ok {
            findings.push(Finding::Unavailable {
                id: addon.id,
                title: addon.display_title(),
                status: addon.status,
            });
            continue;
        }
        total_size_bytes += addon.size_bytes;

        match &addon.required {
            Some(required) => {
                for &missing in required.iter().filter(|id| !present.contains(id)) {
                    findings.push(Finding::MissingRequirement {
                        addon: addon.id,
                        addon_title: addon.display_title(),
                        missing,
                    });
                }
            }
            None => unknown_requirements += 1,
        }

        if addon.has_tag("gamemode") {
            gamemodes.push(addon.id);
        } else if let Some(target) = &options.target {
            if let Some((_, made_for, _)) = made_for(&addon.title).filter(|(key, ..)| key != target) {
                findings.push(Finding::OtherGamemode { id: addon.id, title: addon.display_title(), made_for });
            }
        }

        let years = now.saturating_sub(addon.time_updated) / SECONDS_PER_YEAR;
        if options.stale_after_years > 0 && addon.time_updated > 0 && years >= options.stale_after_years
        {
            findings.push(Finding::Stale { id: addon.id, title: addon.display_title(), years });
        }
    }

    if unknown_requirements > 0 {
        findings.push(Finding::RequirementsUnknown { count: unknown_requirements });
    }
    if gamemodes.len() > 1 {
        findings.push(Finding::MultipleGamemodes { ids: gamemodes });
    }
    findings.extend(file_conflicts(addons, &options.files));
    findings.sort_by_key(Finding::severity);

    let files_compared = addons.iter().filter(|a| options.files.contains_key(&a.id)).count();
    Report { findings, addon_count: addons.len(), total_size_bytes, files_compared }
}

/// The gamemode a title names, if any, as an entry of [`TARGETS`].
fn made_for(title: &str) -> Option<(&'static str, &'static str, &'static [&'static str])> {
    // Words separated by single spaces, so phrases match whole words only:
    // "ttt_minecraft_b5" and "[TTT] Knife" both contain " ttt ".
    let words: Vec<String> = title
        .to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let spaced = format!(" {} ", words.join(" "));
    TARGETS
        .into_iter()
        .find(|(_, _, phrases)| phrases.iter().any(|p| spaced.contains(&format!(" {p} "))))
}

/// Pairs of addons that ship files at the same paths, worst first.
fn file_conflicts(addons: &[Addon], files: &HashMap<u64, Vec<String>>) -> Vec<Finding> {
    let mut owners: HashMap<&str, Vec<u64>> = HashMap::new();
    for addon in addons {
        for path in files.get(&addon.id).into_iter().flatten() {
            // Every addon may carry its own description file.
            if path.contains('/') {
                owners.entry(path.as_str()).or_default().push(addon.id);
            }
        }
    }
    struct Shared<'a> {
        files: usize,
        example: &'a str,
        scripts: bool,
    }
    let mut pairs: HashMap<(u64, u64), Shared> = HashMap::new();
    for (path, ids) in owners {
        if !(2..=MAX_OWNERS).contains(&ids.len()) {
            continue;
        }
        let script = gma::is_script(path);
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                let shared = pairs.entry((a.min(b), a.max(b))).or_insert(Shared {
                    files: 0,
                    example: path,
                    scripts: false,
                });
                shared.files += 1;
                // Prefer showing a script, and otherwise a stable example.
                if (script && !shared.scripts) || (script == shared.scripts && path < shared.example) {
                    shared.example = path;
                }
                shared.scripts |= script;
            }
        }
    }
    let title = |id| {
        addons.iter().find(|a| a.id == id).map(Addon::display_title).unwrap_or_default()
    };
    let mut found: Vec<Finding> = pairs
        .into_iter()
        .map(|((a, b), shared)| Finding::FileConflict {
            a,
            a_title: title(a),
            b,
            b_title: title(b),
            files: shared.files,
            example: shared.example.to_string(),
            scripts: shared.scripts,
        })
        .collect();
    found.sort_by_key(|f| match f {
        Finding::FileConflict { a, b, files, scripts, .. } => (!scripts, usize::MAX - files, *a, *b),
        _ => unreachable!("only conflicts are built here"),
    });
    found.truncate(MAX_CONFLICTS);
    found
}

/// Required items that are absent from the collection, each listed once.
pub fn missing_requirements(report: &Report) -> Vec<u64> {
    let mut seen = HashSet::new();
    report
        .findings
        .iter()
        .filter_map(|f| match f {
            Finding::MissingRequirement { missing, .. } => Some(*missing),
            _ => None,
        })
        .filter(|id| seen.insert(*id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_790_000_000;

    fn addon(id: u64) -> Addon {
        Addon {
            title: format!("A{id}"),
            size_bytes: 100,
            time_updated: NOW,
            required: Some(Vec::new()),
            ..Addon::unavailable(id, AddonStatus::Ok)
        }
    }

    fn check(addons: &[Addon]) -> Report {
        analyze(addons, NOW, &Options::default())
    }

    #[test]
    fn clean_collection_has_no_findings() {
        let report = check(&[addon(1), addon(2)]);
        assert!(report.findings.is_empty());
        assert_eq!(report.total_size_bytes, 200);
        assert_eq!(report.addon_count, 2);
    }

    #[test]
    fn reports_missing_requirements_once_each() {
        let mut a = addon(1);
        a.required = Some(vec![2, 9]);
        let mut b = addon(2);
        b.required = Some(vec![9]);
        let report = check(&[a, b]);
        assert_eq!(report.findings.len(), 2);
        assert_eq!(missing_requirements(&report), vec![9]);
        assert_eq!(report.findings[0].key(), "missing:1:9");
    }

    #[test]
    fn unavailable_addons_come_first_and_do_not_count_toward_size() {
        let mut old = addon(1);
        old.time_updated = NOW - 9 * SECONDS_PER_YEAR;
        let gone = Addon::unavailable(2, AddonStatus::Removed);
        let report = check(&[old, gone]);
        assert_eq!(report.total_size_bytes, 100);
        assert!(matches!(report.findings[0], Finding::Unavailable { id: 2, .. }));
        assert!(matches!(report.findings[1], Finding::Stale { id: 1, years: 9, .. }));
        assert_eq!(report.findings[0].message(), "Addon 2 was removed from the Workshop or is private");
    }

    #[test]
    fn keyless_data_is_flagged_not_treated_as_clean() {
        let mut a = addon(1);
        a.required = None;
        assert_eq!(check(&[a]).findings, vec![Finding::RequirementsUnknown { count: 1 }]);
    }

    #[test]
    fn flags_several_gamemodes() {
        let mut a = addon(1);
        a.tags = vec!["Gamemode".into()];
        let mut b = addon(2);
        b.tags = vec!["gamemode".into()];
        assert_eq!(check(&[a, b]).findings, vec![Finding::MultipleGamemodes { ids: vec![1, 2] }]);
    }

    #[test]
    fn flags_addons_made_for_another_gamemode() {
        let titled = |id, title: &str| Addon { title: title.into(), ..addon(id) };
        let addons = [
            titled(1, "[TTT] Golden Deagle"),
            titled(2, "ttt_minecraft_b5"),
            titled(3, "DarkRP Printers"),
            titled(4, "Attribute Editor"), // contains "ttt" inside a word
            titled(5, "Wiremod"),
            // A gamemode itself is left to the several-gamemodes check.
            Addon { tags: vec!["gamemode".into()], ..titled(6, "DarkRP") },
        ];
        let options = Options { target: Some("ttt".into()), ..Default::default() };
        let report = analyze(&addons, NOW, &options);
        assert_eq!(
            report.findings,
            vec![Finding::OtherGamemode { id: 3, title: "DarkRP Printers".into(), made_for: "DarkRP" }]
        );
        // With no declared gamemode nothing is guessed.
        assert!(check(&addons).findings.is_empty());
        // A sandbox collection flags anything gamemode-specific.
        let options = Options { target: Some("sandbox".into()), ..Default::default() };
        assert_eq!(analyze(&addons, NOW, &options).findings.len(), 3);
    }

    #[test]
    fn reports_addons_that_overwrite_each_other() {
        let files = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let options = Options {
            files: HashMap::from([
                (1, files(&["lua/weapons/base.lua", "models/gun.mdl", "addon.json"])),
                (2, files(&["lua/weapons/base.lua", "models/gun.mdl", "addon.json"])),
                (3, files(&["models/gun.mdl", "materials/x.vmt"])),
            ]),
            ..Default::default()
        };
        let report = analyze(&[addon(1), addon(2), addon(3), addon(4)], NOW, &options);
        assert_eq!(report.files_compared, 3);
        assert_eq!(report.findings.len(), 3);
        assert_eq!(
            report.findings[0],
            Finding::FileConflict {
                a: 1,
                a_title: "A1".into(),
                b: 2,
                b_title: "A2".into(),
                files: 2,
                example: "lua/weapons/base.lua".into(),
                scripts: true,
            }
        );
        assert_eq!(report.findings[0].severity(), Severity::Warning);
        assert_eq!(
            report.findings[0].message(),
            "A1 and A2 contain the same files: lua/weapons/base.lua and 1 more"
        );
        assert_eq!(report.findings[1].severity(), Severity::Info);
    }
}
