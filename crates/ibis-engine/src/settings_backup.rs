//! Shared settings backup operations. Reads have a deadline; a requested write
//! finishes before returning. Paths and restore plans remain native.
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub use backup::{Backup, BackupError, BackupProblem, Note, Plan, shown};
use ibis_core::contract::ErrorCode;
use ibis_core::settings::backup;
use ibis_core::settings::{self, PrimaryRead, SettingsError};

const MAX_IN_FLIGHT: usize = 8;
const DEADLINE: Duration = Duration::from_secs(2);

pub fn export_file(path: PathBuf, version: &str) -> Result<String, BackupError> {
    let loaded = within(move || settings::load(&path)).ok_or_else(settings_timeout)?;
    if let Some(problem) = loaded.problem {
        return Err(BackupError::Current(problem));
    }
    if loaded.primary_read == PrimaryRead::Missing && !loaded.from_backup {
        return Err(BackupError::MissingSettings);
    }
    backup::export(&loaded.settings, version)
}

/// Prepare without writing. The caller retains this exact plan until commit.
pub fn prepare(path: PathBuf, source: PathBuf) -> Result<(Backup, Plan), BackupError> {
    let backup = within(move || backup::read(&source))
        .ok_or_else(|| BackupError::Unreadable(io::ErrorKind::TimedOut.into()))??;
    let mut loaded = within(move || settings::load(&path)).ok_or_else(settings_timeout)?;
    if let Some(problem) = loaded
        .problem
        .take_if(|problem| settings_code(problem) == ErrorCode::NewerFormat)
    {
        return Err(BackupError::Current(problem));
    }
    let plan = backup::plan(&loaded.settings, &backup, |library| {
        let library = library.to_path_buf();
        within(move || library.try_exists().unwrap_or(false)).unwrap_or(false)
    })
    .repairing(backup::needs_repair(&loaded));
    Ok((backup, plan))
}

/// Core holds the cross-process writer lock through revalidation and commit.
pub fn restore(path: &Path, plan: &Plan) -> Result<settings::Settings, BackupError> {
    backup::restore(path, plan).map_err(|error| match error {
        BackupError::Settings(problem) => BackupError::Current(problem),
        other => other,
    })
}

pub fn save(path: &Path, text: &str, replace: bool) -> Result<(), BackupError> {
    backup::save(path, text, replace).map_err(BackupError::NotSaved)
}

fn settings_timeout() -> BackupError {
    BackupError::Current(SettingsError::Unreadable(io::ErrorKind::TimedOut.into()))
}

/// A fixed UI classification: no setting values, file contents or OS payloads.
pub fn problem(error: &BackupError) -> BackupProblem {
    match error {
        BackupError::MissingSettings | BackupError::Current(_) => BackupProblem::SettingsUnusable,
        BackupError::NotSaved(_) => BackupProblem::NotSaved,
        BackupError::NotDownloaded => BackupProblem::NotDownloaded,
        BackupError::Unreadable(_) => BackupProblem::Unreadable,
        BackupError::NotABackup => BackupProblem::NotABackup,
        BackupError::SettingsFile => BackupProblem::SettingsFile,
        BackupError::NewerFormat(_) => BackupProblem::Newer,
        BackupError::Settings(error) if settings_code(error) == ErrorCode::NewerFormat => BackupProblem::Newer,
        BackupError::Invalid(_) | BackupError::Settings(_) => BackupProblem::Invalid,
        BackupError::Changed => BackupProblem::Changed,
    }
}

pub fn code(error: &BackupError) -> ErrorCode {
    match error {
        BackupError::MissingSettings => ErrorCode::NotFound,
        BackupError::Unreadable(error) if error.kind() == io::ErrorKind::NotFound => ErrorCode::NotFound,
        BackupError::NotSaved(error) if error.kind() == io::ErrorKind::AlreadyExists => ErrorCode::Usage,
        BackupError::Current(error) => settings_code(error),
        BackupError::Invalid("the backup would be larger than 1 MiB") => ErrorCode::ResourceLimit,
        _ => match problem(error) {
            BackupProblem::Newer => ErrorCode::NewerFormat,
            BackupProblem::NotABackup | BackupProblem::SettingsFile | BackupProblem::Invalid => ErrorCode::BadInput,
            _ => ErrorCode::SourceUnavailable,
        },
    }
}

pub fn message(error: &BackupError) -> String {
    let sentence = match error {
        BackupError::MissingSettings => "there are no settings on this computer yet; open Ibis and choose some first.".into(),
        BackupError::Current(error) => format!("{}; {}.", settings_message(error),
            if settings_code(error) == ErrorCode::NewerFormat { "update Ibis before trying again" } else { "Open Ibis to repair them" }),
        BackupError::NotSaved(error) if error.kind() == io::ErrorKind::AlreadyExists =>
            "that file exists already; choose another name, or add --force to replace it.".into(),
        BackupError::NotSaved(_) => "the backup could not be written there.".into(),
        BackupError::NotDownloaded => "the backup is not downloaded to this computer yet; open it in Finder or File Explorer so it downloads, then try again.".into(),
        BackupError::Unreadable(error) if error.kind() == io::ErrorKind::NotFound => "there is no backup file at that path.".into(),
        BackupError::Unreadable(_) => "the backup file could not be read.".into(),
        BackupError::NotABackup => "that file is not an Ibis settings backup.".into(),
        BackupError::SettingsFile => "that is an Ibis settings file, not a backup; make a backup with: ibis settings export FILE".into(),
        BackupError::Invalid(reason) => format!("the backup is not valid: {reason}."),
        BackupError::Changed => "the settings changed while restoring; nothing was restored. Try again.".into(),
        _ if problem(error) == BackupProblem::Newer => "the backup was made by a newer version of Ibis; update Ibis to restore it.".into(),
        BackupError::Settings(SettingsError::InvalidZoteroLibraryEntry { position }) =>
            format!("the backup is not valid: its Zotero library entry {position} is wrong."),
        _ => "the settings in the backup are not valid.".into(),
    };
    format!("{sentence} Nothing was changed.")
}

fn settings_code(problem: &SettingsError) -> ErrorCode {
    match problem {
        SettingsError::NewerVersion(_) => ErrorCode::NewerFormat,
        SettingsError::Backup(inner) => settings_code(inner),
        SettingsError::Invalid(_) | SettingsError::InvalidZoteroLibraryEntry { .. } => ErrorCode::BadInput,
        SettingsError::Unreadable(_) => ErrorCode::SourceUnavailable,
    }
}

/// Settings errors can contain user-entered values or OS details. Only these
/// fixed descriptions reach diagnostics or messages about failed saves.
pub fn settings_problem(problem: &settings::SettingsError) -> &'static str {
    match problem {
        settings::SettingsError::Unreadable(_) => "settings.json could not be read",
        settings::SettingsError::Invalid(_) => "settings.json is not valid",
        settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
            "settings.json has an invalid Zotero library entry"
        }
        settings::SettingsError::NewerVersion(_) => "settings.json was written by a newer version of Ibis",
        settings::SettingsError::Backup(error) => match error.as_ref() {
            settings::SettingsError::NewerVersion(_) => "settings.json.bak was written by a newer version of Ibis",
            settings::SettingsError::Invalid(_) | settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
                "settings.json.bak is not valid"
            }
            _ => "settings.json.bak could not be read",
        },
    }
}

/// Interface and failed-save text may include a typed position, never raw
/// serde or operating-system details. Diagnostics keep the fixed description.
pub fn settings_message(problem: &settings::SettingsError) -> String {
    match problem {
        settings::SettingsError::InvalidZoteroLibraryEntry { .. } => {
            problem.to_string().replacen("the settings file", "settings.json", 1)
        }
        settings::SettingsError::Backup(error) => {
            settings_message(error).replacen("settings.json", "settings.json.bak", 1)
        }
        _ => settings_problem(problem).to_string(),
    }
}

/// Run read-only file work for at most two seconds. At most eight calls run
/// at once: a stalled call keeps its place until it finishes, bounding retries.
pub fn within<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
    bounded(&IN_FLIGHT, work)
}

fn bounded<T: Send + 'static>(in_flight: &'static AtomicUsize, work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    struct Place(&'static AtomicUsize);
    impl Drop for Place {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }
    let place = Place(in_flight);
    if in_flight.fetch_add(1, Ordering::AcqRel) >= MAX_IN_FLIGHT {
        return None;
    }
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("ibis-settings-file".into())
        .spawn(move || {
            let _place = place;
            let _ = sender.send(work());
        })
        .ok()?;
    receiver.recv_timeout(DEADLINE).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn stuck_file_calls_have_a_fixed_limit_until_they_finish() {
        static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
        let within = |work: fn() -> i32| bounded(&IN_FLIGHT, work);
        let (release, wait) = std::sync::mpsc::channel::<()>();
        let wait = std::sync::Arc::new(Mutex::new(wait));
        let stuck: Vec<_> = (0..MAX_IN_FLIGHT)
            .map(|_| {
                let wait = wait.clone();
                std::thread::spawn(move || bounded(&IN_FLIGHT, move || drop(wait.lock().map(|w| w.recv()))))
            })
            .collect();
        for call in stuck {
            assert!(
                call.join().ok().flatten().is_none(),
                "a stalled call gives up at the deadline"
            );
        }
        assert!(within(|| 1).is_none(), "no new worker while eight are stuck");
        for _ in 0..MAX_IN_FLIGHT {
            let _ = release.send(());
        }
        let started = std::time::Instant::now();
        while within(|| 1).is_none() {
            assert!(
                started.elapsed() < DEADLINE * 2,
                "places come back once the calls finish"
            );
            std::thread::yield_now();
        }
    }
}
