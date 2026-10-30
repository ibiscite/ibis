//! Open a native-owned source. IPC accepts an item identity, never a path,
//! arbitrary URI, executable or shell command.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use ibis_core::contract::{ItemRef, LibraryObject};
use ibis_engine::request::ZoteroLibrary;
use serde::Deserialize;
use tauri::State;

use super::state::Library;
use super::{CommandError, Source};

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Zotero,
    ZoteroItem(String),
    File(PathBuf),
}

const FAILED: &str = "The source could not be opened. Try again.";

#[derive(Deserialize, specta::Type)]
#[serde(transparent)]
pub(crate) struct SourceItem(#[specta(type = specta_typescript::Unknown)] ItemRef);

/// Open the searched source or launch Zotero for a reported Zotero problem.
#[tauri::command]
#[specta::specta]
pub(crate) async fn open_library_source(
    item: Option<SourceItem>,
    expected_revision: String,
    zotero_only: bool,
    library: State<'_, Mutex<Library>>,
) -> Result<(), CommandError> {
    let target = {
        let library = library.lock().map_err(|_| CommandError::from(FAILED))?;
        target(
            &library,
            item.as_ref().map(|item| &item.0),
            &expected_revision,
            zotero_only,
        )?
    };
    tauri::async_runtime::spawn_blocking(move || launch(target))
        .await
        .map_err(|_| CommandError::from(FAILED))?
}

fn target(
    library: &Library,
    item: Option<&ItemRef>,
    expected: &str,
    zotero_only: bool,
) -> Result<Target, CommandError> {
    if library.file.as_ref().map(|file| file.revision()).unwrap_or_default() != expected {
        return Err(CommandError::SnapshotChanged);
    }
    if zotero_only {
        return if matches!(
            library.requested.as_ref().or(library.source.as_ref()),
            Some(Source::Zotero(_))
        ) {
            Ok(Target::Zotero)
        } else {
            Err(FAILED.into())
        };
    }
    let source = library.source.as_ref().or(library.requested.as_ref()).ok_or(FAILED)?;
    match source {
        Source::Samples(_) => Err(FAILED.into()),
        Source::File(path) => Ok(Target::File(path.clone())),
        Source::Zotero(_) => {
            let Some(item) = item else { return Ok(Target::Zotero) };
            let file = library.file.as_ref().ok_or(FAILED)?;
            let snapshot = file.engine.snapshot().map_err(|_| CommandError::SnapshotChanged)?;
            if snapshot.fingerprints().full.as_str() != expected
                || !snapshot
                    .get(item)
                    .is_some_and(|object| matches!(object.object, LibraryObject::Record(_)))
            {
                return Err(CommandError::SnapshotChanged);
            }
            let info = file.engine.libraries().map_err(|_| CommandError::from(FAILED))?;
            let source = info
                .libraries
                .iter()
                .find(|info| info.id == item.library)
                .and_then(|info| info.zotero);
            Ok(Target::ZoteroItem(zotero_uri(source.ok_or(FAILED)?, &item.key)?))
        }
    }
}

fn zotero_uri(library: ZoteroLibrary, key: &str) -> Result<String, CommandError> {
    // Native Zotero keys are eight ASCII letters/digits, distinct from
    // citation keys. Refuse separators, query parameters and URI injection.
    if key.len() != 8 || !key.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Err(FAILED.into());
    }
    let library = match library {
        ZoteroLibrary::Personal => "library".into(),
        ZoteroLibrary::Group(id) if id > 0 => format!("groups/{id}"),
        _ => return Err(FAILED.into()),
    };
    Ok(format!("zotero://select/{library}/items/{key}"))
}

fn source_file(path: &Path) -> Result<PathBuf, CommandError> {
    let path = path.canonicalize().map_err(|_| CommandError::from(FAILED))?;
    let supported = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["bib", "bibtex", "json", "csljson"]
                .iter()
                .any(|allowed| extension.eq_ignore_ascii_case(allowed))
        });
    if !path.is_file() || !supported {
        return Err(FAILED.into());
    }
    Ok(path)
}

fn command(target: Target) -> Result<Command, CommandError> {
    #[cfg(target_os = "macos")]
    let zotero = matches!(&target, Target::Zotero | Target::ZoteroItem(_));
    let argument = match target {
        Target::Zotero => "zotero://select/library".into(),
        Target::ZoteroItem(uri) => uri.into(),
        Target::File(path) => source_file(&path)?.into_os_string(),
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/open");
        if zotero {
            command.args(["-b", "org.zotero.zotero"]);
        }
        command.arg("--");
        command
    };
    #[cfg(target_os = "windows")]
    let mut command = {
        // ShellExecute's URL handler, directly invoked: no cmd.exe or `start`.
        let mut command = Command::new("rundll32.exe");
        command.arg("url.dll,FileProtocolHandler");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    command
        .arg(argument)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(command)
}

fn launch(target: Target) -> Result<(), CommandError> {
    run(&mut command(target)?)
}

fn run(command: &mut Command) -> Result<(), CommandError> {
    if command.status().is_ok_and(|status| status.success()) {
        Ok(())
    } else {
        Err(FAILED.into())
    }
}

#[cfg(test)]
mod tests {
    use super::super::state::candidate;
    use super::*;

    #[test]
    fn links_use_native_item_keys_and_exact_library_identity() {
        assert_eq!(
            zotero_uri(ZoteroLibrary::Personal, "AB12CD34").ok().as_deref(),
            Some("zotero://select/library/items/AB12CD34")
        );
        assert_eq!(
            zotero_uri(ZoteroLibrary::Group(7), "AB12CD34").ok().as_deref(),
            Some("zotero://select/groups/7/items/AB12CD34")
        );
        for key in [
            "citation-key",
            "../../sh",
            "ABCD?x=1",
            "ABCD#123",
            "abcd1234",
            "ABCD1234/extra",
        ] {
            assert!(zotero_uri(ZoteroLibrary::Personal, key).is_err());
        }
    }

    #[test]
    fn failed_switch_opens_the_searched_source_and_stale_requests_refuse() {
        let mut library = candidate(PathBuf::from("retained.bib"), "@misc{synthetic}");
        library.requested = Some(Source::File("missing.bib".into()));
        let revision = library.file.as_ref().map(|file| file.revision()).unwrap_or_default();
        assert_eq!(
            target(&library, None, &revision, false).ok(),
            Some(Target::File("retained.bib".into()))
        );
        assert!(matches!(
            target(&library, None, "old", false),
            Err(CommandError::SnapshotChanged)
        ));
        assert!(target(&library, None, &revision, true).is_err());
        library.requested = Some(Source::Zotero(ibis_core::settings::ZoteroLibrary::User.into()));
        assert_eq!(target(&library, None, &revision, true).ok(), Some(Target::Zotero));
        assert!(target(&Library::default(), None, "", false).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn failed_launches_return_only_the_fixed_failure() {
        assert!(run(&mut Command::new("/usr/bin/true")).is_ok());
        for mut command in [
            Command::new("/usr/bin/false"),
            Command::new("/invented/unavailable-launcher"),
        ] {
            assert_eq!(
                run(&mut command).err().map(|error| error.to_string()).as_deref(),
                Some(FAILED)
            );
        }
    }

    #[test]
    fn only_existing_library_files_are_opened_and_the_path_is_one_literal_argument()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("-invented ; $(touch nope) & 'library'.bib");
        std::fs::write(&path, "@misc{synthetic}")?;
        let command = command(Target::File(path.clone())).map_err(|e| e.to_string())?;
        assert_eq!(command.get_args().last(), Some(path.canonicalize()?.as_os_str()));
        assert!(!directory.path().join("nope").exists());
        let executable = directory.path().join("invented.command");
        std::fs::write(&executable, "@misc{synthetic}")?;
        assert!(source_file(&executable).is_err());
        assert!(source_file(directory.path()).is_err());
        std::fs::remove_file(&path)?;
        assert!(source_file(&path).is_err());
        Ok(())
    }
}
