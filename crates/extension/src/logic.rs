//! The parts of the extension that do not touch the browser, so they can be
//! tested natively.

use std::collections::HashSet;

use gmcb_core::handoff;

/// Steam's CSRF token, which every write request must echo back.
pub fn session_id(cookies: &str) -> Option<&str> {
    cookies
        .split(';')
        .find_map(|cookie| cookie.trim().strip_prefix("sessionid="))
        .filter(|value| !value.is_empty())
}

/// The ids still to add: `wanted` minus what the collection already holds.
pub fn pending(wanted: &[u64], existing: &HashSet<u64>) -> Vec<u64> {
    wanted.iter().copied().filter(|id| !existing.contains(id)).collect()
}

/// What the collection holds that the list does not, lowest id first.
pub fn unlisted(wanted: &[u64], existing: &HashSet<u64>) -> Vec<u64> {
    let mut extra: Vec<u64> = existing.iter().copied().filter(|id| !wanted.contains(id)).collect();
    extra.sort_unstable();
    extra
}

/// Form body for `POST /sharedfiles/addchild` and `/sharedfiles/removechild`.
pub fn child_body(collection: u64, session: &str, child: u64) -> String {
    format!("id={collection}&sessionid={session}&childid={child}")
}

/// Steam answers `{"success":1}` when the addon was added.
pub fn is_success(body: &str) -> bool {
    success_code(body) == Some(1)
}

/// Whether a removal went through. Steam's answer to `removechild` is not
/// documented, so only an explicit failure code counts as one.
pub fn removal_succeeded(body: &str) -> bool {
    matches!(success_code(body), None | Some(1))
}

fn success_code(body: &str) -> Option<i64> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("success")
        .and_then(serde_json::Value::as_i64)
}

/// Collection members are marked up as `sharedfile_<id>`, and the user's
/// subscribed items as `choice_MySubscribedItems_<id>`.
pub fn id_from_element_id(element_id: &str) -> Option<u64> {
    element_id.rsplit('_').next()?.parse().ok()
}

/// The basket as kept in the page's local storage: ids separated by commas.
pub fn basket_from_storage(stored: &str) -> Vec<u64> {
    handoff::parse_id_list(stored)
}

pub fn basket_to_storage(ids: &[u64]) -> String {
    ids.iter().map(u64::to_string).collect::<Vec<_>>().join(",")
}

/// Puts an id in the basket or takes it out. Returns true if it is now in.
pub fn toggle(basket: &mut Vec<u64>, id: u64) -> bool {
    match basket.iter().position(|&x| x == id) {
        Some(index) => {
            basket.remove(index);
            false
        }
        None => {
            basket.push(id);
            true
        }
    }
}

/// Short description of what pressing the buttons would do.
pub fn plan(wanted: usize, to_add: usize, to_remove: usize) -> String {
    if wanted == 0 {
        return "Paste addon ids or links, one per line.".to_string();
    }
    let mut text = format!("{to_add} to add, {} already in the collection.", wanted - to_add);
    if to_remove > 0 {
        text.push_str(&format!(" The collection has {to_remove} more that are not in this list."));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_session_cookie() {
        assert_eq!(session_id("a=1; sessionid=abc123; steamCountry=RO"), Some("abc123"));
        assert_eq!(session_id("sessionid=xyz"), Some("xyz"));
        assert_eq!(session_id("a=1; mysessionid=no"), None);
        assert_eq!(session_id("sessionid="), None);
    }

    #[test]
    fn works_out_what_to_add_and_remove() {
        let existing = HashSet::from([2, 4, 9, 7]);
        assert_eq!(pending(&[1, 2, 3, 4], &existing), vec![1, 3]);
        assert_eq!(unlisted(&[1, 2, 3, 4], &existing), vec![7, 9]);
        assert_eq!(
            plan(4, 2, 2),
            "2 to add, 2 already in the collection. The collection has 2 more that are not in this list."
        );
        assert_eq!(plan(3, 3, 0), "3 to add, 0 already in the collection.");
    }

    #[test]
    fn request_and_response() {
        assert_eq!(child_body(9, "s", 7), "id=9&sessionid=s&childid=7");
        assert!(is_success(r#"{"success":1}"#));
        assert!(!is_success(r#"{"success":15}"#));
        assert!(!is_success("<html>Sign in</html>"));
        assert!(removal_succeeded(r#"{"success":1}"#));
        assert!(removal_succeeded(""));
        assert!(!removal_succeeded(r#"{"success":15}"#));
    }

    #[test]
    fn element_ids() {
        assert_eq!(id_from_element_id("sharedfile_160250458"), Some(160250458));
        assert_eq!(id_from_element_id("choice_MySubscribedItems_42"), Some(42));
        assert_eq!(id_from_element_id("sharedfile_"), None);
    }

    #[test]
    fn basket_round_trip() {
        let mut basket = basket_from_storage("3,1");
        assert!(toggle(&mut basket, 7));
        assert!(!toggle(&mut basket, 3));
        assert_eq!(basket_to_storage(&basket), "1,7");
        assert!(basket_from_storage("").is_empty());
    }
}
