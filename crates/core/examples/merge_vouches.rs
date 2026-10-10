//! Folds one vouch submission into the community ledger and rewrites the
//! published totals. Run by `.github/workflows/vouches.yml` for each issue
//! titled "Vouches":
//!
//!     cargo run -p gmcb-core --no-default-features --example merge_vouches -- <account> <body file> <community dir>

use std::path::Path;
use std::process::ExitCode;

use gmcb_core::community::{self, Ledger};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [account, body, dir] = args.as_slice() else {
        eprintln!("usage: merge_vouches <account> <body file> <community dir>");
        return ExitCode::FAILURE;
    };
    match run(account, Path::new(body), Path::new(dir)) {
        Ok(count) => {
            println!("{count}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// Returns how many votes the submission carried.
fn run(account: &str, body: &Path, dir: &Path) -> Result<usize, String> {
    let votes = community::decode(&std::fs::read_to_string(body).map_err(|e| e.to_string())?);
    let count = votes.len();
    let ledger_path = dir.join("ledger.json");
    let mut ledger: Ledger = match std::fs::read_to_string(&ledger_path) {
        Ok(text) => serde_json::from_str(&text).map_err(|e| format!("ledger.json: {e}"))?,
        Err(_) => Ledger::new(),
    };
    community::merge(&mut ledger, account, votes);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let write = |name: &str, text: String| std::fs::write(dir.join(name), text).map_err(|e| e.to_string());
    write("ledger.json", serde_json::to_string_pretty(&ledger).map_err(|e| e.to_string())?)?;
    write("scores.json", serde_json::to_string(&community::tally(&ledger)).map_err(|e| e.to_string())?)?;
    Ok(count)
}
