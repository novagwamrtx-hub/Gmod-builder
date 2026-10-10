//! Fetch a public collection and print a report. Works without an API key;
//! set STEAM_API_KEY to also check required items.
//!
//!     cargo run -p gmcb-core --example check_collection -- <collection id or url>

use std::time::{SystemTime, UNIX_EPOCH};

use gmcb_core::{analysis, parse_workshop_id, SteamClient};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let arg = std::env::args().nth(1).ok_or("usage: check_collection <collection id or url>")?;
    let id = parse_workshop_id(&arg).ok_or("not a Workshop id or URL")?;

    let client = SteamClient::new(std::env::var("STEAM_API_KEY").ok());
    let ids = client.collection_items(id).await?;
    let addons = client.details(&ids).await?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let report = analysis::analyze(&addons, now, &analysis::Options::default());

    println!(
        "{} addons, {:.1} MB",
        report.addon_count,
        report.total_size_bytes as f64 / 1_048_576.0
    );
    for addon in addons.iter().take(5) {
        println!("  {} [{}]", addon.display_title(), addon.tags.join(", "));
    }
    for finding in &report.findings {
        println!("{:?}: {}", finding.severity(), finding.message());
    }
    Ok(())
}
