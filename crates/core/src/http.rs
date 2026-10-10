//! One HTTP client, set up the same way for everything that is downloaded.

/// Sent with every request. GitHub's API refuses requests that do not say
/// who is asking.
pub const USER_AGENT: &str = concat!("gmod-collection-builder/", env!("CARGO_PKG_VERSION"));

/// A client that gives up on a server that does not answer, instead of
/// leaving a spinner on screen for good.
pub fn client() -> reqwest::Client {
    let builder = reqwest::Client::builder().user_agent(USER_AGENT);
    #[cfg(not(target_arch = "wasm32"))]
    let builder = builder
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10));
    builder.build().unwrap_or_default()
}

/// Waits before a request is tried again: longer each time.
pub(crate) async fn pause_before_retry(attempt: u32) {
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(std::time::Duration::from_millis(500 * u64::from(attempt * attempt))).await;
    #[cfg(target_arch = "wasm32")]
    let _ = attempt;
}
