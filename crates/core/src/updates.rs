//! Finding out whether a newer version of the app has been released.

/// The page where the latest release of `repo` ("owner/name") can be
/// downloaded.
pub fn release_page(repo: &str) -> String {
    format!("https://github.com/{repo}/releases/latest")
}

/// True when the release tagged `tag` ("v1.4.0") is newer than the version
/// `current` ("1.3.2"). Tags that are not version numbers are never newer.
pub fn is_newer(tag: &str, current: &str) -> bool {
    match (parse(tag), parse(current)) {
        (Some(released), Some(running)) => released > running,
        _ => false,
    }
}

/// Major, minor and patch. What follows a `-` or `+` is not compared.
fn parse(version: &str) -> Option<[u32; 3]> {
    let version = version.trim().trim_start_matches(['v', 'V']);
    let numbers = version.split(['-', '+']).next()?;
    let mut parts = numbers.split('.');
    let mut parsed = [0; 3];
    for (index, slot) in parsed.iter_mut().enumerate() {
        match parts.next() {
            Some(part) => *slot = part.parse().ok()?,
            None if index == 0 => return None,
            None => {}
        }
    }
    Some(parsed)
}

/// The tag of the latest release of `repo`, or `None` if it has none yet.
#[cfg(feature = "client")]
pub async fn latest(repo: &str) -> Result<Option<String>, String> {
    let failed = |e: reqwest::Error| e.without_url().to_string();
    let url = format!("https://api.github.com/repos/{repo}/releases/latest");
    let request = crate::http::client().get(url).header("accept", "application/vnd.github+json");
    let response = request.send().await.map_err(failed)?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let text = response.error_for_status().map_err(failed)?.text().await.map_err(failed)?;
    let release: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(release["tag_name"].as_str().map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.3"), "ten is more than nine");
        assert!(is_newer("1.0", "0.9.9"));
        assert!(is_newer("v1.2.3-beta", "1.2.2"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.2.0"));
        assert!(!is_newer("nightly", "0.1.0"));
        assert!(!is_newer("", "0.1.0"));
        assert!(!is_newer("v1.x", "0.1.0"));
    }
}
