//! Writing the app's own files so that a crash, a power cut or a full disk
//! cannot leave half of one behind.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Writes `text` to a new file beside `path`, then moves it into place. The
/// file that was there stays whole until the new one is complete.
pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut unfinished = path.as_os_str().to_owned();
    unfinished.push(".tmp");
    let unfinished = PathBuf::from(unfinished);
    let written = File::create(&unfinished).and_then(|mut file| {
        file.write_all(text.as_bytes())?;
        file.sync_all()
    });
    let moved = written.and_then(|()| std::fs::rename(&unfinished, path));
    if moved.is_err() {
        let _ = std::fs::remove_file(&unfinished);
    }
    moved
}

/// A number that stands for `bytes` in the name of a cache file (FNV-1a).
/// The same bytes give the same number on every computer and in every
/// version of the app, which the standard library's hashes do not promise.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3))
}

/// Tidies a folder of cached files: deletes those written longer ago than
/// `max_age`, and then the oldest until the rest fits in `max_bytes`.
pub fn prune(dir: &Path, max_age: Duration, max_bytes: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut found: Vec<(SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let about = entry.metadata().ok().filter(|about| about.is_file())?;
            Some((about.modified().ok()?, about.len(), entry.path()))
        })
        .collect();
    found.sort();
    let now = SystemTime::now();
    let mut total: u64 = found.iter().map(|(_, size, _)| size).sum();
    for (written, size, path) in found {
        let stale = now.duration_since(written).is_ok_and(|age| age > max_age);
        if (stale || total > max_bytes) && std::fs::remove_file(path).is_ok() {
            total -= size;
        }
    }
}

/// A folder under the system's temporary files that is removed again when
/// the test is over.
#[cfg(test)]
pub struct TestDir(pub PathBuf);

#[cfg(test)]
impl TestDir {
    pub fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("gmcb-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    /// Creates a file, and the folders leading to it.
    pub fn file(&self, path: &str, contents: &[u8]) -> PathBuf {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, contents).unwrap();
        path
    }
}

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writing_replaces_the_file_and_leaves_nothing_else() {
        let dir = TestDir::new("files");
        let path = dir.0.join("deeper/settings.json");
        write(&path, "first").unwrap();
        write(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().flatten().collect();
        assert_eq!(names.len(), 1, "no temporary file is left behind");
    }

    #[test]
    fn names_for_cache_files_do_not_change() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_ne!(fnv1a("зомби".as_bytes()), fnv1a("войны".as_bytes()));
    }

    #[test]
    fn pruning_deletes_what_is_old_and_what_does_not_fit() {
        let dir = TestDir::new("files-prune");
        let day = Duration::from_secs(24 * 60 * 60);
        let aged = |name: &str, days: u32, size: usize| {
            let path = dir.file(name, &vec![0; size]);
            let file = File::options().write(true).open(path).unwrap();
            file.set_modified(SystemTime::now() - day * days).unwrap();
        };
        aged("ancient", 40, 10);
        aged("older", 3, 600);
        aged("newer", 2, 600);
        aged("newest", 1, 600);
        std::fs::create_dir_all(dir.0.join("a-folder")).unwrap();

        prune(&dir.0, day * 30, 1300);
        let mut left: Vec<String> =
            std::fs::read_dir(&dir.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["a-folder", "newer", "newest"]);
        prune(&dir.0.join("not-there"), day, 0);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file() {
        let dir = TestDir::new("files-failed");
        let path = dir.file("settings.json", b"old");
        // A folder where the temporary file would go makes the write fail.
        std::fs::create_dir_all(dir.0.join("settings.json.tmp")).unwrap();
        assert!(write(&path, "new").is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "old");
    }
}
