//! The libraries chosen in the Ibis app, read from its settings file and never written.
//!
//! One-shot commands use them when no library is named; `--app-libraries` asks
//! for them by name, which `serve` and `mcp` need. Settings that cannot be used
//! as they are stop the command with their reason: Ibis never falls back to a
//! backup, a default or another app's settings when the chosen one is unusable.

use crate::Failure;
use ibis_core::contract::ErrorCode;
use ibis_core::settings::{self, LibrarySetting, PrimaryRead, SettingsError, ZoteroLibrary as Chosen};
use ibis_engine::request::ZoteroLibrary;
use std::path::{Path, PathBuf};

/// The apps whose settings are read, in order: the first one with a settings
/// file is used, so the stable app wins over Nightly when both are installed.
const APPS: [(&str, &str); 2] = [
    ("com.ibiscite.ibis", "Ibis"),
    ("com.ibiscite.ibis.nightly", "Ibis Nightly"),
];

pub(crate) const NAME_ONE: &str = "name one with --file PATH, --zotero or --zotero-group ID";

/// What the app chose, and where that choice was read.
#[derive(Debug)]
pub(crate) struct AppLibraries {
    pub app: &'static str,
    pub files: Vec<PathBuf>,
    pub zotero: Vec<ZoteroLibrary>,
}

/// The folder the app keeps its settings in: the same place as Tauri's
/// `app_config_dir`, from the environment so a test or a person can move it.
fn config_dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Application Support"))
    } else if cfg!(windows) {
        var("APPDATA")
    } else {
        var("XDG_CONFIG_HOME")
            .filter(|path| path.is_absolute())
            .or_else(|| var("HOME").map(|home| home.join(".config")))
    }
}

pub(crate) fn load() -> Result<AppLibraries, Failure> {
    let missing = || {
        Failure::new(
            ErrorCode::Usage,
            format!("no library is named and the Ibis app has no settings here; {NAME_ONE}."),
        )
    };
    let directory = config_dir().ok_or_else(missing)?;
    for (identifier, app) in APPS {
        if let Some(chosen) = read(app, &directory.join(identifier).join("settings.json"))? {
            return Ok(chosen);
        }
    }
    Err(missing())
}

/// `None` when this app has no settings file at all, so the next app is tried.
fn read(app: &'static str, path: &Path) -> Result<Option<AppLibraries>, Failure> {
    let loaded = settings::load(path);
    if loaded.primary_read == PrimaryRead::Missing && loaded.problem.is_none() && !loaded.from_backup {
        return Ok(None);
    }
    if let Some(problem) = &loaded.problem {
        let reason = if loaded.from_backup {
            format!("{}, and Ibis does not use its backup here", problem_text(problem))
        } else {
            problem_text(problem)
        };
        let code = problem_code(problem);
        let fix = if code == ErrorCode::NewerFormat {
            format!("Update this ibis command to match {app}")
        } else {
            format!("Open {app} to repair its settings")
        };
        return Err(Failure::new(
            code,
            format!("cannot read the libraries chosen in {app}: {reason}. {fix}, or {NAME_ONE}."),
        ));
    }
    let (files, zotero) = match loaded.settings.library {
        None => {
            return Err(Failure::new(
                ErrorCode::Usage,
                format!("no library is chosen in {app} yet; choose one there, or {NAME_ONE}."),
            ));
        }
        // A relative path would mean a different file in every folder; ask again instead.
        Some(LibrarySetting::Bibtex(file)) if !file.is_absolute() => {
            return Err(Failure::new(
                ErrorCode::BadInput,
                format!("the library chosen in {app} is not a full path; choose it again there, or {NAME_ONE}."),
            ));
        }
        Some(LibrarySetting::Bibtex(file)) => (vec![file], Vec::new()),
        Some(LibrarySetting::Zotero(libraries)) => (
            Vec::new(),
            libraries
                .libraries()
                .iter()
                .map(|library| match library {
                    Chosen::User => ZoteroLibrary::Personal,
                    Chosen::Group(id) => ZoteroLibrary::Group(*id),
                })
                .collect(),
        ),
    };
    Ok(Some(AppLibraries { app, files, zotero }))
}

/// A fixed description: never the settings' own text, which may hold private values.
fn problem_text(problem: &SettingsError) -> String {
    match problem {
        SettingsError::Unreadable(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            "this program is not allowed to read its settings file; allow it to, or run it outside the sandbox".into()
        }
        SettingsError::Unreadable(error) if error.kind() == std::io::ErrorKind::NotFound => {
            "its settings file is missing".into()
        }
        SettingsError::Unreadable(_) => "its settings file could not be read".into(),
        SettingsError::Invalid(_) => "its settings file is not valid".into(),
        SettingsError::InvalidZoteroLibraryEntry { position } => {
            format!("its settings file is not valid (Zotero library entry {position})")
        }
        SettingsError::NewerVersion(version) => {
            format!("its settings were written by a newer version of Ibis (format {version})")
        }
        SettingsError::Backup(inner) => problem_text(inner),
    }
}

fn problem_code(problem: &SettingsError) -> ErrorCode {
    match problem {
        SettingsError::NewerVersion(_) => ErrorCode::NewerFormat,
        SettingsError::Invalid(_) | SettingsError::InvalidZoteroLibraryEntry { .. } => ErrorCode::BadInput,
        SettingsError::Backup(inner) => problem_code(inner),
        SettingsError::Unreadable(_) => ErrorCode::SourceUnavailable,
    }
}
