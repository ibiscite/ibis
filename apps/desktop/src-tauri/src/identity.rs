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
    let app = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(io::ErrorKind::InvalidInput)?;
    // Keep this inode: unlinking it after unlocking would let launches lock different files.
    let lock_path = directory.with_file_name(format!("{app}.migration.lock"));
    if present(&lock_path) && !fs::symlink_metadata(&lock_path)?.is_file() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(lock_path)?;
    lock.lock()?;
    // The lock covers cleanup, staging creation and publication across processes.
    clean_staging(directory, &format!("{app}.migration-"))?;
    if directory.exists() && fs::read_dir(directory)?.next().is_some() {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    let staging = directory.with_file_name(format!(
        "{app}.migration-{}-{}",
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
            let metadata = match fs::metadata(&source) {
                Err(error)
                    if error.kind() == io::ErrorKind::NotFound && name.ends_with(".bak") && !present(&source) =>
                {
                    continue;
                }
                other => other?,
            };
            // Follow ordinary file symlinks, matching settings::require_regular (ADR 0023).
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

/// Only real directories with our exact app name and canonical pid/counter are owned.
/// Truncated names from the dormant old implementation are ambiguous; leave them alone.
fn clean_staging(directory: &Path, prefix: &str) -> io::Result<()> {
    let parent = directory.parent().ok_or(io::ErrorKind::InvalidInput)?;
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some((pid, count)) = name
            .to_str()
            .and_then(|name| name.strip_prefix(prefix))
            .and_then(|tail| tail.split_once('-'))
        else {
            continue;
        };
        if pid.parse::<u32>().is_ok_and(|value| value.to_string() == pid)
            && count.parse::<u64>().is_ok_and(|value| value.to_string() == count)
            && entry.file_type()?.is_dir()
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
        std::fs::create_dir_all(path.parent().unwrap_or_else(|| panic!("missing test parent")))?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn ordinary_file_symlinks_migrate_but_broken_directory_and_loop_links_refuse() -> io::Result<()> {
        use std::os::unix::fs::symlink;
        for name in ["settings.json", "settings.json.bak"] {
            for kind in ["file", "dangling", "directory", "loop"] {
                let root = tempfile::tempdir()?;
                let old = root.path().join("io.github.ibiscite.ibis");
                let new = root.path().join("com.ibiscite.ibis");
                write(&old.join("settings.json"), b"primary")?;
                let source = old.join(name);
                if source.exists() {
                    fs::remove_file(&source)?;
                }
                let target = root.path().join("target");
                match kind {
                    "file" => write(&target, b"linked bytes")?,
                    "directory" => fs::create_dir(&target)?,
                    "loop" => symlink(&source, &target)?,
                    _ => {}
                }
                symlink(&target, &source)?;
                let location = settings_location(&new, "com.ibiscite.ibis");
                assert_eq!(location.using_legacy, kind != "file", "{name}: {kind}");
                if kind == "file" {
                    assert_eq!(fs::read(new.join(name))?, b"linked bytes");
                    assert!(fs::symlink_metadata(&source)?.is_symlink());
                } else {
                    assert_eq!(location.path, old.join("settings.json"));
                    assert!(!new.exists());
                }
            }
        }
        Ok(())
    }

    #[test]
    fn interrupted_staging_is_removed_without_touching_lookalikes() -> io::Result<()> {
        for suffix in ["", ".nightly"] {
            let root = tempfile::tempdir()?;
            let identifier = format!("com.ibiscite.ibis{suffix}");
            let new = root.path().join(&identifier);
            let old = root
                .path()
                .join(format!("io.github.ibiscite.ibis{suffix}/settings.json"));
            write(&old, b"complete")?;
            let stale = root.path().join(format!("{identifier}.migration-42-7"));
            write(&stale.join("settings.json"), b"partial")?;
            let lookalikes = [
                format!("{identifier}.migration-42"),
                format!("{identifier}.migration-42-7-extra"),
                format!("{identifier}.migration-abc-7"),
                format!("{identifier}.migration--7"),
                format!("{identifier}.migration-42-"),
                format!("{identifier}.migration-042-7"),
                format!("{identifier}.migration-4294967296-7"),
                format!("{identifier}.other.migration-42-7"),
                "com.ibiscite.migration-42-7".into(),
            ];
            for name in &lookalikes {
                write(&root.path().join(name).join("keep"), b"unrelated")?;
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(root.path().join(&lookalikes[0]), stale.join("outside"))?;
            // A matching file or symlink is not an owned staging directory.
            let file = root.path().join(format!("{identifier}.migration-42-8"));
            write(&file, b"keep file")?;
            #[cfg(unix)]
            let link = root.path().join(format!("{identifier}.migration-42-9"));
            #[cfg(unix)]
            std::os::unix::fs::symlink(&stale, &link)?;
            assert!(!settings_location(&new, &identifier).using_legacy);
            assert!(!stale.exists());
            assert_eq!(fs::read(new.join("settings.json"))?, b"complete");
            assert_eq!(fs::read(file)?, b"keep file");
            for name in &lookalikes {
                assert_eq!(fs::read(root.path().join(name).join("keep"))?, b"unrelated");
            }
            #[cfg(unix)]
            assert!(fs::symlink_metadata(link)?.is_symlink());
        }
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_sources_fail_without_publication() -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir()?;
        let parent = root.path();
        let old = parent.join("io.github.ibiscite.ibis/settings.json");
        let new = parent.join("com.ibiscite.ibis");
        write(&old, b"private")?;
        fs::set_permissions(&old, fs::Permissions::from_mode(0o000))?;
        let location = settings_location(&new, "com.ibiscite.ibis");
        fs::set_permissions(&old, fs::Permissions::from_mode(0o600))?;
        assert!(location.using_legacy);
        assert!(!new.exists());
        assert!(!settings_location(&new, "com.ibiscite.ibis").using_legacy);
        assert_eq!(fs::read(new.join("settings.json"))?, b"private");
        Ok(())
    }

    // APFS rejects invalid UTF-8 names itself; Linux permits this path case.
    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_parent_paths_migrate_without_conversion() -> io::Result<()> {
        use std::os::unix::ffi::OsStringExt;
        let root = tempfile::tempdir()?;
        let parent = root.path().join(std::ffi::OsString::from_vec(b"config-\xff".to_vec()));
        let new = parent.join("com.ibiscite.ibis");
        write(&parent.join("io.github.ibiscite.ibis/settings.json"), b"legacy")?;
        assert!(!settings_location(&new, "com.ibiscite.ibis").using_legacy);
        assert_eq!(fs::read(new.join("settings.json"))?, b"legacy");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_lock_is_preserved_and_refuses_cleanup() -> io::Result<()> {
        let root = tempfile::tempdir()?;
        let new = root.path().join("com.ibiscite.ibis");
        write(&root.path().join("io.github.ibiscite.ibis/settings.json"), b"legacy")?;
        let target = root.path().join("unrelated");
        write(&target, b"keep")?;
        let lock = root.path().join("com.ibiscite.ibis.migration.lock");
        std::os::unix::fs::symlink(&target, &lock)?;
        let stale = root.path().join("com.ibiscite.ibis.migration-42-7/settings.json");
        write(&stale, b"partial")?;
        assert!(settings_location(&new, "com.ibiscite.ibis").using_legacy);
        assert_eq!(fs::read(target)?, b"keep");
        assert_eq!(fs::read(stale)?, b"partial");
        assert!(fs::symlink_metadata(lock)?.is_symlink());
        assert!(!new.exists());
        Ok(())
    }

    // Own every child immediately; cleanup also runs on `?` and assertion unwinds.
    struct MigrationChildren {
        children: Vec<std::process::Child>,
        deadline: std::time::Instant,
    }

    impl MigrationChildren {
        fn new(budget: std::time::Duration) -> Self {
            Self {
                children: Vec::new(),
                deadline: std::time::Instant::now() + budget,
            }
        }

        fn spawn(&mut self, root: &Path, mode: &str, executable: &Path) -> io::Result<()> {
            use std::process::{Command, Stdio};
            if let Err(error) = self.poll() {
                self.cleanup();
                return Err(error);
            }
            let result = Command::new(executable)
                .args(["--exact", "identity::tests::migration_child"])
                .env("IBIS_TEST_MIGRATION_ROOT", root)
                .env(
                    "IBIS_TEST_MIGRATION_READY",
                    root.join(format!("ready-{}", self.children.len())),
                )
                .env("IBIS_TEST_MIGRATION_MODE", mode)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            match result {
                Ok(child) => {
                    self.children.push(child);
                    Ok(())
                }
                Err(_) => {
                    self.cleanup();
                    Err(io::Error::other("migration child spawn failed"))
                }
            }
        }

        fn poll(&mut self) -> io::Result<bool> {
            if std::time::Instant::now() >= self.deadline {
                return Err(io::ErrorKind::TimedOut.into());
            }
            let mut complete = true;
            for child in &mut self.children {
                match child.try_wait()? {
                    Some(status) if !status.success() => return Err(io::Error::other("migration child failed")),
                    Some(_) => {}
                    None => complete = false,
                }
            }
            Ok(complete)
        }

        fn ready(&mut self, root: &Path) -> io::Result<()> {
            let result = (|| loop {
                let complete = self.poll()?;
                if (0..self.children.len()).all(|index| root.join(format!("ready-{index}")).is_file()) {
                    return Ok(());
                }
                if complete {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            })();
            if result.is_err() {
                self.cleanup();
            }
            result
        }

        fn release(&mut self, index: usize) -> io::Result<()> {
            use std::io::Write;
            let result = (|| {
                self.poll()?;
                self.children[index]
                    .stdin
                    .take()
                    .ok_or(io::ErrorKind::BrokenPipe)?
                    .write_all(b"!")
            })();
            if result.is_err() {
                self.cleanup();
            }
            result
        }

        fn finish(&mut self) -> io::Result<()> {
            let result = (|| {
                while !self.poll()? {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Ok(())
            })();
            if result.is_err() {
                self.cleanup();
            }
            result
        }

        fn cleanup(&mut self) {
            // Kill all before reaping any: no sibling is left running behind a wait.
            for child in &mut self.children {
                let _ = child.kill();
            }
            for child in &mut self.children {
                let _ = child.wait();
            }
        }
    }

    impl Drop for MigrationChildren {
        fn drop(&mut self) {
            self.cleanup();
        }
    }

    #[test]
    fn migration_child() -> io::Result<()> {
        if let Some(root) = std::env::var_os("IBIS_TEST_MIGRATION_ROOT") {
            use std::io::Read;
            let root = Path::new(&root);
            let mode = std::env::var("IBIS_TEST_MIGRATION_MODE").map_err(|_| io::ErrorKind::InvalidInput)?;
            if mode == "fail-before-ready" {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let lock = if mode == "migrate" {
                let lock = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(root.join("com.ibiscite.ibis.migration.lock"))?;
                // Readiness proves real contention, even on a single-core runner.
                assert!(matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)));
                Some(lock)
            } else {
                None
            };
            let ready = std::env::var_os("IBIS_TEST_MIGRATION_READY").ok_or(io::ErrorKind::InvalidInput)?;
            fs::write(ready, b"")?;
            if let Some(lock) = lock {
                // The parent's held lock is the release barrier; no timing-based start.
                lock.lock()?;
                drop(lock);
            } else {
                std::io::stdin().read_exact(&mut [0])?;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let new = root.join("com.ibiscite.ibis");
            assert!(!settings_location(&new, "com.ibiscite.ibis").using_legacy);
        }
        Ok(())
    }

    #[test]
    fn migration_children_reap_blocked_siblings_on_timeout_failure_and_spawn_error() -> io::Result<()> {
        use std::time::{Duration, Instant};
        let executable = std::env::current_exe()?;
        for mode in ["timeout", "failure", "spawn", "readiness"] {
            let root = tempfile::tempdir()?;
            let mut group = MigrationChildren::new(Duration::from_secs(20));
            group.spawn(root.path(), "blocked", &executable)?;
            group.ready(root.path())?;
            group.spawn(
                root.path(),
                if mode == "readiness" {
                    "fail-before-ready"
                } else {
                    "fail"
                },
                &executable,
            )?;
            if mode != "readiness" {
                group.ready(root.path())?;
            }
            let started = Instant::now();
            let result = match mode {
                "timeout" => {
                    // Both children are confirmed blocked; no scheduler-dependent startup budget.
                    group.deadline = started + Duration::from_millis(100);
                    group.finish()
                }
                "failure" => {
                    group.release(1)?;
                    group.finish()
                }
                "spawn" => group.spawn(root.path(), "blocked", &root.path().join("absent-child")),
                _ => group.ready(root.path()),
            };
            let error = result.err().unwrap_or_else(|| panic!("child failure was missed"));
            assert_eq!(
                error.kind(),
                if mode == "timeout" {
                    io::ErrorKind::TimedOut
                } else {
                    io::ErrorKind::Other
                }
            );
            assert!(
                started.elapsed() < Duration::from_secs(5),
                "child cleanup exceeded budget"
            );
            for child in &mut group.children {
                assert!(child.try_wait()?.is_some(), "owned child was not reaped");
            }
        }
        Ok(())
    }

    #[test]
    fn concurrent_launches_clean_interrupted_staging_and_publish_a_complete_pair() -> io::Result<()> {
        let root = tempfile::Builder::new().prefix("ibis migration 漢字 ").tempdir()?;
        let bytes = vec![b'x'; 256 * 1024];
        let old = root.path().join("io.github.ibiscite.ibis");
        write(&old.join("settings.json"), &bytes)?;
        write(&old.join("settings.json.bak"), b"backup")?;
        let stale = root.path().join("com.ibiscite.ibis.migration-42-7");
        write(&stale.join("settings.json"), b"partial")?;
        let lock = File::create(root.path().join("com.ibiscite.ibis.migration.lock"))?;
        lock.lock()?;
        let mut group = MigrationChildren::new(std::time::Duration::from_secs(20));
        let executable = std::env::current_exe()?;
        for _ in 0..4 {
            group.spawn(root.path(), "migrate", &executable)?;
        }
        group.ready(root.path())?;
        let new = root.path().join("com.ibiscite.ibis");
        assert!(!new.exists());
        assert!(stale.exists());
        drop(lock);
        group.finish()?;
        for (name, expected) in [("settings.json", bytes.as_slice()), ("settings.json.bak", b"backup")] {
            assert_eq!(fs::read(new.join(name))?, expected);
            assert_eq!(fs::read(old.join(name))?, expected);
        }
        assert!(!stale.exists());
        for entry in fs::read_dir(root.path())? {
            assert!(
                !entry?
                    .file_name()
                    .to_string_lossy()
                    .starts_with("com.ibiscite.ibis.migration-")
            );
        }
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
        assert_eq!(std::fs::read_dir(root.path())?.count(), 2);
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
        write(
            &root.path().join("com.ibiscite.ibis.migration-42-7/settings.json"),
            b"interrupted",
        )?;
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
        assert_eq!(std::fs::read_dir(root.path())?.count(), 3);
        Ok(())
    }
}
