//! Channel-isolated settings migration for the permanent app identity (ADR 0023).

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COPY: AtomicU64 = AtomicU64::new(0);
pub(crate) const MIGRATION_NOTICE: &str =
    "Settings could not be moved; Ibis is using the previous settings. It will try again next time.";

pub(crate) struct SettingsLocation {
    pub(crate) path: PathBuf,
    pub(crate) using_legacy: bool,
}

pub(crate) fn settings_location(directory: &Path, identifier: &str) -> SettingsLocation {
    let current = SettingsLocation {
        path: directory.join("settings.json"),
        using_legacy: false,
    };
    let legacy = match identifier {
        "com.ibiscite.ibis" => "io.github.ibiscite.ibis",
        "com.ibiscite.ibis.nightly" => "io.github.ibiscite.ibis.nightly",
        _ => return current,
    };
    let Some(parent) = directory.parent() else {
        return current;
    };
    if directory.file_name() != Some(std::ffi::OsStr::new(identifier)) {
        return current;
    }
    let old = parent.join(legacy);
    if !present(&old.join("settings.json")) || present(&current.path) || present(&directory.join("settings.json.bak")) {
        return current;
    }
    if copy_settings(&old, directory).is_ok() || present(&current.path) {
        return current;
    }
    SettingsLocation {
        path: old.join("settings.json"),
        using_legacy: true,
    }
}

/// An unreadable entry is never treated as an absent file to replace.
fn present(path: &Path) -> bool {
    !matches!(fs::symlink_metadata(path), Err(error) if matches!(error.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory))
}

fn copy_settings(old: &Path, directory: &Path) -> io::Result<()> {
    if directory.exists() && fs::read_dir(directory)?.next().is_some() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    let staging = directory.with_extension(format!(
        "migration-{}-{}",
        std::process::id(),
        COPY.fetch_add(1, Ordering::Relaxed)
    ));
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(&staging)?;
    }
    #[cfg(not(unix))]
    fs::create_dir(&staging)?;
    let result = (|| {
        for name in ["settings.json", "settings.json.bak"] {
            let source = old.join(name);
            let metadata = match fs::symlink_metadata(&source) {
                Err(error) if error.kind() == io::ErrorKind::NotFound && name.ends_with(".bak") => continue,
                other => other?,
            };
            // Do not traverse links or open a special file during the copy.
            if !metadata.is_file() {
                return Err(io::ErrorKind::InvalidData.into());
            }
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(staging.join(name))?;
            io::copy(&mut File::open(source)?, &mut output)?;
            output.set_permissions(metadata.permissions())?;
            output.sync_all()?;
        }
        #[cfg(unix)]
        let _ = File::open(&staging).and_then(|folder| folder.sync_all());
        if directory.exists() {
            fs::remove_dir(directory)?;
        }
        // A concurrent nonempty destination cannot be replaced by this rename.
        fs::rename(&staging, directory)?;
        #[cfg(unix)]
        let _ = directory
            .parent()
            .and_then(|parent| File::open(parent).ok())
            .map(|folder| folder.sync_all());
        Ok(())
    })();
    // This invocation alone created the staging directory.
    let _ = fs::remove_dir_all(&staging);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("missing test parent")))?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    #[test]
    fn each_channel_copies_its_primary_and_backup_without_rewriting_or_removing_them() -> io::Result<()> {
        for suffix in ["", ".nightly"] {
            let root = tempfile::tempdir()?;
            let old = root.path().join(format!("io.github.ibiscite.ibis{suffix}"));
            let new = root.path().join(format!("com.ibiscite.ibis{suffix}"));
            let primary = br#"{ "schema_version": 99, "future": ["opaque"], "shortcut": "Control+Alt+Y" }"#;
            let backup = br#"{"library":{"zotero":["user",{"group":7}]},"copy_format":"typst"}"#;
            write(&old.join("settings.json"), primary)?;
            write(&old.join("settings.json.bak"), backup)?;
            let location = settings_location(&new, &format!("com.ibiscite.ibis{suffix}"));
            assert_eq!(location.path, new.join("settings.json"));
            assert!(!location.using_legacy);
            for (name, bytes) in [
                ("settings.json", primary.as_slice()),
                ("settings.json.bak", backup.as_slice()),
            ] {
                assert_eq!(std::fs::read(new.join(name))?, bytes);
                assert_eq!(std::fs::read(old.join(name))?, bytes);
            }
            write(&old.join("settings.json"), b"later old edit")?;
            let again = settings_location(&new, &format!("com.ibiscite.ibis{suffix}"));
            assert_eq!(again.path, new.join("settings.json"));
            assert_eq!(std::fs::read(&again.path)?, primary);
        }
        Ok(())
    }

    #[test]
    fn existing_primary_or_backup_wins_even_when_empty_invalid_or_newer() -> io::Result<()> {
        for name in ["settings.json", "settings.json.bak"] {
            for contents in [b"".as_slice(), b"invalid", br#"{"schema_version":99}"#] {
                let root = tempfile::tempdir()?;
                let new = root.path().join("com.ibiscite.ibis");
                write(&root.path().join("io.github.ibiscite.ibis/settings.json"), b"legacy")?;
                write(&new.join(name), contents)?;
                let location = settings_location(&new, "com.ibiscite.ibis");
                assert_eq!(location.path, new.join("settings.json"));
                assert!(!location.using_legacy);
                assert_eq!(std::fs::read(new.join(name))?, contents);
                if name.ends_with(".bak") {
                    assert!(!location.path.exists());
                }
            }
        }
        Ok(())
    }

    #[test]
    fn another_channel_or_unknown_identifier_is_never_a_migration_source() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        write(&root.path().join("io.github.ibiscite.ibis/settings.json"), b"stable")?;
        for identifier in [
            "com.ibiscite.ibis.nightly",
            "com.ibiscite.ibis.development",
            "io.github.ibiscite.ibis",
        ] {
            let new = root.path().join(identifier);
            let location = settings_location(&new, identifier);
            assert_eq!(location.path, new.join("settings.json"));
            assert!(!location.using_legacy);
            if identifier.starts_with("com.") {
                assert!(!location.path.exists());
            }
        }
        Ok(())
    }

    #[test]
    fn an_empty_destination_is_allowed_but_unrelated_files_are_not_replaced() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let old = root.path().join("io.github.ibiscite.ibis/settings.json");
        let new = root.path().join("com.ibiscite.ibis");
        write(&old, b"legacy")?;
        std::fs::create_dir(&new)?;
        assert_eq!(
            settings_location(&new, "com.ibiscite.ibis").path,
            new.join("settings.json")
        );
        assert_eq!(std::fs::read(new.join("settings.json"))?, b"legacy");
        std::fs::remove_file(new.join("settings.json"))?;
        write(&new.join("unrelated"), b"keep")?;
        let fallback = settings_location(&new, "com.ibiscite.ibis");
        assert_eq!(fallback.path, old);
        assert!(fallback.using_legacy);
        assert_eq!(std::fs::read(new.join("unrelated"))?, b"keep");
        assert!(!new.join("settings.json").exists());
        Ok(())
    }

    #[test]
    fn a_failed_copy_keeps_using_legacy_and_never_exposes_partial_new_settings() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let old = root.path().join("io.github.ibiscite.ibis/settings.json");
        let new = root.path().join("com.ibiscite.ibis");
        write(&old, b"legacy")?;
        // A malformed backup deterministically refuses the whole copy.
        std::fs::create_dir(old.with_extension("json.bak"))?;
        let fallback = settings_location(&new, "com.ibiscite.ibis");
        assert_eq!(fallback.path, old);
        assert!(fallback.using_legacy);
        assert!(!new.join("settings.json").exists());
        assert_eq!(std::fs::read(&fallback.path)?, b"legacy");
        assert_eq!(std::fs::read_dir(root.path())?.count(), 1);
        Ok(())
    }

    #[test]
    fn a_destination_blocked_by_a_file_keeps_using_the_old_settings() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let old = root.path().join("io.github.ibiscite.ibis/settings.json");
        let new = root.path().join("com.ibiscite.ibis");
        write(&old, b"legacy")?;
        write(&new, b"unrelated file")?;
        let location = settings_location(&new, "com.ibiscite.ibis");
        assert_eq!(location.path, old);
        assert!(location.using_legacy);
        assert_eq!(std::fs::read(new)?, b"unrelated file");
        Ok(())
    }

    #[test]
    fn concurrent_copies_publish_one_complete_large_settings_pair() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let old = root.path().join("io.github.ibiscite.ibis/settings.json");
        let new = root.path().join("com.ibiscite.ibis");
        let bytes = vec![b'x'; 256 * 1024];
        write(&old, &bytes)?;
        write(&old.with_extension("json.bak"), b"backup")?;
        std::thread::scope(|scope| {
            let copies: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| settings_location(&new, "com.ibiscite.ibis")))
                .collect();
            for copy in copies {
                assert_eq!(
                    copy.join().unwrap_or_else(|_| panic!("migration worker failed")).path,
                    new.join("settings.json")
                );
            }
        });
        assert_eq!(std::fs::read(new.join("settings.json"))?, bytes);
        assert_eq!(std::fs::read(new.join("settings.json.bak"))?, b"backup");
        assert_eq!(std::fs::read_dir(root.path())?.count(), 2);
        Ok(())
    }
}
