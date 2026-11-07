//! `ibis settings export|import`: back up the Ibis app's settings and restore
//! them on this computer or another one (#1561). The rules are the engine's
//! (`ibis_engine::settings_backup`); this module only finds the app's settings
//! file and words the result. Paste approvals are never read or written.

use std::fs;
use std::path::{Path, PathBuf};

use clap::ArgMatches;
use ibis_core::contract::{ErrorCode, ExitCode};
use ibis_core::destination::System;
use ibis_engine::DispatchResult;
use ibis_engine::settings_backup::{self as backup, Backup, BackupError, Note};
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
pub(crate) fn settings_file(nightly: bool) -> Result<(&'static str, PathBuf), Failure> {
    let directory = ibis_engine::identity::config_dir().ok_or_else(|| {
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
    let text = backup::export_file(path, ibis_core::VERSION).map_err(|error| backup_failure(&error))?;
    let destination = args.get_one::<PathBuf>("output").filter(|path| path.as_os_str() != "-");
    let Some(destination) = destination.cloned() else {
        let data: Value = serde_json::from_str(&text).unwrap_or_default();
        return Ok(result(json!({"app": app, "backup": data}), Some(text)));
    };
    let force = args.get_flag("force");
    // A write the person asked for runs to the end, so the result reported is
    // the one on disk.
    backup::save(&destination, &text, force).map_err(|error| backup_failure(&error))?;
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
    let (backup, plan) = backup::prepare(path.clone(), chosen).map_err(|error| backup_failure(&error))?;
    let dry_run = args.get_flag("dry-run");
    if !dry_run && !plan.is_empty() {
        // Through the one settings owner, in turn with any other change (decision 0040).
        ibis_engine::settings::Owner::open(path.clone())
            .in_turn(|path| backup::restore(path, &plan))
            .unwrap_or_else(|| Err(BackupError::NotSaved(std::io::Error::other("settings are closing"))))
            .map_err(|error| backup_failure(&error))?;
    }
    let restored = !dry_run && !plan.is_empty();
    let changes: Vec<_> = plan.changes.iter().map(backup::shown).collect();
    let data = json!({
        "app": app,
        "made_on": backup.made_on,
        "made_by": backup.made_by,
        "changes": changes,
        "notes": plan.notes,
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
        "output" => "Default format",
        "density" => "Row density",
        "larger_text" => "Larger text",
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
    Failure::new(backup::code(error), backup::message(error))
}
