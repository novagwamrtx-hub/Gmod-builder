//! Reads the file list of a `.gma`, the archive format GMod addons ship in.
//!
//! The list sits at the front of the archive, so nothing past it is read.
//! With file lists for a whole collection the app can tell which addons
//! overwrite each other and which ones clients need to download at all.

use std::io::{self, BufRead, Read};

const MAGIC: &[u8; 4] = b"GMAD";
/// Guards against reading a corrupt archive forever.
const MAX_FILES: usize = 200_000;
const MAX_STRING: usize = 4096;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Archive {
    pub title: String,
    /// Paths inside the addon, lower case with forward slashes.
    pub files: Vec<String>,
}

impl Archive {
    /// True if clients need nothing from this addon: scripts reach them
    /// through the server, so only models, materials, sounds and maps count.
    pub fn is_lua_only(&self) -> bool {
        !self.files.is_empty() && self.files.iter().all(|file| is_script(file))
    }

    /// Names of the maps in the addon, without folder or extension.
    pub fn maps(&self) -> Vec<&str> {
        self.files
            .iter()
            .filter_map(|file| file.strip_prefix("maps/")?.strip_suffix(".bsp"))
            .filter(|name| !name.contains('/'))
            .collect()
    }
}

pub fn is_script(path: &str) -> bool {
    matches!(path.rsplit('.').next(), Some("lua" | "txt" | "json" | "md" | "properties"))
}

pub fn read_archive(mut reader: impl BufRead) -> io::Result<Archive> {
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "not a GMA archive"));
    }
    let version = read_array::<1>(&mut reader)?[0];
    read_array::<16>(&mut reader)?; // author's SteamID and a timestamp
    if version > 1 {
        // Required content: strings up to an empty one.
        while !read_string(&mut reader)?.is_empty() {}
    }
    let title = read_string(&mut reader)?;
    read_string(&mut reader)?; // description
    read_string(&mut reader)?; // author
    read_array::<4>(&mut reader)?; // addon version

    let mut files = Vec::new();
    // Entries are numbered from 1; a zero ends the list.
    while u32::from_le_bytes(read_array(&mut reader)?) != 0 {
        if files.len() >= MAX_FILES {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "too many files in archive"));
        }
        files.push(read_string(&mut reader)?.to_lowercase().replace('\\', "/"));
        read_array::<12>(&mut reader)?; // size and checksum
    }
    Ok(Archive { title, files })
}

fn read_array<const N: usize>(reader: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_string(reader: &mut impl BufRead) -> io::Result<String> {
    let mut bytes = Vec::new();
    reader.by_ref().take(MAX_STRING as u64).read_until(0, &mut bytes)?;
    if bytes.pop() != Some(0) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "unterminated string in archive"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
pub(crate) fn sample(title: &str, files: &[&str]) -> Vec<u8> {
    let mut out = b"GMAD\x03".to_vec();
    out.extend([0u8; 16]);
    out.push(0); // no required content
    for text in [title, "{}", "author"] {
        out.extend(text.as_bytes());
        out.push(0);
    }
    out.extend(1i32.to_le_bytes());
    for (i, file) in files.iter().enumerate() {
        out.extend((i as u32 + 1).to_le_bytes());
        out.extend(file.as_bytes());
        out.push(0);
        out.extend([0u8; 12]);
    }
    out.extend(0u32.to_le_bytes());
    out.extend(b"file contents follow");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_title_and_files() {
        let bytes = sample("Zombies", &["lua/autorun/Z.lua", "models\\z.mdl", "maps/gm_dead.bsp"]);
        let archive = read_archive(bytes.as_slice()).unwrap();
        assert_eq!(archive.title, "Zombies");
        assert_eq!(archive.files, ["lua/autorun/z.lua", "models/z.mdl", "maps/gm_dead.bsp"]);
        assert_eq!(archive.maps(), ["gm_dead"]);
        assert!(!archive.is_lua_only());
    }

    #[test]
    fn script_only_addons() {
        let bytes = sample("Admin", &["lua/autorun/a.lua", "addon.json"]);
        assert!(read_archive(bytes.as_slice()).unwrap().is_lua_only());
        // An empty list is "unknown", not "nothing to download".
        assert!(!Archive::default().is_lua_only());
    }

    #[test]
    fn rejects_other_files() {
        assert!(read_archive(&b"\xff\xd8\xff\xe0 a jpeg"[..]).is_err());
        assert!(read_archive(&b"GMAD\x03 cut short"[..]).is_err());
    }
}
