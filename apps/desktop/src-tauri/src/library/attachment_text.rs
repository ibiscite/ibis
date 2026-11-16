//! Settings may grant attachment text only to an open, previously listed library.
use std::sync::Mutex;

use ibis_core::contract::{ErrorCode, LibraryId};
use ibis_engine::{
    Engine,
    request::ZoteroLibrary,
    settings::{Owner as SettingsOwner, Pending},
};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::{CommandError, Source, engine_library::FileLibrary};
use crate::preferences::SettingsFile;
use ibis_engine::library::{Current, Owner};

#[derive(Serialize, schemars::JsonSchema)]
pub(crate) struct AttachmentTextLibrary {
    id: LibraryId,
    name: String,
    on: bool,
}

#[derive(Default)]
pub(crate) struct AttachmentText(Mutex<Vec<LibraryId>>);

impl AttachmentText {
    fn list(
        &self,
        file: Option<&Current>,
        source: Option<&Source>,
        owner: &SettingsOwner,
    ) -> Result<Vec<AttachmentTextLibrary>, CommandError> {
        let libraries = file
            .map(|file| file.engine.libraries())
            .transpose()?
            .map(|list| list.libraries)
            .unwrap_or_default();
        let groups = if libraries
            .iter()
            .any(|library| matches!(library.zotero, Some(ZoteroLibrary::Group(_))))
        {
            file.and_then(|file| file.zotero_groups().ok()).unwrap_or_default()
        } else {
            Vec::new()
        };
        let loaded = owner.read();
        let settings = loaded.cites().map_err(CommandError::from)?;
        let rows: Vec<_> = libraries
            .into_iter()
            .map(|library| {
                let name = match library.zotero {
                    Some(ZoteroLibrary::User) => "Zotero library".into(),
                    Some(ZoteroLibrary::Group(id)) => groups
                        .iter()
                        .find(|group| group.id == id)
                        .map(|group| group.name.clone())
                        .unwrap_or_else(|| format!("Zotero group {id}")),
                    None => source.and_then(Source::name).unwrap_or_else(|| "Library".into()),
                };
                AttachmentTextLibrary {
                    on: settings.attachment_text(&library.id),
                    id: library.id,
                    name,
                }
            })
            .collect();
        *self.0.lock().map_err(|_| CommandError::SettingsUnavailable)? =
            rows.iter().map(|row| row.id.clone()).collect();
        Ok(rows)
    }

    fn set(
        &self,
        engine: Option<&Engine>,
        owner: &SettingsOwner,
        id: LibraryId,
        on: bool,
    ) -> Result<Pending<Result<bool, CommandError>>, CommandError> {
        let offered = self
            .0
            .lock()
            .map_err(|_| CommandError::SettingsUnavailable)?
            .contains(&id);
        let open = engine
            .map(Engine::libraries)
            .transpose()?
            .is_some_and(|list| list.libraries.iter().any(|library| library.id == id));
        if !offered || !open {
            return Err(CommandError::Engine(ErrorCode::BadInput.into()));
        }
        // Accept the intent before waiting off-thread, preserving command order.
        Ok(owner
            .submit_with_receipt(
                move |settings| {
                    settings.device.attachment_text.retain(|library| library != &id);
                    if on {
                        settings.device.attachment_text.push(id);
                    }
                    Ok::<_, CommandError>(())
                },
                move |_, result| result.map(|_| on),
            )
            .1)
    }
}

/// Open libraries by display name and complete opaque ID, with their saved choice.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn attachment_text_libraries(app: AppHandle) -> Result<Vec<AttachmentTextLibrary>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let view = app.state::<Owner>().view();
        let (file, source) = (view.current.clone(), view.source().cloned());
        app.state::<AttachmentText>()
            .list(file.as_deref(), source.as_ref(), &app.state::<SettingsFile>().owner)
    })
    .await
    .map_err(|_| CommandError::SettingsUnavailable)?
}

/// Save a choice for an open library previously returned to Settings.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn set_attachment_text(app: AppHandle, id: LibraryId, on: bool) -> Result<bool, CommandError> {
    let file = app.state::<Owner>().current();
    let pending = app.state::<AttachmentText>().set(
        file.as_ref().map(|file| &file.engine),
        &app.state::<SettingsFile>().owner,
        id,
        on,
    )?;
    tauri::async_runtime::spawn_blocking(move || pending.wait().ok_or(CommandError::SettingsUnavailable)?)
        .await
        .map_err(|_| CommandError::SettingsUnavailable)?
}

/// Whether attachment text tools may read files beyond the library's folder.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn attachment_text_outside_library(app: AppHandle) -> Result<bool, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let loaded = app.state::<SettingsFile>().owner.read();
        Ok(loaded.cites()?.device.attachment_text_outside_library)
    })
    .await
    .map_err(|_| CommandError::SettingsUnavailable)?
}

/// Save this computer's permission to read attachment text outside libraries.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn set_attachment_text_outside_library(app: AppHandle, on: bool) -> Result<bool, CommandError> {
    let pending = app
        .state::<SettingsFile>()
        .owner
        .submit_with_receipt(
            move |settings| {
                settings.device.attachment_text_outside_library = on;
                Ok::<_, CommandError>(())
            },
            move |_, result| result.map(|_| on),
        )
        .1;
    tauri::async_runtime::spawn_blocking(move || pending.wait().ok_or(CommandError::SettingsUnavailable)?)
        .await
        .map_err(|_| CommandError::SettingsUnavailable)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_text_requires_a_listed_open_id_and_saves_the_shared_setting() {
        let (directory, path) = crate::settings_fixture::file();
        let library_path = directory.path().join("invented.bib");
        std::fs::write(&library_path, "@book{invented, title={Invented}}").unwrap();
        let file = ibis_engine::library::Current::open(
            ibis_engine::Engine::new()
                .unwrap_or_else(|_| panic!("engine"))
                .with_formatting(crate::library::engine_library::FORMATTING.clone()),
            ibis_engine::library::Source::File(library_path.clone()),
            None,
            &ibis_engine::Cancellation::default(),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let owner = SettingsOwner::open(path.clone());
        let choices = AttachmentText::default();
        let id = file.engine.libraries().unwrap().libraries[0].id.clone();
        assert!(choices.set(Some(&file.engine), &owner, id.clone(), true).is_err());
        let rows = choices
            .list(Some(&file), Some(&Source::File(library_path)), &owner)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "invented.bib");
        assert_eq!(rows[0].id, id);
        assert!(!rows[0].on);
        let other = LibraryId::new("lib_unlisted_library").unwrap();
        assert!(choices.set(Some(&file.engine), &owner, other, true).is_err());
        assert!(!path.exists(), "refused grants must not write settings");
        assert!(
            choices
                .set(Some(&file.engine), &owner, id.clone(), true)
                .unwrap()
                .wait()
                .unwrap()
                .unwrap()
        );
        assert_eq!(
            crate::settings_fixture::read(&path).device.attachment_text,
            vec![id.clone()]
        );
        assert!(choices.list(Some(&file), None, &owner).unwrap()[0].on);
        assert!(choices.set(None, &owner, id.clone(), false).is_err());
        assert!(
            file.engine
                .attachment_text_allowed(&crate::settings_fixture::read(&path), &id)
                .unwrap()
        );
        assert!(
            !choices
                .set(Some(&file.engine), &owner, id.clone(), false)
                .unwrap()
                .wait()
                .unwrap()
                .unwrap()
        );
        assert!(crate::settings_fixture::read(&path).device.attachment_text.is_empty());
        assert!(
            !file
                .engine
                .attachment_text_allowed(&crate::settings_fixture::read(&path), &id)
                .unwrap()
        );
        assert!(choices.list(None, None, &owner).unwrap().is_empty());
    }
}
