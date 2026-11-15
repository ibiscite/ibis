//! The libraries chosen in the Ibis app, read from its settings file and never written.
//!
//! One-shot commands use them when no library is named; `--app-libraries` asks
//! for them by name, which `serve` and `mcp` need. Settings that cannot be used
//! as they are stop the command with their reason: Ibis never falls back to a
//! backup, a default or another app's settings when the chosen one is unusable.

use crate::{Failure, human::cli};
use ibis_core::contract::ErrorCode;
use ibis_engine::request::ZoteroLibrary;
use ibis_engine::settings::{self, LibrarySetting, PrimaryRead, SettingsError, ZoteroLibrary as Chosen};
use std::path::{Path, PathBuf};

/// The apps whose settings are read, in order: the first one with a settings
/// file is used, so the stable app wins over Nightly when both are installed.
pub(crate) const APPS: [(&str, &str); 2] = [
    ("com.ibiscite.ibis", "Ibis"),
    ("com.ibiscite.ibis.nightly", "Ibis Nightly"),
];

/// What the app chose, and where that choice was read.
#[derive(Debug)]
pub(crate) struct AppLibraries {
    pub app: &'static str,
    pub files: Vec<PathBuf>,
    pub zotero: Vec<ZoteroLibrary>,
    pub zotero_profile: Option<String>,
}

pub(crate) fn load() -> Result<AppLibraries, Failure> {
    let missing = || Failure::new(ErrorCode::Usage, cli("cli-app-settings-missing", &[]));
    let directory = ibis_engine::identity::config_dir().ok_or_else(missing)?;
    for (identifier, app) in APPS {
        if let Some(chosen) = read(app, &directory.join(identifier).join("settings.json"))? {
            return Ok(chosen);
        }
    }
    Err(missing())
}

/// The app's settings, for the format it uses for each app (decision 0042):
/// none yet means nothing is chosen, and settings that cannot be used as they
/// are stop the command with their reason, never falling back.
pub(crate) fn format_settings(nightly: bool) -> Result<settings::Settings, Failure> {
    let (app, path) = crate::settings_backup::settings_file(nightly)?;
    let loaded = settings::load(&path);
    match loaded.cites() {
        Err(problem) => Err(Failure::new(
            ibis_engine::settings_backup::settings_code(problem),
            cli(
                "cli-app-formats-unreadable",
                &[("app", app), ("reason", &problem_text(problem))],
            ),
        )),
        Ok(settings) => Ok(settings.clone()),
    }
}

/// `None` when this app has no settings file at all, so the next app is tried.
fn read(app: &'static str, path: &Path) -> Result<Option<AppLibraries>, Failure> {
    let loaded = settings::load(path);
    if loaded.primary_read == PrimaryRead::Missing && loaded.problem.is_none() && !loaded.from_backup {
        return Ok(None);
    }
    let settings = loaded.cites().map_err(|problem| {
        let reason = if loaded.from_backup {
            cli("cli-app-settings-backup-unused", &[("reason", &problem_text(problem))])
        } else {
            problem_text(problem)
        };
        let code = ibis_engine::settings_backup::settings_code(problem);
        let fix = if code == ErrorCode::NewerFormat {
            cli("cli-app-settings-update", &[("app", app)])
        } else {
            cli("cli-app-settings-repair", &[("app", app)])
        };
        Failure::new(
            code,
            cli(
                "cli-app-libraries-unreadable",
                &[("app", app), ("reason", &reason), ("fix", &fix)],
            ),
        )
    })?;
    let (files, zotero) = match settings.portable.library.clone() {
        None => {
            return Err(Failure::new(
                ErrorCode::Usage,
                cli("cli-app-library-unchosen", &[("app", app)]),
            ));
        }
        // A relative path would mean a different file in every folder; ask again instead.
        Some(LibrarySetting::Bibtex(file)) if !file.is_absolute() => {
            return Err(Failure::new(
                ErrorCode::BadInput,
                cli("cli-app-library-relative", &[("app", app)]),
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
    Ok(Some(AppLibraries {
        app,
        files,
        zotero,
        zotero_profile: settings.device.zotero_profile.clone(),
    }))
}

/// A fixed description: never the settings' own text, which may hold private values.
fn problem_text(problem: &SettingsError) -> String {
    match problem {
        SettingsError::Changed => cli("cli-app-settings-changed", &[]),
        SettingsError::Busy => ibis_engine::settings_backup::settings_message(problem),
        SettingsError::Unreadable(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            cli("cli-app-settings-permission", &[])
        }
        SettingsError::Unreadable(error) if error.kind() == std::io::ErrorKind::NotFound => {
            cli("cli-app-settings-file-missing", &[])
        }
        SettingsError::Unreadable(_) => cli("cli-app-settings-unreadable", &[]),
        SettingsError::Invalid(_) => cli("cli-app-settings-invalid", &[]),
        SettingsError::InvalidZoteroLibraryEntry { position } => cli(
            "cli-app-settings-zotero-invalid",
            &[("position", &position.to_string())],
        ),
        SettingsError::NewerVersion(version) => cli("cli-app-settings-newer", &[("version", &version.to_string())]),
        SettingsError::Backup(inner) => problem_text(inner),
    }
}
