//! `ibis settings export|import`: back up the Ibis app's settings and restore
//! them on this computer or another one (#1561). The rules are the engine's
//! (`ibis_core::settings::backup`); this module only finds the app's settings
//! file and words the result. Paste approvals are never read or written.

use std::fs;
use std::path::{Path, PathBuf};

use clap::ArgMatches;
use ibis_core::contract::{ErrorCode, ExitCode};
use ibis_core::destination::System;
use ibis_core::settings::backup::{self, Backup, BackupError, Change, Note};
use ibis_core::settings::{self, PrimaryRead, SettingsError};
use ibis_engine::DispatchResult;
use serde_json::{Value, json};

use crate::{Failure, app_libraries, bounded};

pub(crate) fn command() -> clap::Command {
    use clap::{Arg, ArgAction, Command};
    let nightly = Arg::new("nightly")
        .long("nightly")
        .action(ArgAction::SetTrue)
        .help("Use Ibis Nightly's settings instead of the stable app's");
    Command::new("settings")
        .about("Back up the Ibis app's settings, or restore them on this or another computer")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("export")
                .about("Write a backup of the app's settings, without paste approvals or window places")
                .arg(
                    Arg::new("output")
                        .value_name("FILE")
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("The backup file to write; without one, or with -, the backup goes to stdout"),
                )
                .arg(
                    Arg::new("force")
                        .long("force")
                        .action(ArgAction::SetTrue)
                        .help("Replace FILE if it exists"),
                )
                .arg(nightly.clone()),
        )
        .subcommand(
            Command::new("import")
                .about("Restore a backup into the app's settings, keeping the replaced settings as settings.json.bak")
                .arg(
                    Arg::new("backup")
                        .value_name("FILE")
                        .required(true)
                        .value_parser(clap::value_parser!(PathBuf))
                        .help("The backup file to restore"),
                )
                .arg(
                    Arg::new("dry-run")
                        .long("dry-run")
                        .action(ArgAction::SetTrue)
                        .help("Show what would change and change nothing"),
                )
                .arg(nightly),
        )
}

/// The settings file a command uses: Nightly's when asked; otherwise the
/// stable app's, or Nightly's when only Nightly has settings here.
fn settings_file(nightly: bool) -> Result<(&'static str, PathBuf), Failure> {
    let directory = app_libraries::config_dir().ok_or_else(|| {
        Failure::new(
            ErrorCode::Usage,
            "this computer has no folder for app settings (HOME or APPDATA is not set).",
        )
    })?;
    let [stable, night] =
        app_libraries::APPS.map(|(identifier, app)| (app, directory.join(identifier).join("settings.json")));
    if nightly {
        return Ok(night);
    }
    let present = |path: &Path| {
        fs::symlink_metadata(path).is_ok() || fs::symlink_metadata(path.with_extension("json.bak")).is_ok()
    };
    let (stable_path, night_path) = (stable.1.clone(), night.1.clone());
    // A stalled settings folder is reported after the deadline, never taken as absent.
    let only_nightly = within("the settings folder", move || {
        !present(&stable_path) && present(&night_path)
    })?;
    Ok(if only_nightly { night } else { stable })
}

pub(crate) fn run(matches: &ArgMatches) -> Result<DispatchResult, Failure> {
    match matches.subcommand() {
        Some(("export", args)) => export(args),
        Some(("import", args)) => import(args),
        _ => Err(Failure::new(
            ErrorCode::Usage,
            "choose export or import; see: ibis settings --help",
        )),
    }
}

fn within<T: Send + 'static>(what: &str, operation: impl FnOnce() -> T + Send + 'static) -> Result<T, Failure> {
    bounded(operation).ok_or_else(|| {
        Failure::new(
            ErrorCode::SourceUnavailable,
            format!("{what} did not respond in time; try again."),
        )
    })
}

fn export(args: &ArgMatches) -> Result<DispatchResult, Failure> {
    let (app, path) = settings_file(args.get_flag("nightly"))?;
    let loaded = within("the settings file", move || settings::load(&path))?;
    if loaded.primary_read == PrimaryRead::Missing && loaded.problem.is_none() && !loaded.from_backup {
        return Err(Failure::new(
            ErrorCode::NotFound,
            format!("{app} has no settings on this computer yet; open it and choose some first."),
        ));
    }
    if let Some(problem) = &loaded.problem {
        // A backup of recovered or default settings would not be what the person chose.
        return Err(Failure::new(
            settings_code(problem),
            format!(
                "{app}'s settings cannot be backed up as they are: {}. Open {app} to repair them.",
                settings_text(problem)
            ),
        ));
    }
    let text = backup::export(&loaded.settings, ibis_core::VERSION)
        .map_err(|_| Failure::new(ErrorCode::ResourceLimit, "the backup would be larger than 1 MiB."))?;
    let destination = args.get_one::<PathBuf>("output").filter(|path| path.as_os_str() != "-");
    let Some(destination) = destination.cloned() else {
        let data: Value = serde_json::from_str(&text).unwrap_or_default();
        return Ok(result(json!({"app": app, "backup": data}), Some(text)));
    };
    let force = args.get_flag("force");
    // A write the person asked for runs to the end, so the result reported is
    // the one on disk.
    backup::save(&destination, &text, force).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AlreadyExists {
            Failure::new(
                ErrorCode::Usage,
                "that file exists already; choose another name, or add --force to replace it.",
            )
        } else {
            Failure::new(ErrorCode::SourceUnavailable, "the backup could not be written there.")
        }
    })?;
    let name = file_name(&destination);
    Ok(result(
        json!({"app": app, "written": name}),
        Some(format!(
            "Backed up {app}'s settings to {name}.\nPaste approvals and window places stay on this computer.\n"
        )),
    ))
}

fn import(args: &ArgMatches) -> Result<DispatchResult, Failure> {
    let (app, path) = settings_file(args.get_flag("nightly"))?;
    let chosen = args
        .get_one::<PathBuf>("backup")
        .cloned()
        .ok_or_else(|| Failure::new(ErrorCode::Usage, "name the backup file to restore."))?;
    let file = chosen.clone();
    let backup = within("the backup file", move || backup::read(&file))?.map_err(|error| backup_failure(&error))?;
    let settings_path = path.clone();
    let loaded = within("the settings file", move || settings::load(&settings_path))?;
    if let Some(problem) = &loaded.problem
        && settings_code(problem) == ErrorCode::NewerFormat
    {
        return Err(current_failure(app, problem));
    }
    let library = match &backup.settings.library {
        Some(settings::LibrarySetting::Bibtex(file)) => Some(file.clone()),
        _ => None,
    };
    let found = match library {
        Some(file) => bounded(move || file.try_exists().unwrap_or(false)).unwrap_or(false),
        None => true,
    };
    let plan = backup::plan(&loaded.settings, &backup, |_| found).repairing(backup::needs_repair(&loaded));
    let dry_run = args.get_flag("dry-run");
    if !dry_run && !plan.is_empty() {
        backup::restore(&path, &plan).map_err(|error| match &error {
            BackupError::Settings(problem) => current_failure(app, problem),
            BackupError::Changed => Failure::new(
                ErrorCode::SourceUnavailable,
                format!("{app}'s settings changed while restoring; nothing was restored. Try again."),
            ),
            _ => Failure::new(
                ErrorCode::SourceUnavailable,
                "the settings could not be saved; nothing was restored.",
            ),
        })?;
    }
    let restored = !dry_run && !plan.is_empty();
    let changes: Vec<Value> = plan.changes.iter().map(change_value).collect();
    let notes: Vec<&str> = plan.notes.iter().map(note_id).collect();
    let data = json!({
        "app": app,
        "made_on": backup.made_on,
        "made_by": backup.made_by,
        "changes": changes,
        "notes": notes,
        "restored": restored,
    });
    Ok(result(data, Some(import_text(app, &backup, &plan, dry_run))))
}

fn import_text(app: &str, backup: &Backup, plan: &backup::Plan, dry_run: bool) -> String {
    let made = format!(
        "a backup made on {} by Ibis {}",
        system_label(backup.made_on),
        backup.made_by
    );
    if plan.is_empty() {
        return format!("{app} already has the settings in {made}. Nothing changed.\n");
    }
    let mut text = if dry_run {
        format!("Restoring {made} would change {app}'s settings:\n")
    } else {
        format!("Restored {made} into {app}:\n")
    };
    for change in plan.changes.iter().map(backup::shown) {
        text.push_str(&format!(
            "  {}: {} → {}\n",
            setting_label(&change.setting),
            value_label(&change.setting, change.from.as_ref()),
            value_label(&change.setting, change.to.as_ref())
        ));
    }
    for note in &plan.notes {
        text.push_str(match note {
            Note::LibraryNotFound => {
                "Note: the backup's bibliography file is not on this computer. It is kept, and Ibis finds it once the file is there.\n"
            }
            Note::ShortcutKept => "Note: the backup has no opening shortcut for this system, so this computer's is kept.\n",
            Note::RepairsSettings => {
                "Note: the settings file on this computer is damaged or missing. Restoring writes it again, and keeps any damaged file beside it.\n"
            }
        });
    }
    text.push_str(if dry_run {
        "Nothing changed. Run it again without --dry-run to restore.\n"
    } else {
        "The replaced settings are kept as settings.json.bak. Paste approvals and window places stay as they were.\nIbis uses the restored settings the next time it opens; quit and reopen it if it is running.\n"
    });
    text
}

fn result(data: Value, text: Option<String>) -> DispatchResult {
    DispatchResult {
        data,
        text,
        source_findings: false,
        exit_code: ExitCode::Ok,
    }
}

/// Only the file's name: messages never carry private folders.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| "the backup".into(), |name| name.to_string_lossy().into_owned())
}

fn change_value(change: &Change) -> Value {
    let change = backup::shown(change);
    json!({"setting": change.setting, "from": change.from, "to": change.to})
}

fn note_id(note: &Note) -> &'static str {
    match note {
        Note::LibraryNotFound => "library-not-found",
        Note::ShortcutKept => "shortcut-kept",
        Note::RepairsSettings => "repairs-settings",
    }
}

fn system_label(system: System) -> &'static str {
    match system {
        System::Macos => "macOS",
        System::Windows => "Windows",
        System::Linux => "Linux",
    }
}

fn setting_label(setting: &str) -> String {
    let label = match setting {
        "library" => "Library",
        "copy_format" => "Key format",
        "citation_style" => "Citation style",
        "density" => "Row density",
        "larger_text" => "Larger text",
        "show_paste_preview" => "Show the citation",
        "search_icon" => "Search field icon",
        "paste" => "Enter pastes",
        "menu_bar" => "Menu bar item",
        "recent_citations" => "Recent citations",
        "small_surprises" => "Small surprises",
        "appearance" => "Appearance",
        "accent" => "Accent colour",
        _ => {
            return match setting.strip_prefix("shortcut.") {
                Some("macos") => "Opening shortcut on macOS".into(),
                Some("windows") => "Opening shortcut on Windows".into(),
                Some("linux") => "Opening shortcut on Linux".into(),
                _ => setting.to_owned(),
            };
        }
    };
    label.into()
}

fn value_label(setting: &str, value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "default".into(),
        Some(Value::Bool(true)) => "on".into(),
        Some(Value::Bool(false)) => "off".into(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Object(library)) if setting == "library" => match (library.get("bibtex"), library.get("zotero")) {
            (Some(Value::String(name)), _) => name.clone(),
            (_, Some(Value::Array(libraries))) => format!("{} Zotero libraries", libraries.len()),
            (_, Some(Value::String(_))) => "Zotero, My Library".into(),
            (_, Some(group)) => format!("Zotero group {}", group.get("group").unwrap_or(&Value::Null)),
            _ => "set".into(),
        },
        Some(other) => other.to_string(),
    }
}

fn backup_failure(error: &BackupError) -> Failure {
    let (code, sentence) = match error {
        BackupError::NotDownloaded => (
            ErrorCode::SourceUnavailable,
            "the backup is not downloaded to this computer yet; open it in Finder or File Explorer so it downloads, then try again.".into(),
        ),
        BackupError::Unreadable(error) if error.kind() == std::io::ErrorKind::NotFound => {
            (ErrorCode::NotFound, "there is no backup file at that path.".into())
        }
        BackupError::Unreadable(_) => (ErrorCode::SourceUnavailable, "the backup file could not be read.".into()),
        BackupError::NotABackup => (ErrorCode::BadInput, "that file is not an Ibis settings backup.".into()),
        BackupError::SettingsFile => (
            ErrorCode::BadInput,
            "that is an Ibis settings file, not a backup; make a backup with: ibis settings export FILE".into(),
        ),
        BackupError::NewerFormat(_) | BackupError::Settings(SettingsError::NewerVersion(_)) => (
            ErrorCode::NewerFormat,
            "the backup was made by a newer version of Ibis; update this ibis command to restore it.".into(),
        ),
        BackupError::Invalid(reason) => (ErrorCode::BadInput, format!("the backup is not valid: {reason}.")),
        BackupError::Settings(SettingsError::InvalidZoteroLibraryEntry { position }) => (
            ErrorCode::BadInput,
            format!("the backup is not valid: its Zotero library entry {position} is wrong."),
        ),
        BackupError::Settings(_) | BackupError::Changed => {
            (ErrorCode::BadInput, "the settings in the backup are not valid.".into())
        }
    };
    Failure::new(code, format!("{sentence} Nothing was changed."))
}

/// This computer's settings file cannot take a restore.
fn current_failure(app: &str, problem: &SettingsError) -> Failure {
    let fix = if settings_code(problem) == ErrorCode::NewerFormat {
        format!("update this ibis command to match {app}")
    } else {
        format!("open {app} to repair them")
    };
    Failure::new(
        settings_code(problem),
        format!(
            "{app}'s settings cannot be replaced: {}; {fix}. Nothing was changed.",
            settings_text(problem)
        ),
    )
}

fn settings_code(problem: &SettingsError) -> ErrorCode {
    match problem {
        SettingsError::NewerVersion(_) => ErrorCode::NewerFormat,
        SettingsError::Backup(inner) => settings_code(inner),
        SettingsError::Invalid(_) | SettingsError::InvalidZoteroLibraryEntry { .. } => ErrorCode::BadInput,
        SettingsError::Unreadable(_) => ErrorCode::SourceUnavailable,
    }
}

/// A fixed description, never the file's own text.
fn settings_text(problem: &SettingsError) -> &'static str {
    match problem {
        SettingsError::NewerVersion(_) => "they were written by a newer version of Ibis",
        SettingsError::Backup(inner) => settings_text(inner),
        SettingsError::Invalid(_) | SettingsError::InvalidZoteroLibraryEntry { .. } => "the settings file is not valid",
        SettingsError::Unreadable(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            "this program is not allowed to read the settings file"
        }
        SettingsError::Unreadable(_) => "the settings file could not be read",
    }
}
