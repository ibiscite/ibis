//! Readers of files a person or Zotero wrote that are not libraries: a `.bib`
//! edited in place (decision 0055), a project's bibliography read only to
//! check keys, and Zotero's profile registry and preferences, read to find
//! its data directory.

use std::time::Instant;

use ibis_sources::bibtex_edit::BibFile;
use ibis_sources::project::ProjectRead;
use ibis_sources::zotero_local::discovery::{DiscoveryRoots, Platform, discover};

use crate::{MAX_INPUT_BYTES, assert_in_time, assert_no_contained_panic, watch_panics};

/// An edit keeps every byte it does not change: an entry edited without
/// changes leaves the whole file as it was, and an appended entry follows it.
pub fn check_bibtex_edit(data: &[u8]) {
    let Ok(text) = std::str::from_utf8(data) else { return };
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    if let Ok(file) = BibFile::read(text) {
        let appended = file.append("@misc{invented2026,\n  title = {Invented}\n}\n");
        assert!(appended.starts_with(text), "appending changed the existing text");
        // Keys as written after `@type{`, found without the reader's help.
        for key in text
            .split('@')
            .skip(1)
            .filter_map(|rest| rest.split_once('{')?.1.split_once(','))
            .map(|(key, _)| key.trim())
        {
            if let Ok(entry) = file.entry(key) {
                assert_eq!(entry.edited(&[]), text, "an edit without changes changed the file");
                let _ = entry.edited(&[("title", Some("{Invented}".into())), ("note", None)]);
            }
        }
    }
    assert_no_contained_panic();
    assert_in_time(started);
}

/// A project's bibliography, read as the picker reads it.
pub fn check_project_bib(data: &[u8]) {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    let directory = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary directory creation failed"));
    let path = directory.path().join("invented.bib");
    std::fs::write(&path, data).unwrap_or_else(|_| panic!("temporary input write failed"));
    let read = ProjectRead::read(&path, None);
    let again = ProjectRead::read(&path, Some(&read));
    assert!(read.same_answers(&again), "an unchanged file gave different answers");
    if read.bibliography().is_some_and(|bibliography| bibliography.complete()) {
        assert!(read.damaged_lines().is_empty(), "a complete read with damaged lines");
    }
    assert_no_contained_panic();
    assert_in_time(started);
}

/// `profiles.ini`, then a zero byte and the profile's `prefs.js`.
pub fn check_zotero_profiles(data: &[u8]) {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    let (registry, preferences) = match data.iter().position(|byte| *byte == 0) {
        Some(split) => (&data[..split], &data[split + 1..]),
        None => (data, &b""[..]),
    };
    let home = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary directory creation failed"));
    #[cfg(windows)]
    let (platform, profiles) = (Platform::Windows, home.path().join("AppData/Roaming/Zotero/Zotero"));
    #[cfg(not(windows))]
    let (platform, profiles) = (Platform::Linux, home.path().join(".zotero/zotero"));
    std::fs::create_dir_all(profiles.join("Profiles/invented")).unwrap_or_else(|_| panic!("profile folder failed"));
    std::fs::write(profiles.join("profiles.ini"), registry).unwrap_or_else(|_| panic!("registry write failed"));
    std::fs::write(profiles.join("Profiles/invented/prefs.js"), preferences)
        .unwrap_or_else(|_| panic!("preferences write failed"));
    let found = discover(&DiscoveryRoots::new(platform, home.path().to_path_buf(), None));
    assert_no_contained_panic();
    let mut names: Vec<_> = found.directories.iter().map(|directory| directory.name()).collect();
    assert!(
        found.directories.iter().all(|directory| directory.path().is_absolute()),
        "a relative data directory"
    );
    names.sort_unstable();
    let count = names.len();
    names.dedup();
    assert_eq!(names.len(), count, "two directories with one name");
    assert_in_time(started);
}

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;

    /// The profile seed reaches preferences and finds its data directory.
    #[test]
    fn the_profile_seed_finds_a_directory() {
        let seed =
            std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/zotero_profiles/profile.txt"))
                .unwrap_or_else(|_| panic!("no seed"));
        let split = seed
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or_else(|| panic!("no preferences"));
        let home = tempfile::tempdir().unwrap_or_else(|_| panic!("temporary directory creation failed"));
        let profiles = home.path().join(".zotero/zotero");
        std::fs::create_dir_all(profiles.join("Profiles/invented")).unwrap_or_else(|_| panic!("profile folder failed"));
        std::fs::write(profiles.join("profiles.ini"), &seed[..split]).unwrap_or_else(|_| panic!("write failed"));
        std::fs::write(profiles.join("Profiles/invented/prefs.js"), &seed[split + 1..])
            .unwrap_or_else(|_| panic!("write failed"));
        let found = discover(&DiscoveryRoots::new(Platform::Linux, home.path().to_path_buf(), None));
        assert_eq!(found.directories.len(), 1, "{:?}", found.warnings);
    }
}
