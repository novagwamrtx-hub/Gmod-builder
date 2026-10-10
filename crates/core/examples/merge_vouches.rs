//! Folds shared vouches into the community ledger and rewrites the published
//! totals. Run by `.github/workflows/vouches.yml`:
//!
//!     cargo run -p gmcb-core --no-default-features --example merge_vouches -- <issues file> <community dir>
//!
//! The issues file is GitHub's list of the repository's open issues, as
//! `gh api --paginate repos/OWNER/NAME/issues` prints it. Every issue titled
//! "Vouches" is counted; for each, a line with its number and how many
//! verdicts it carried is printed, so the workflow can answer and close it.

use std::path::Path;
use std::process::ExitCode;

use gmcb_core::community::{self, Ledger};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [issues, dir] = args.as_slice() else {
        eprintln!("usage: merge_vouches <issues file> <community dir>");
        return ExitCode::FAILURE;
    };
    match run(Path::new(issues), Path::new(dir)) {
        Ok(counted) => {
            for (number, verdicts) in counted {
                println!("{number}\t{verdicts}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run(issues: &Path, dir: &Path) -> Result<Vec<(u64, usize)>, String> {
    let read = |path: &Path| std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()));
    let issues = community::submissions(&read(issues)?).map_err(|e| format!("issues: {e}"))?;
    let ledger_path = dir.join("ledger.json");
    let mut ledger: Ledger = match ledger_path.exists() {
        true => serde_json::from_str(&read(&ledger_path)?).map_err(|e| format!("ledger.json: {e}"))?,
        false => Ledger::new(),
    };
    let counted = community::count(&mut ledger, &issues);

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let write = |name: &str, text: String| std::fs::write(dir.join(name), text + "\n").map_err(|e| e.to_string());
    write("ledger.json", serde_json::to_string_pretty(&ledger).map_err(|e| e.to_string())?)?;
    write("scores.json", serde_json::to_string(&community::tally(&ledger)).map_err(|e| e.to_string())?)?;
    Ok(counted)
}
