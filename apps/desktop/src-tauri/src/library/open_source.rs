//! Open a native-owned source. IPC accepts an item identity, never a path,
//! arbitrary URI, executable or shell command.

use std::path::{Path, PathBuf};
#[cfg(not(target_os = "windows"))]
use std::process::{Command, Stdio};
use std::sync::Mutex;

use ibis_core::contract::{ItemRef, LibraryObject};
use ibis_engine::request::ZoteroLibrary;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

mod icons;

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
    reveal: bool,
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
    tauri::async_runtime::spawn_blocking(move || launch(target, reveal))
        .await
        .map_err(|_| CommandError::from(FAILED))?
}

#[derive(Clone, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceKind {
    Zotero,
    File,
}

#[derive(Clone, Serialize, specta::Type)]
pub(crate) struct SystemIcon {
    mime: String,
    bytes: Vec<u8>,
}

#[derive(Serialize, specta::Type)]
pub(crate) struct SourcePresentation {
    kind: SourceKind,
    name: String,
    icon: Option<SystemIcon>,
    /// The searched library may differ from the selected/default source.
    status_icon: Option<SystemIcon>,
}

/// Resolve the selected source and its system icon, never exposing its path.
#[tauri::command]
#[specta::specta]
pub(crate) async fn library_source(
    app: AppHandle,
    item: Option<SourceItem>,
    expected_revision: String,
    zotero_only: bool,
    with_status_icon: bool,
    library: State<'_, Mutex<Library>>,
) -> Result<SourcePresentation, CommandError> {
    let (target, status_path) = {
        let library = library.lock().map_err(|_| CommandError::from(FAILED))?;
        let target = target(
            &library,
            item.as_ref().map(|item| &item.0),
            &expected_revision,
            zotero_only,
        )?;
        let status_path = match library.source.as_ref().or(library.requested.as_ref()) {
            Some(Source::File(path) | Source::Samples(path)) => Some(path.clone()),
            _ => None,
        };
        (target, status_path)
    };
    let (kind, name, path) = match target {
        Target::File(path) => (
            SourceKind::File,
            path.file_name().ok_or(FAILED)?.to_string_lossy().into_owned(),
            Some(path),
        ),
        _ => (SourceKind::Zotero, "Zotero".into(), None),
    };
    let same_icon = path == status_path;
    let icon = icons::load(app.clone(), path).await;
    let status_icon = if !with_status_icon {
        None
    } else if same_icon {
        icon.clone()
    } else {
        icons::load(app, status_path).await
    };
    Ok(SourcePresentation {
        kind,
        name,
        icon,
        status_icon,
    })
}

fn target(
    library: &Library,
    item: Option<&ItemRef>,
    expected: &str,
    zotero_only: bool,
) -> Result<Target, CommandError> {
    // Opening the app itself is independent of a library snapshot. In
    // particular it still works on first run or after a failed status read.
    if zotero_only || item.is_none() {
        return Ok(Target::Zotero);
    }
    if library.file.as_ref().map(|file| file.revision()).unwrap_or_default() != expected {
        return Err(CommandError::SnapshotChanged);
    }
    let item = item.ok_or(FAILED)?;
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
        .ok_or(FAILED)?;
    if let Some(zotero) = source.zotero {
        return Ok(Target::ZoteroItem(zotero_uri(zotero, &item.key)?));
    }
    match library.source.as_ref() {
        Some(Source::File(path) | Source::Samples(path)) => Ok(Target::File(path.clone())),
        _ => Err(FAILED.into()),
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

#[cfg(not(target_os = "windows"))]
fn command(target: Target, reveal: bool) -> Result<Command, CommandError> {
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
        if reveal && !zotero {
            command.arg("-R");
        }
        command.arg("--");
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = Command::new("xdg-open");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let argument = if reveal && !argument.to_string_lossy().starts_with("zotero:") {
        Path::new(&argument).parent().ok_or(FAILED)?.as_os_str().to_os_string()
    } else {
        argument
    };
    command
        .arg(argument)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    Ok(command)
}

#[cfg(target_os = "windows")]
fn launch(target: Target, reveal: bool) -> Result<(), CommandError> {
    use windows::{
        Foundation::Uri,
        Storage::{IStorageItem, StorageFile, StorageFolder},
        System::{FolderLauncherOptions, Launcher},
        core::{HSTRING, Interface},
    };
    // Storage objects keep filenames literal, including commas and spaces.
    // Launcher's boolean reports an absent handler; no URL-handler shell or
    // command-line parser can hide a failed open or reinterpret the path.
    let failed = |_| CommandError::from(FAILED);
    let opened = match target {
        Target::File(path) => {
            let canonical = source_file(&path)?;
            let path = dunce::simplified(&canonical);
            let location = path.to_str().ok_or(FAILED)?;
            let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(location))
                .map_err(failed)?
                .join()
                .map_err(failed)?;
            if reveal {
                let parent = path.parent().and_then(Path::to_str).ok_or(FAILED)?;
                let folder = StorageFolder::GetFolderFromPathAsync(&HSTRING::from(parent))
                    .map_err(failed)?
                    .join()
                    .map_err(failed)?;
                let options = FolderLauncherOptions::new().map_err(failed)?;
                let item = file.cast::<IStorageItem>().map_err(failed)?;
                options.ItemsToSelect().map_err(failed)?.Append(&item).map_err(failed)?;
                Launcher::LaunchFolderWithOptionsAsync(&folder, &options)
                    .map_err(failed)?
                    .join()
                    .map_err(failed)?
            } else {
                Launcher::LaunchFileAsync(&file)
                    .map_err(failed)?
                    .join()
                    .map_err(failed)?
            }
        }
        target => {
            let uri = match target {
                Target::ZoteroItem(uri) => uri,
                _ => "zotero://select/library".into(),
            };
            let uri = Uri::CreateUri(&HSTRING::from(uri)).map_err(failed)?;
            Launcher::LaunchUriAsync(&uri).map_err(failed)?.join().map_err(failed)?
        }
    };
    if opened { Ok(()) } else { Err(FAILED.into()) }
}

#[cfg(not(target_os = "windows"))]
fn launch(target: Target, reveal: bool) -> Result<(), CommandError> {
    run(&mut command(target, reveal)?)
}

#[cfg(not(target_os = "windows"))]
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
        assert_eq!(target(&library, None, &revision, false).ok(), Some(Target::Zotero));
        assert_eq!(target(&library, None, "old", false).ok(), Some(Target::Zotero));
        let item = library
            .file
            .as_ref()
            .and_then(|file| file.engine.snapshot().ok())
            .and_then(|snapshot| snapshot.objects().first().map(|object| object.object.id().clone()))
            .unwrap_or_else(|| panic!("invented record"));
        assert_eq!(
            target(&library, Some(&item), &revision, false).ok(),
            Some(Target::File("retained.bib".into()))
        );
        assert!(matches!(
            target(&library, Some(&item), "old", false),
            Err(CommandError::SnapshotChanged)
        ));
        let mut forged = item.clone();
        forged.key = "not-a-record".into();
        assert!(target(&library, Some(&forged), &revision, false).is_err());
        library.source = Some(Source::Samples("samples.csljson".into()));
        assert_eq!(
            target(&library, Some(&item), &revision, false).ok(),
            Some(Target::File("samples.csljson".into()))
        );
        assert_eq!(target(&Library::default(), None, "", true).ok(), Some(Target::Zotero));
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
        #[cfg(not(target_os = "windows"))]
        {
            let command = command(Target::File(path.clone()), false).map_err(|e| e.to_string())?;
            assert_eq!(command.get_args().last(), Some(path.canonicalize()?.as_os_str()));
        }
        assert!(!directory.path().join("nope").exists());
        let executable = directory.path().join("invented.command");
        std::fs::write(&executable, "@misc{synthetic}")?;
        assert!(source_file(&executable).is_err());
        assert!(source_file(directory.path()).is_err());
        std::fs::remove_file(&path)?;
        assert!(source_file(&path).is_err());
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn revealing_uses_the_native_file_manager_with_one_literal_path() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("invented & reference.bib");
        std::fs::write(&path, "@misc{synthetic}")?;
        let reveal = command(Target::File(path.clone()), true).map_err(|e| e.to_string())?;
        #[cfg(target_os = "macos")]
        {
            assert_eq!(reveal.get_program(), "/usr/bin/open");
            assert_eq!(
                reveal.get_args().collect::<Vec<_>>(),
                [
                    std::ffi::OsStr::new("-R"),
                    std::ffi::OsStr::new("--"),
                    path.canonicalize()?.as_os_str()
                ]
            );
            let zotero = command(Target::Zotero, true).map_err(|e| e.to_string())?;
            assert!(!zotero.get_args().any(|arg| arg == "-R"));
            assert!(zotero.get_args().any(|arg| arg == "org.zotero.zotero"));
        }
        #[cfg(target_os = "linux")]
        assert_eq!(
            reveal.get_args().last(),
            path.canonicalize()?.parent().map(Path::as_os_str)
        );
        Ok(())
    }
}
