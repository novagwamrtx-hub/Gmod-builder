//! Reads the Garry's Mod install on this computer: which Workshop addons the
//! game has downloaded, and what files they contain.
//!
//! Steam downloads each subscribed addon to
//! `steamapps/workshop/content/4000/<id>/<name>.gma`. Addons uploaded before
//! 2020 arrive packed as `<name>_legacy.bin` instead, and the game unpacks
//! those to `garrysmod/cache/workshop/<id>.gma` the first time it loads them.
//! Both places are read. A packed addon the game has not unpacked yet counts
//! as downloaded, but its files cannot be listed.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use gmcb_core::gma::{self, Archive};
use gmcb_core::GMOD_APPID;

const GAME_DIR: &str = "steamapps/common/GarrysMod/garrysmod";

/// The addons the game has downloaded.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Installed {
    /// Where the archive of each addon that can be read is.
    files: BTreeMap<u64, PathBuf>,
    /// Addons that are still packed.
    packed: BTreeSet<u64>,
}

impl Installed {
    pub fn len(&self) -> usize {
        self.files.len() + self.packed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every downloaded addon, oldest id first.
    pub fn ids(&self) -> Vec<u64> {
        let mut ids: Vec<u64> = self.files.keys().chain(&self.packed).copied().collect();
        ids.sort_unstable();
        ids
    }

    /// True if the addon's files can be listed.
    pub fn has_files(&self, id: u64) -> bool {
        self.files.contains_key(&id)
    }

    /// The file list of a downloaded addon; `None` if it is not here or
    /// cannot be read.
    pub fn archive(&self, id: u64) -> Option<Archive> {
        let file = File::open(self.files.get(&id)?).ok()?;
        gma::read_archive(BufReader::new(file)).ok()
    }
}

/// The `garrysmod` folder: `configured` if the user pointed at it, else the
/// first one found where Steam installs games.
pub fn find(configured: &str) -> Option<PathBuf> {
    let configured = configured.trim();
    if !configured.is_empty() {
        // Pointing at the game's folder instead of the `garrysmod` inside it
        // is an easy mistake to make.
        let path = PathBuf::from(configured);
        return [path.join("garrysmod"), path].into_iter().find(|dir| is_game_dir(dir));
    }
    steam_roots()
        .iter()
        .flat_map(|root| {
            let mut libraries = vec![root.clone()];
            libraries.extend(extra_libraries(root));
            libraries
        })
        // Joined a folder at a time, so that the path reads the way the
        // system writes its own.
        .map(|library| GAME_DIR.split('/').fold(library, |path, folder| path.join(folder)))
        .find(|game| game.is_dir())
}

fn is_game_dir(dir: &Path) -> bool {
    dir.join("gameinfo.txt").is_file() || dir.join("cache/workshop").is_dir()
}

/// Where Steam itself may be installed.
fn steam_roots() -> Vec<PathBuf> {
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let mut roots = registered_roots();
    for program_files in ["ProgramFiles(x86)", "ProgramFiles"] {
        roots.extend(env(program_files).map(|dir| dir.join("Steam")));
    }
    if let Some(home) = env("HOME") {
        for dir in [
            ".steam/steam",
            ".local/share/Steam",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
            "snap/steam/common/.local/share/Steam",
            "Library/Application Support/Steam",
        ] {
            roots.push(home.join(dir));
        }
    }
    roots
}

/// Where Windows says Steam is; it is often not in Program Files.
#[cfg(windows)]
fn registered_roots() -> Vec<PathBuf> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let read = |hive, key: &str, value: &str| -> Option<PathBuf> {
        let path: String = winreg::RegKey::predef(hive).open_subkey(key).ok()?.get_value(value).ok()?;
        Some(PathBuf::from(path.replace('/', "\\")))
    };
    // The installer's record first: it is spelled the way the folder is,
    // where Steam's own is all in small letters.
    [
        read(HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        read(HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath"),
        read(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(not(windows))]
fn registered_roots() -> Vec<PathBuf> {
    Vec::new()
}

/// Other drives Steam installs games to.
fn extra_libraries(steam: &Path) -> Vec<PathBuf> {
    ["steamapps/libraryfolders.vdf", "config/libraryfolders.vdf"]
        .iter()
        .filter_map(|file| std::fs::read_to_string(steam.join(file)).ok())
        .flat_map(|text| library_paths(&text))
        .collect()
}

/// The folders named in a `libraryfolders.vdf`. Steam has written them as
/// `"path" "D:\\Games"` since 2021, and as `"1" "D:\\Games"` before that.
fn library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let mut quoted = line.split('"').skip(1).step_by(2);
            let (key, value) = (quoted.next()?, quoted.next()?);
            // The newer form also lists each game's size under its number.
            let numbered = !key.is_empty() && key.bytes().all(|b| b.is_ascii_digit());
            let names_a_folder = key == "path" || (numbered && value.contains(['/', '\\']));
            names_a_folder.then(|| PathBuf::from(value.replace("\\\\", "\\")))
        })
        .collect()
}

/// What the game has downloaded, given its `garrysmod` folder.
pub fn installed(gmod: &Path) -> Installed {
    let mut found = Installed::default();
    for unpacked in entries(&gmod.join("cache/workshop")) {
        let name = unpacked.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if let Some(id) = name.strip_suffix(".gma").and_then(|id| id.parse().ok()) {
            found.files.insert(id, unpacked);
        }
    }
    // From `<library>/steamapps/common/GarrysMod/garrysmod` up to `steamapps`.
    let Some(steamapps) = gmod.ancestors().nth(3) else { return found };
    for folder in entries(&steamapps.join(format!("workshop/content/{GMOD_APPID}"))) {
        let Some(id) = folder.file_name().and_then(|name| name.to_str()).and_then(|id| id.parse().ok()) else {
            continue;
        };
        let downloads = entries(&folder);
        // Steam's own copy is the current one, whatever the game unpacked earlier.
        if let Some(archive) = downloads.iter().find(|file| file.extension().is_some_and(|ext| ext == "gma")) {
            found.files.insert(id, archive.clone());
        } else if !downloads.is_empty() && !found.files.contains_key(&id) {
            found.packed.insert(id);
        }
    }
    found
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map(|entries| entries.flatten().map(|entry| entry.path()).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::TestDir;

    /// The smallest archive the reader accepts, with one file in it.
    fn archive(title: &str) -> Vec<u8> {
        let mut bytes = b"GMAD\x03".to_vec();
        bytes.extend([0; 16]);
        bytes.push(0); // no required content
        for text in [title, "description", "author"] {
            bytes.extend(text.as_bytes());
            bytes.push(0);
        }
        bytes.extend(1u32.to_le_bytes()); // addon version
        bytes.extend(1u32.to_le_bytes()); // first file
        bytes.extend(b"lua/autorun/hello.lua\0");
        bytes.extend([0; 12]);
        bytes.extend(0u32.to_le_bytes());
        bytes
    }

    #[test]
    fn addons_are_found_where_steam_and_the_game_put_them() {
        let library = TestDir::new("gmod-installed");
        let game = "steamapps/common/GarrysMod/garrysmod";
        let content = "steamapps/workshop/content/4000";
        library.file(&format!("{game}/cache/workshop/100.gma"), &archive("unpacked"));
        library.file(&format!("{game}/cache/workshop/200.gma"), &archive("stale"));
        library.file(&format!("{game}/cache/workshop/100.cache"), b"a thumbnail");
        library.file(&format!("{content}/200/addon.gma"), &archive("current"));
        library.file(&format!("{content}/300/addon.gma"), &archive("new"));
        library.file(&format!("{content}/400/old_legacy.bin"), b"packed");
        library.file(&format!("{content}/100/old_legacy.bin"), b"packed, but the game unpacked it");
        library.file(&format!("{content}/not-an-id/addon.gma"), &archive("stray"));
        std::fs::create_dir_all(library.0.join(format!("{content}/500"))).unwrap();

        let found = installed(&library.0.join(game));
        assert_eq!(found.ids(), [100, 200, 300, 400]);
        assert_eq!(found.len(), 4);
        assert!(found.has_files(100) && !found.has_files(400) && !found.has_files(500));
        assert_eq!(found.archive(200).unwrap().title, "current");
        assert_eq!(found.archive(100).unwrap().files, ["lua/autorun/hello.lua"]);
        assert_eq!(found.archive(400), None);

        assert!(installed(&library.0.join("nowhere")).is_empty());
    }

    #[test]
    fn the_folder_the_user_points_at_may_be_the_game_or_the_one_inside_it() {
        let library = TestDir::new("gmod-find");
        library.file("GarrysMod/garrysmod/gameinfo.txt", b"");
        let inner = library.0.join("GarrysMod/garrysmod");
        assert_eq!(find(inner.to_str().unwrap()), Some(inner.clone()));
        assert_eq!(find(library.0.join("GarrysMod").to_str().unwrap()), Some(inner));
        assert_eq!(find(library.0.to_str().unwrap()), None, "some other folder is not the game");
    }

    #[test]
    fn library_folders_are_read_in_both_forms() {
        let new = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"label\"\t\t\"\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"4000\"\t\t\"4242424242\"\n\t\t}\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\Games\"\n\t}\n}";
        assert_eq!(
            library_paths(new),
            [PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"D:\Games")],
            "a game's size is not a folder"
        );

        let old = "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\t\"1600000000\"\n\t\"1\"\t\t\"E:\\\\SteamLibrary\"\n}";
        assert_eq!(library_paths(old), [PathBuf::from(r"E:\SteamLibrary")]);
    }
}
