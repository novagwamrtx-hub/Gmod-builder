//! How the desktop app passes an addon list to the browser extension: as a
//! URL fragment on Steam's own "manage collection" page. Fragments are never
//! sent to the server, so the list stays in the user's browser.

const FRAGMENT_KEY: &str = "gmcb";

/// The collection's edit page on Steam, where the extension runs.
pub fn manage_page_url(collection_id: u64) -> String {
    format!("https://steamcommunity.com/sharedfiles/managecollection/?id={collection_id}")
}

/// Link that opens the collection's edit page with the ids attached.
pub fn manage_url(collection_id: u64, ids: &[u64]) -> String {
    let list = ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
    format!("{}#{FRAGMENT_KEY}={list}", manage_page_url(collection_id))
}

/// Reads the ids back out of `location.hash` (with or without the `#`).
/// `None` if the fragment is not ours or holds anything but ids.
pub fn decode_fragment(fragment: &str) -> Option<Vec<u64>> {
    let list = fragment.trim_start_matches('#').strip_prefix(FRAGMENT_KEY)?.strip_prefix('=')?;
    if list.is_empty() {
        return Some(Vec::new());
    }
    list.split(',').map(|id| id.parse().ok()).collect()
}

/// Reads ids from text a user pasted: ids or Workshop links separated by
/// whitespace or commas. Unreadable entries are dropped, repeats collapsed.
pub fn parse_id_list(text: &str) -> Vec<u64> {
    let mut ids = Vec::new();
    for id in text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter_map(crate::model::parse_workshop_id)
    {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_lists() {
        let text = "1, 2\n2\nhttps://steamcommunity.com/sharedfiles/filedetails/?id=3 junk";
        assert_eq!(parse_id_list(text), vec![1, 2, 3]);
        assert!(parse_id_list("  ").is_empty());
    }

    #[test]
    fn round_trip() {
        let url = manage_url(99, &[1, 22, 333]);
        assert!(url.starts_with("https://steamcommunity.com/sharedfiles/managecollection/?id=99#"));
        let fragment = url.split_once('#').unwrap().1;
        assert_eq!(decode_fragment(fragment), Some(vec![1, 22, 333]));
        assert_eq!(decode_fragment(&format!("#{fragment}")), Some(vec![1, 22, 333]));
    }

    #[test]
    fn rejects_foreign_or_malformed_fragments() {
        assert_eq!(decode_fragment("#comments"), None);
        assert_eq!(decode_fragment("#gmcb=1,x"), None);
        assert_eq!(decode_fragment("#gmcb="), Some(vec![]));
    }
}
