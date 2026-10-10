//! Reads the Garry's Mod install on this computer: which Workshop addons the
//! game has downloaded, and what files they contain.
//!
//! The game keeps each downloaded addon as `garrysmod/cache/workshop/<id>.gma`.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use gmcb_core::gma::{self, Archive};

const GAME_DIR: &str = "steamapps/common/GarrysMod/garrysmod";

/// The `garrysmod` folder: `configured` if it is one, else the first found
/// in Steam's usual install locations and extra library folders.
pub fn find_gmod(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        return cache_dir(&path).is_dir().then_some(path);
    }
    steam_roots()
        .iter()
        .flat_map(|root| {
            let mut libraries = vec![root.clone()];
            libraries.extend(extra_libraries(root));
            libraries
        })
        .map(|library| library.join(GAME_DIR))
        .find(|game| game.is_dir())
}

fn steam_roots() -> Vec<PathBuf> {
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut roots = Vec::new();
    for program_files in ["ProgramFiles(x86)", "ProgramFiles"] {
        roots.extend(env(program_files).map(|dir| dir.join("Steam")));
    }
    if let Some(home) = env("HOME") {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join("Library/Application Support/Steam"));
    }
    roots
}

/// Other drives Steam installs games to, from `libraryfolders.vdf`.
fn extra_libraries(steam: &Path) -> Vec<PathBuf> {
    let Ok(text) = std::fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("\"path\""))
        .map(|value| PathBuf::from(value.trim().trim_matches('"').replace("\\\\", "\\")))
        .collect()
}

fn cache_dir(gmod: &Path) -> PathBuf {
    gmod.join("cache/workshop")
}

/// Ids of the addons the game has downloaded, oldest id first.
pub fn installed_ids(gmod: &Path) -> Vec<u64> {
    let Ok(entries) = std::fs::read_dir(cache_dir(gmod)) else {
        return Vec::new();
    };
    let mut ids: Vec<u64> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.strip_suffix(".gma")?.parse().ok())
        .collect();
    ids.sort_unstable();
    ids
}

/// The file list of a downloaded addon; `None` if the game does not have it.
pub fn read_archive(gmod: &Path, id: u64) -> Option<Archive> {
    let file = File::open(cache_dir(gmod).join(format!("{id}.gma"))).ok()?;
    gma::read_archive(BufReader::new(file)).ok()
}
