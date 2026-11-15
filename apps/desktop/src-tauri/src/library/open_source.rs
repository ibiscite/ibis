//! Open a native-owned source. IPC accepts an item identity, never a path,
//! arbitrary URI, executable or shell command.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use ibis_core::contract::{ItemRef, LibraryId, LibraryObject};
use ibis_engine::request::ZoteroLibrary;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

mod icons;

static OPENS: AtomicU64 = AtomicU64::new(0);

use super::state::Library;
use super::{CommandError, Source};
use crate::diagnostics::{NativeIssue, report};
use crate::picker::{PickerGeneration, Takeover, yield_to_window};

#[derive(Debug, PartialEq, Eq)]
enum Target {
    Zotero(ZoteroLibrary),
    ZoteroItem(String),
    File(PathBuf),
}

/// Why a source could not be opened, whatever the cause.
fn failed() -> CommandError {
    CommandError::SourceNotOpened
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum SourceItem {
    Item(ItemRef),
    #[serde(skip)]
    #[schemars(skip)]
    Library {
        library: LibraryId,
    },
}

/// Open the searched source or launch Zotero for a reported Zotero problem.
#[tauri::command]
#[ibis_desktop_macros::command]
#[allow(clippy::too_many_arguments)]
pub(crate) async fn open_library_source(
    item: Option<SourceItem>,
    expected_revision: String,
    expected_source_generation: String,
    zotero_only: bool,
    reveal: bool,
    presentation: Option<u64>,
    app: AppHandle,
    library: State<'_, Mutex<Library>>,
) -> Result<(), CommandError> {
    // Named by the picker: this body runs after Tauri schedules it, and a
    // launch that ends later gives way for the opening that asked alone.
    let presentation = app.state::<PickerGeneration>().originating(presentation);
    let request = OPENS.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let target = {
        let library = library.lock().map_err(|_| failed())?;
        target(
            &library,
            item.as_ref(),
            &expected_revision,
            &expected_source_generation,
            zotero_only,
        )?
    };
    let owner = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let target = match target {
            Target::ZoteroItem(uri) if !reveal => {
                Target::ZoteroItem(ibis_engine::zotero_open::pdf_uri(&uri).unwrap_or(uri))
            }
            target => target,
        };
        if OPENS.load(Ordering::SeqCst) != request || owner.state::<PickerGeneration>().current() != presentation {
            return Err(CommandError::Cancelled);
        }
        // Attachment lookup can wait on Zotero. Recheck the immutable source
        // observation before launching anything after that wait.
        let library = owner.state::<Mutex<Library>>();
        let library = library.lock().map_err(|_| failed())?;
        self::target(
            &library,
            item.as_ref(),
            &expected_revision,
            &expected_source_generation,
            zotero_only,
        )?;
        launch(target, reveal)
    })
    .await
    .map_err(|_| failed())??;
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || yield_to_window(&handle, presentation, Takeover::Beside)),
        NativeIssue::Window,
    );
    Ok(())
}

#[derive(Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SourceKind {
    Zotero,
    File,
}

#[derive(Clone, Serialize, schemars::JsonSchema)]
pub(crate) struct SystemIcon {
    mime: String,
    bytes: Vec<u8>,
}

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct SourcePresentation {
    kind: SourceKind,
    name: String,
    icon: Option<SystemIcon>,
    /// The searched library may differ from the selected/default source.
    status_icon: Option<SystemIcon>,
}

/// Resolve the selected source and its system icon, never exposing its path.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn library_source(
    app: AppHandle,
    item: Option<SourceItem>,
    expected_revision: String,
    expected_source_generation: String,
    zotero_only: bool,
    with_status_icon: bool,
    library: State<'_, Mutex<Library>>,
) -> Result<SourcePresentation, CommandError> {
    let (target, status_path) = {
        let library = library.lock().map_err(|_| failed())?;
        let target = target(
            &library,
            item.as_ref(),
            &expected_revision,
            &expected_source_generation,
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
            path.file_name().ok_or_else(failed)?.to_string_lossy().into_owned(),
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
    item: Option<&SourceItem>,
    expected: &str,
    expected_source_generation: &str,
    zotero_only: bool,
) -> Result<Target, CommandError> {
    // Opening the app itself is independent of a library snapshot. In
    // particular it still works on first run or after a failed status read.
    if zotero_only {
        let chosen = [library.requested.as_ref(), library.source.as_ref()]
            .into_iter()
            .flatten()
            .find_map(|source| match source {
                Source::Zotero(chosen) => Some(chosen),
                Source::File(_) | Source::Samples(_) => None,
            });
        return Ok(Target::Zotero(
            chosen
                .map(selected_zotero)
                .transpose()?
                .unwrap_or(ZoteroLibrary::Personal),
        ));
    }
    if library.source_generation.to_string() != expected_source_generation
        || library.file.as_ref().map(|file| file.revision()).unwrap_or_default() != expected
    {
        return Err(CommandError::SnapshotChanged);
    }
    let Some(item) = item else {
        return match library.source.as_ref().or(library.requested.as_ref()) {
            Some(Source::File(path) | Source::Samples(path)) => Ok(Target::File(path.clone())),
            Some(Source::Zotero(chosen)) => Ok(Target::Zotero(selected_zotero(chosen)?)),
            None => Err(failed()),
        };
    };
    let file = library.file.as_ref().ok_or_else(failed)?;
    let snapshot = file.engine.snapshot().map_err(|_| CommandError::SnapshotChanged)?;
    if snapshot.fingerprints().full.as_str() != expected {
        return Err(CommandError::SnapshotChanged);
    }
    let id = match item {
        SourceItem::Item(item) => {
            if !snapshot
                .get(item)
                .is_some_and(|object| matches!(object.object, LibraryObject::Record(_)))
            {
                return Err(CommandError::SnapshotChanged);
            }
            &item.library
        }
        SourceItem::Library { library } => library,
    };
    let info = file.engine.libraries().map_err(|_| failed())?;
    let source = info.libraries.iter().find(|info| &info.id == id).ok_or_else(failed)?;
    if let Some(zotero) = source.zotero {
        return match item {
            SourceItem::Item(item) => Ok(Target::ZoteroItem(zotero_uri(zotero, &item.key)?)),
            SourceItem::Library { .. } => Ok(Target::Zotero(zotero)),
        };
    }
    match library.source.as_ref() {
        Some(Source::File(path) | Source::Samples(path)) => Ok(Target::File(path.clone())),
        _ => Err(failed()),
    }
}

fn selected_zotero(chosen: &ibis_engine::settings::ZoteroLibraries) -> Result<ZoteroLibrary, CommandError> {
    match chosen.libraries().first() {
        Some(ibis_engine::settings::ZoteroLibrary::User) => Ok(ZoteroLibrary::Personal),
        Some(ibis_engine::settings::ZoteroLibrary::Group(id)) if *id > 0 => Ok(ZoteroLibrary::Group(*id)),
        _ => Err(failed()),
    }
}

fn zotero_library_uri(library: ZoteroLibrary) -> Result<String, CommandError> {
    match library {
        ZoteroLibrary::Personal => Ok("zotero://select/library".into()),
        ZoteroLibrary::Group(id) if id > 0 => Ok(format!("zotero://select/groups/{id}")),
        _ => Err(failed()),
    }
}

fn zotero_uri(library: ZoteroLibrary, key: &str) -> Result<String, CommandError> {
    // Native Zotero keys are eight ASCII letters/digits, distinct from
    // citation keys. Refuse separators, query parameters and URI injection.
    if key.len() != 8 || !key.bytes().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return Err(failed());
    }
    Ok(format!("{}/items/{key}", zotero_library_uri(library)?))
}

fn source_file(path: &Path) -> Result<PathBuf, CommandError> {
    let path = path.canonicalize().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CommandError::from(ibis_core::contract::EngineError::because(
                ibis_core::contract::ErrorCode::SourceUnavailable,
                ibis_core::contract::ErrorReason::MissingFile,
            ))
        } else {
            failed()
        }
    })?;
    let supported = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ibis_engine::library_file_extensions().any(|allowed| extension.eq_ignore_ascii_case(allowed))
        });
    if !path.is_file() || !supported {
        return Err(failed());
    }
    Ok(path)
}

fn launch(target: Target, reveal: bool) -> Result<(), CommandError> {
    use crate::system_open;
    match target {
        Target::File(path) => system_open::open(source_file(&path)?.as_os_str(), reveal),
        Target::Zotero(library) => system_open::open(zotero_library_uri(library)?.as_ref(), reveal),
        Target::ZoteroItem(uri) => system_open::open(uri.as_ref(), reveal),
    }
    .map_err(|()| failed())
}

#[cfg(test)]
mod tests {
    use super::super::state::candidate;
    use super::*;

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn tray_fix_problems_targets_the_warning_library_among_multiple_selections()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::library::{engine_library::FileLibrary, zotero_fixture};
        use ibis_engine::{
            Cancellation, Engine,
            settings::{ZoteroLibraries, ZoteroLibrary},
        };
        let server = zotero_fixture::Server::new();
        server.group_warning.store(true, std::sync::atomic::Ordering::Relaxed);
        let chosen: ZoteroLibraries = serde_json::from_value(serde_json::json!(["user", {"group":7}]))?;
        let engine = Engine::with_zotero_port(server.port).map_err(|_| "engine")?;
        engine
            .dispatch(
                ibis_engine::Method::Libraries,
                &serde_json::json!({"action":"open-zotero", "libraries":["personal", {"group":7}]}),
                &Cancellation::default(),
            )
            .map_err(|_| "open Zotero")?;
        let file = FileLibrary::from_engine(engine);
        let library = Library {
            source: Some(Source::Zotero(chosen)),
            file: Some(std::sync::Arc::new(file)),
            ..Library::default()
        };
        let status = crate::library::snapshot_status(&library);
        assert_eq!(status.zotero_libraries, [ZoteroLibrary::User, ZoteroLibrary::Group(7)]);
        assert_eq!(status.warnings, 1);
        let affected = SourceItem::Library {
            library: status.problem_library.ok_or("warning library")?,
        };
        let target = target(
            &library,
            Some(&affected),
            &status.revision,
            &status.source_generation,
            false,
        )?;
        assert_eq!(target, Target::Zotero(ibis_engine::request::ZoteroLibrary::Group(7)));
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn opening_zotero_after_a_failed_switch_keeps_the_requested_group() -> Result<(), Box<dyn std::error::Error>> {
        let mut library = Library {
            source: Some(Source::Zotero(ibis_engine::settings::ZoteroLibrary::Group(7).into())),
            ..Library::default()
        };
        let generation = library.source_generation.to_string();
        library.requested = library.source.take();
        library.source = Some(Source::File("retained.bib".into()));
        let selected = target(&library, None, "", &generation, true)?;
        assert_eq!(selected, Target::Zotero(ZoteroLibrary::Group(7)));
        Ok(())
    }

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
        let generation = library.source_generation.to_string();
        assert_eq!(
            target(&library, None, &revision, &generation, false).ok(),
            Some(Target::File("retained.bib".into()))
        );
        assert!(matches!(
            target(&library, None, "old", &generation, false),
            Err(CommandError::SnapshotChanged)
        ));
        let item = library
            .file
            .as_ref()
            .and_then(|file| file.engine.snapshot().ok())
            .and_then(|snapshot| snapshot.objects().first().map(|object| object.object.id().clone()))
            .unwrap_or_else(|| panic!("invented record"));
        let source = SourceItem::Item(item.clone());
        assert_eq!(
            target(&library, Some(&source), &revision, &generation, false).ok(),
            Some(Target::File("retained.bib".into()))
        );
        assert!(matches!(
            target(&library, Some(&source), "old", &generation, false),
            Err(CommandError::SnapshotChanged)
        ));
        let mut forged = item.clone();
        forged.key = "not-a-record".into();
        assert!(target(&library, Some(&SourceItem::Item(forged)), &revision, &generation, false).is_err());
        library.source = Some(Source::Samples("samples.csljson".into()));
        assert_eq!(
            target(&library, Some(&source), &revision, &generation, false).ok(),
            Some(Target::File("samples.csljson".into()))
        );
        assert_eq!(
            target(&Library::default(), None, "", "", true).ok(),
            Some(Target::Zotero(ZoteroLibrary::Personal))
        );
    }

    #[test]
    fn an_itemless_open_from_empty_a_cannot_open_empty_b_with_the_same_snapshot() {
        use super::super::state::{LoadRequests, begin_open, install};
        let active = Mutex::new(candidate(PathBuf::from("empty-a.bib"), ""));
        let first = super::super::status::status(&active.lock().unwrap_or_else(|_| panic!("library lock")));
        let requests = LoadRequests::default();
        let (request, _, opening) =
            begin_open(&active, &requests, PathBuf::from("empty-b.bib")).unwrap_or_else(|| panic!("open empty B"));
        let second = install(
            &active,
            &requests,
            request,
            Ok(candidate(PathBuf::from("empty-b.bib"), "")),
        )
        .unwrap_or_else(|| panic!("install empty B"));
        let library = active.lock().unwrap_or_else(|_| panic!("library lock"));
        assert_eq!(
            first.revision, second.revision,
            "empty snapshots deliberately share their fingerprint"
        );
        assert!(
            matches!(
                target(&library, None, &first.revision, &first.source_generation, false),
                Err(CommandError::SnapshotChanged)
            ),
            "A's delayed request must refuse instead of opening B"
        );
        assert!(matches!(
            target(&library, None, &opening.revision, &opening.source_generation, false),
            Err(CommandError::SnapshotChanged)
        ));
        assert_eq!(
            target(&library, None, &second.revision, &second.source_generation, false).ok(),
            Some(Target::File("empty-b.bib".into()))
        );
    }

    #[test]
    fn only_existing_library_files_are_opened() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("-invented ; $(touch nope) & 'library'.bib");
        std::fs::write(&path, "@misc{synthetic}")?;
        let executable = directory.path().join("invented.command");
        std::fs::write(&executable, "@misc{synthetic}")?;
        assert!(source_file(&executable).is_err());
        assert!(source_file(directory.path()).is_err());
        std::fs::remove_file(&path)?;
        assert!(source_file(&path).is_err());
        Ok(())
    }
}
