//! What happened to a collection's addons since they were last reviewed.

use std::collections::HashMap;

use crate::model::{Addon, AddonStatus, Snap};

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// The author uploaded a new version.
    Updated { id: u64, title: String, time_updated: u64 },
    /// It was fine before and can no longer be downloaded.
    Lost { id: u64, title: String, status: AddonStatus },
    /// It was unavailable before and is back.
    Restored { id: u64, title: String },
}

impl Change {
    pub fn id(&self) -> u64 {
        match self {
            Change::Updated { id, .. } | Change::Lost { id, .. } | Change::Restored { id, .. } => *id,
        }
    }
}

pub fn snapshot(addons: &[Addon]) -> Vec<Snap> {
    addons
        .iter()
        .map(|a| Snap { id: a.id, title: a.title.clone(), time_updated: a.time_updated, status: a.status })
        .collect()
}

/// Compares addons to an earlier snapshot. Losses first, then updates with
/// the most recent first. Addons the snapshot never saw are not changes.
pub fn diff(before: &[Snap], now: &[Addon]) -> Vec<Change> {
    let before: HashMap<u64, &Snap> = before.iter().map(|snap| (snap.id, snap)).collect();
    let mut changes = Vec::new();
    for addon in now {
        let Some(old) = before.get(&addon.id) else { continue };
        // A removed addon comes back without a title; keep the one we knew.
        let title = if addon.title.is_empty() { old.title.clone() } else { addon.title.clone() };
        let (was_ok, is_ok) = (old.status == AddonStatus::Ok, addon.status == AddonStatus::Ok);
        if was_ok && !is_ok {
            changes.push(Change::Lost { id: addon.id, title, status: addon.status });
        } else if !was_ok && is_ok {
            changes.push(Change::Restored { id: addon.id, title });
        } else if is_ok && old.time_updated > 0 && addon.time_updated > old.time_updated {
            changes.push(Change::Updated { id: addon.id, title, time_updated: addon.time_updated });
        }
    }
    changes.sort_by_key(|change| match change {
        Change::Lost { .. } => (0, 0),
        Change::Restored { .. } => (1, 0),
        Change::Updated { time_updated, .. } => (2, u64::MAX - time_updated),
    });
    changes
}

/// Adds addons the snapshot has not seen, leaving the rest as they were, so
/// a later [`diff`] still reports what changed since the last review.
pub fn remember_new(before: &mut Vec<Snap>, now: &[Addon]) {
    for snap in snapshot(now) {
        // Details that have not loaded yet would be remembered as blank.
        let loaded = snap.time_updated > 0 || snap.status != AddonStatus::Ok;
        if loaded && !before.iter().any(|old| old.id == snap.id) {
            before.push(snap);
        }
    }
}

/// The snapshot to keep once the user has seen the changes: the addons as
/// they are now. Addons whose details are not loaded, or that have lost
/// their title by being removed, keep what was known about them.
pub fn reviewed(before: &[Snap], now: &[Addon]) -> Vec<Snap> {
    snapshot(now)
        .into_iter()
        .filter_map(|mut snap| {
            let old = before.iter().find(|old| old.id == snap.id);
            let loaded = snap.time_updated > 0 || snap.status != AddonStatus::Ok;
            if !loaded {
                return old.cloned();
            }
            if snap.title.is_empty() {
                snap.title = old.map(|old| old.title.clone()).unwrap_or_default();
            }
            Some(snap)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addon(id: u64, time_updated: u64) -> Addon {
        Addon { title: format!("A{id}"), time_updated, ..Addon::unavailable(id, AddonStatus::Ok) }
    }

    #[test]
    fn reports_updates_and_losses_and_remembers_titles() {
        let before = snapshot(&[addon(1, 100), addon(2, 100), addon(3, 100), addon(4, 100)]);
        let now = [
            addon(1, 100),
            addon(2, 300),
            Addon::unavailable(3, AddonStatus::Removed),
            addon(4, 500),
            addon(5, 900), // new since the snapshot
        ];
        assert_eq!(
            diff(&before, &now),
            vec![
                Change::Lost { id: 3, title: "A3".into(), status: AddonStatus::Removed },
                Change::Updated { id: 4, title: "A4".into(), time_updated: 500 },
                Change::Updated { id: 2, title: "A2".into(), time_updated: 300 },
            ]
        );
    }

    #[test]
    fn remembering_new_addons_keeps_old_entries() {
        let mut before = snapshot(&[addon(1, 100)]);
        remember_new(&mut before, &[addon(1, 300), addon(2, 50), addon(3, 0)]);
        assert_eq!(before.len(), 2);
        assert_eq!(before[0].time_updated, 100);
        assert_eq!(before[1].id, 2);
    }

    #[test]
    fn reviewing_keeps_the_title_of_a_removed_addon() {
        let before = snapshot(&[addon(1, 100), addon(2, 100), addon(9, 100)]);
        let now = [addon(1, 300), Addon::unavailable(2, AddonStatus::Removed), addon(3, 0)];
        let after = reviewed(&before, &now);
        assert_eq!(after.len(), 2);
        assert_eq!(after[0].time_updated, 300);
        assert_eq!((after[1].title.as_str(), after[1].status), ("A2", AddonStatus::Removed));
    }

    #[test]
    fn an_addon_coming_back_is_reported() {
        let before = snapshot(&[Addon::unavailable(1, AddonStatus::Hidden)]);
        assert_eq!(diff(&before, &[addon(1, 5)]), vec![Change::Restored { id: 1, title: "A1".into() }]);
    }
}
