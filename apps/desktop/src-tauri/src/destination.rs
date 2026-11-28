//! The output the picker starts with for the app it was opened from (#1202),
//! or for the website in front in it when it is a browser, and the per-app
//! choices Ibis remembers (decision 0031,
//! docs/decisions/0031-what-ibis-remembers-per-app.md).
//!
//! Paste choices live in `apps.json`, owned by [`ibis_engine::grants`] and
//! shared with every other Ibis process that delivers.

use ibis_engine::app_context::output;
use std::path::Path;

use ibis_core::destination::{AppId, AppPaste, CopyReason, PasteSafety, lookup};
use ibis_engine::grants::Grants;
use ibis_engine::settings;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::library::CommandError;
use crate::preferences::SettingsFile;

pub(crate) mod site;

/// The name both windows show, resolved when read, never saved: the installed
/// Mac bundle's display name, then the destination table, then the original ID.
pub(crate) fn display_name(app: &AppId) -> String {
    #[cfg(target_os = "macos")]
    if app.system == ibis_core::destination::System::Macos
        && let Some(name) = installed_name(&app.id)
    {
        return name;
    }
    if app.system == ibis_core::destination::System::Web {
        // Only ever a site name, whatever the stored ID held.
        return lookup(app).map_or_else(|| app.normalized(), |known| known.name.to_owned());
    }
    lookup(app).map_or_else(|| app.id.clone(), |known| known.name.to_owned())
}

#[cfg(target_os = "macos")]
fn installed_name(id: &str) -> Option<String> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSBundle, NSString};

    let url = NSWorkspace::sharedWorkspace().URLForApplicationWithBundleIdentifier(&NSString::from_str(id))?;
    if let Some(bundle) = NSBundle::bundleWithURL(&url) {
        for key in ["CFBundleDisplayName", "CFBundleName"] {
            if let Some(value) = bundle.objectForInfoDictionaryKey(&NSString::from_str(key))
                && let Some(name) = value.downcast_ref::<NSString>()
                && name.length() > 0
            {
                return Some(name.to_string());
            }
        }
    }
    url.URLByDeletingPathExtension()?
        .lastPathComponent()
        .map(|name| name.to_string())
}

/// What the picker needs to know about the app it was opened from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub(crate) struct PickerDestination {
    /// The app's own name; `None` when no app was captured.
    pub(crate) app: Option<String>,
    /// The website in front in that app, by name, when it is a browser and
    /// the site could be read: formats are remembered for it, while paste
    /// choices stay with the app.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) site: Option<String>,
    /// That website's site name, such as `overleaf.com`, which a format
    /// choice echoes so it is refused once another site is in front.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) site_id: Option<String>,
    /// The captured OS app identifier, used to refuse a save after the target changes.
    pub(crate) app_id: Option<String>,
    /// Opaque app-location fingerprint; no path crosses IPC.
    pub(crate) app_identity: Option<String>,
    /// The engine's answer: the format the next citation uses and which level
    /// chose it (decision 0042). The picker shows it as it is.
    pub(crate) format: ibis_engine::format::FormatChoice,
    /// The app or site whose remembered format reset removes, named by the engine.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) remembered_for: Option<String>,
    /// Your format for this kind, used by the return-to-usual action.
    pub(crate) usual_format: String,
    pub(crate) find_open: bool,
    /// This app needs a delivery choice after an unreadable per-app file; formats use defaults.
    pub(crate) unreadable: bool,
    /// The current paste policy permits an attempt; final delivery checks still apply.
    pub(crate) can_paste: bool,
    /// The separate delivery choice; never inferred from a remembered format.
    pub(crate) paste: Option<AppPaste>,
    /// An unrecognised app has a proven target but no delivery choice yet.
    pub(crate) paste_question: bool,
    /// A proven ordinary target can accept an explicit delivery choice, including recovery.
    pub(crate) can_choose_paste: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) writing: Option<ibis_engine::app_context::WritingView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) context_version: Option<(u32, u32)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) writing_context: Option<String>,
    /// The engine's answer to whether the format palette offers Projects.
    pub(crate) offers_projects: bool,
    /// How many references this app's reference list holds: those cited into
    /// it since Ibis opened. Paste Reference List is offered once there are any.
    pub(crate) cited_here: u32,
}

/// The remembered delivery for the captured app and the program it named.
pub(crate) fn paste_for_capture(
    grants: &Grants,
    captured: &crate::paste::Captured,
) -> Result<Option<AppPaste>, CopyReason> {
    captured
        .target
        .app
        .as_ref()
        .map_or(Ok(None), |id| grants.paste_for(id, captured.app_identity.as_deref()))
}

/// Native delivery facts for the engine's captured context.
pub(crate) fn present(app: &AppHandle, context: &ibis_engine::app_context::ContextView) -> PickerDestination {
    let memory = app.state::<Grants>();
    let editor = app.state::<crate::editor_endpoint::Pending>();
    let last = if editor.current().is_some() {
        None
    } else {
        crate::paste::last_app(app)
    };
    let format = context.format.clone();
    let usual_format = context.usual_format.clone();
    let writing = context.writing.clone();
    let writing_context = context.writing_context.clone();
    let context_version = Some(context.context_version);
    let offers_projects = context.offers_projects;
    let Some((id, name, app_identity)) = last else {
        return PickerDestination {
            app: None,
            site: None,
            site_id: None,
            app_id: None,
            app_identity: None,
            format,
            remembered_for: None,
            usual_format,
            find_open: false,
            unreadable: false,
            can_paste: false,
            paste: None,
            paste_question: false,
            can_choose_paste: false,
            writing,
            context_version,
            writing_context,
            offers_projects,
            cited_here: 0,
        };
    };
    let can_paste = crate::paste::intends_paste(app, context.paste_enabled);
    let policy = memory.paste_for(&id, app_identity.as_deref());
    let can_choose_paste = crate::paste::may_choose_paste(app, context.paste_enabled);
    // Only an app that can answer sees the warning; the others never paste anyway.
    let unreadable = policy.is_err() && can_choose_paste;
    let ordinary = lookup(&id).is_none_or(|entry| entry.safety == PasteSafety::Normal);
    let paste = ordinary.then(|| policy.ok().flatten()).flatten();
    let paste_question = lookup(&id).is_none() && paste.is_none() && can_choose_paste;
    // The rows the list would paste: those of the open library, as `session_references` gives them.
    let cited_here = {
        use crate::library::engine_library::FileLibrary;
        let items = app
            .state::<crate::session_references::SessionReferences>()
            .lock()
            .cited_here(&id.normalized());
        app.state::<ibis_engine::library::Owner>()
            .current()
            .and_then(|file| file.session_rows(&items).ok())
            .map_or(0, |rows| rows.len() as u32)
    };
    let front_site = site::current(app, &id);
    let remembered_for = context.remembered_for.as_ref().map(|owner| {
        if owner.system == ibis_core::destination::System::Web {
            owner.normalized()
        } else {
            name.clone()
        }
    });
    PickerDestination {
        app: Some(name),
        site: front_site.as_ref().map(display_name),
        site_id: front_site.map(|site| site.id),
        app_id: Some(id.normalized()),
        app_identity,
        format,
        remembered_for,
        usual_format,
        find_open: ibis_core::destination::finds_in_app(&id),
        unreadable,
        can_paste,
        paste,
        paste_question,
        can_choose_paste,
        writing,
        context_version,
        writing_context,
        offers_projects,
        cited_here,
    }
}

/// Remember a format, a delivery choice or both for the captured app; the
/// format goes to the website in front instead when one was read. Whichever
/// is absent keeps its saved value; only a delivery choice revokes outstanding
/// proofs. The format goes to the settings file first, then the delivery choice
/// to the per-app file; a delivery that fails after its format saved says so.
// Tauri commands take their IPC arguments one by one.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn remember_app_output<R: tauri::Runtime>(
    app: AppHandle<R>,
    memory: State<'_, Grants>,
    settings_file: State<'_, SettingsFile>,
    app_id: String,
    output: Option<String>,
    paste: Option<AppPaste>,
    identity: Option<String>,
    site: Option<String>,
) -> Result<(), AppChoiceError> {
    // Only an id the engine lists is remembered; anything else changes nothing.
    let output = output
        .map(|id| output::listed(&id).ok_or("this format is not available; choose another"))
        .transpose()
        .map_err(|_| AppChoiceError::not_saved())?;
    if output.is_none() && paste.is_none() {
        return Err(AppChoiceError::not_saved());
    }
    let target = |captured: Option<&crate::paste::Captured>| -> Result<AppId, String> {
        let captured = captured.ok_or("no app to remember a choice for")?;
        let id = captured.target.app.as_ref().ok_or("no app to remember a choice for")?;
        if id.normalized() != app_id || captured.app_identity != identity {
            return Err("the app changed; open the output menu again".into());
        }
        Ok(id.clone())
    };
    let delivery = || {
        if let Some(paste) = paste {
            crate::paste::change_destination(&app, true, |captured| {
                memory
                    .set_choice(&target(captured)?, paste, identity.clone())
                    .map_err(|_| "the choice for this app could not be saved".into())
            })
            .map_err(|_| {
                if output.is_some() {
                    AppChoiceError::DeliveryNotSaved
                } else {
                    AppChoiceError::not_saved()
                }
            })?;
        }
        Ok(())
    };
    let Some(output) = output else { return delivery() };
    settings_file.owner.change_with(
        |settings| {
            crate::paste::change_destination(&app, paste.is_some(), |captured| {
                let id = target(captured)?;
                let front = captured.and_then(|captured| site::of_capture(&app, captured));
                if !site::unchanged(site.as_deref(), front.as_ref()) {
                    return Err("the website changed; open the format menu again".into());
                }
                let owner = ibis_engine::format::remembers_for(Some(&id), front.as_ref()).unwrap_or(id);
                settings.remember_app_output(&owner, output.id().to_owned());
                Ok(())
            })
            .map_err(|_| AppChoiceError::not_saved())
        },
        |saved| {
            saved?;
            delivery()
        },
    )
}

/// Forget only the format the current app or website uses, after checking the
/// captured target.
#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn forget_app_output(
    app: AppHandle,
    settings_file: State<'_, SettingsFile>,
    app_id: String,
    identity: Option<String>,
    site: Option<String>,
) -> Result<(), CommandError> {
    settings_file
        .owner
        .change(|settings| {
            crate::paste::change_destination(&app, false, |captured| {
                let captured = captured.ok_or("no app to forget a format for")?;
                let id = captured.target.app.as_ref().ok_or("no app to forget a format for")?;
                if id.normalized() != app_id || captured.app_identity != identity {
                    return Err("the app changed; open the format menu again".into());
                }
                let front = site::of_capture(&app, captured);
                if !site::unchanged(site.as_deref(), front.as_ref()) {
                    return Err("the website changed; open the format menu again".into());
                }
                ibis_engine::format::forget_at(settings, Some(id), front.as_ref());
                Ok(())
            })
            .map_err(|_| CommandError::SnapshotChanged)
        })
        .map(|_| ())
}

/// Why a choice for an app was not saved, so the picker can say which part
/// was kept without reading the words.
#[derive(Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(tag = "code", rename_all = "snake_case")]
pub(crate) enum AppChoiceError {
    NotSaved {
        problem: ibis_core::contract::Problem,
    },
    /// The format was saved; whether to paste into the app was not.
    DeliveryNotSaved,
}

impl AppChoiceError {
    fn not_saved() -> Self {
        Self::from(CommandError::OutputNotSaved)
    }
}

impl From<settings::SettingsError> for AppChoiceError {
    fn from(error: settings::SettingsError) -> Self {
        Self::from(CommandError::from(error))
    }
}

impl std::error::Error for AppChoiceError {}
impl From<CommandError> for AppChoiceError {
    fn from(error: CommandError) -> Self {
        Self::NotSaved {
            problem: error.problem(),
        }
    }
}

impl std::fmt::Display for AppChoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSaved { problem } => f.write_str(&ibis_engine::catalogue::problem(
                problem,
                ibis_engine::catalogue::Audience::App,
            )),
            Self::DeliveryNotSaved => f.write_str("the format was saved, but the paste choice could not be"),
        }
    }
}

#[cfg(test)]
fn test_identity(app: &AppId) -> Option<String> {
    let path = if app.system == ibis_core::destination::System::Windows {
        app.id.as_str()
    } else {
        "/invented/Writer.app"
    };
    crate::paste::capture::app_identity(app.system, &app.id, path)
}

/// The per-app file, once the settings location is known.
pub(crate) fn manage(app: &AppHandle, settings: &Path) {
    app.manage(Grants::beside(settings));
    app.manage(site::FrontSite::default());
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::destination::System;
    use ibis_engine::app_context::output::OutputRef;
    use std::{fs, io};

    fn mac(id: &str) -> AppId {
        AppId {
            system: System::Macos,
            id: id.into(),
        }
    }

    fn word() -> AppId {
        mac("com.microsoft.word")
    }

    fn memory_in(directory: &tempfile::TempDir) -> Grants {
        Grants::beside(&directory.path().join("settings.json"))
    }

    /// The remembered delivery for `app` as the test capture names its program.
    fn paste_of(grants: &Grants, app: &AppId) -> Result<Option<AppPaste>, CopyReason> {
        grants.paste_for(app, test_identity(app).as_deref())
    }

    fn listed(id: &str) -> Option<OutputRef> {
        Some(output::listed(id).unwrap_or_else(|| panic!("listed")))
    }

    /// The settings file and the per-app file side by side, as the app keeps them.
    struct Choices {
        directory: tempfile::TempDir,
        app: tauri::App<tauri::test::MockRuntime>,
    }

    impl Choices {
        fn new() -> io::Result<Self> {
            Self::open(tempfile::tempdir()?)
        }

        fn open(directory: tempfile::TempDir) -> io::Result<Self> {
            let app = tauri::test::mock_builder()
                .manage(SettingsFile::new(directory.path().join("settings.json")))
                .manage(memory_in(&directory))
                .manage(crate::paste::LastTarget::default())
                .manage(site::FrontSite::default())
                .build(tauri::test::mock_context(tauri::test::noop_assets()))
                .map_err(io::Error::other)?;
            Ok(Self { directory, app })
        }

        fn choose(&self, app: &AppId, output: Option<&str>, paste: Option<AppPaste>) -> Result<(), CommandError> {
            self.choose_for(app, &app.normalized(), output, paste, test_identity(app))
        }

        fn choose_for(
            &self,
            app: &AppId,
            expected: &str,
            output: Option<&str>,
            paste: Option<AppPaste>,
            identity: Option<String>,
        ) -> Result<(), CommandError> {
            let mut captured = crate::paste::Captured::unknown(app.system);
            captured.target.app = Some(app.clone());
            captured.pid = Some(42);
            captured.app_identity = identity.clone();
            self.app.state::<crate::paste::LastTarget>().capture_fixture(captured);
            tauri::async_runtime::block_on(remember_app_output(
                self.app.handle().clone(),
                self.app.state(),
                self.app.state(),
                expected.into(),
                output.map(str::to_owned),
                paste,
                identity,
                self.app
                    .state::<crate::paste::LastTarget>()
                    .with_captured(|captured| site::of_capture(self.app.handle(), captured?))
                    .flatten()
                    .map(|site| site.id),
            ))
        }

        /// The format a restart reads for `app`.
        fn format(&self, app: &AppId) -> Option<OutputRef> {
            settings::load(&self.directory.path().join("settings.json"))
                .cites()
                .unwrap_or_else(|error| panic!("{error}"))
                .clone()
                .app_output(app)
                .and_then(output::listed)
        }

        fn reopened(&self) -> Grants {
            memory_in(&self.directory)
        }
    }

    #[test]
    fn a_format_save_from_a_replacement_program_carries_no_approval() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let app = mac("org.example.writer");
        let first = crate::paste::capture::app_identity(System::Macos, &app.id, "/invented/Writer.app");
        let other = crate::paste::capture::app_identity(System::Macos, &app.id, "/invented/Viewer.app");
        let grants = memory_in(&directory);
        grants.set_choice(&app, AppPaste::Paste, first.clone())?;
        let choices = Choices::open(directory)?;
        choices
            .choose_for(&app, &app.normalized(), Some("pandoc"), None, other.clone())
            .map_err(io::Error::other)?;
        assert_eq!(grants.paste_for(&app, other.as_deref()), Ok(None));
        assert_eq!(grants.paste_for(&app, first.as_deref()), Ok(Some(AppPaste::Paste)));
        let json = std::fs::read_to_string(choices.directory.path().join("apps.json"))?;
        assert!(!json.contains("/invented/"), "only an identity digest is saved");
        Ok(())
    }

    #[test]
    fn paste_choices_and_formats_stay_separate_and_durable() -> io::Result<()> {
        let choices = Choices::new()?;
        let app = mac("org.example.Writer");
        for paste in [AppPaste::Paste, AppPaste::CopyOnly] {
            choices
                .choose(&app, Some("pandoc"), Some(paste))
                .map_err(io::Error::other)?;
            assert_eq!(choices.format(&mac("ORG.EXAMPLE.WRITER")), listed("pandoc"));
            assert_eq!(
                paste_of(&choices.reopened(), &mac("ORG.EXAMPLE.WRITER")),
                Ok(Some(paste))
            );
            // A later format pick preserves delivery; a format alone never grants it.
            choices.choose(&app, Some("cite"), None).map_err(io::Error::other)?;
            assert_eq!(paste_of(&choices.reopened(), &app), Ok(Some(paste)));
            // A delivery answer keeps the format the person chose.
            choices.choose(&app, None, Some(paste)).map_err(io::Error::other)?;
            assert_eq!(choices.format(&app), listed("cite"));
        }
        let other = mac("org.example.other");
        choices.choose(&other, Some("pandoc"), None).map_err(io::Error::other)?;
        assert_eq!(paste_of(&choices.app.state::<Grants>(), &other), Ok(None));
        Ok(())
    }

    #[test]
    fn a_stale_paste_choice_never_grants_approval() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let grants = memory_in(&directory);
        let app = mac("org.example.Writer");
        let choices = Choices::open(directory)?;
        assert!(
            choices
                .choose_for(
                    &app,
                    "org.example.other",
                    None,
                    Some(AppPaste::Paste),
                    test_identity(&app)
                )
                .is_err()
        );
        assert_eq!(paste_of(&grants, &app), Ok(None));
        Ok(())
    }

    #[test]
    fn always_copy_saves_through_the_command_with_read_only_settings() -> Result<(), Box<dyn std::error::Error>> {
        let (directory, path) = crate::settings_fixture::file();
        crate::settings_fixture::edit(&path, |settings| settings.portable.output = Some("ieee".into()))?;
        let before = fs::read(&path)?;
        let mut permissions = fs::metadata(&path)?.permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&path, permissions)?;
        let choices = Choices::open(directory)?;
        let id = mac("example.writer");
        choices.choose(&id, None, Some(AppPaste::CopyOnly))?;
        assert_eq!(paste_of(&choices.reopened(), &id), Ok(Some(AppPaste::CopyOnly)));
        assert_eq!(fs::read(&path)?, before);
        assert!(!path.with_extension("json.bak").exists());
        Ok(())
    }

    #[test]
    fn always_copy_refuses_read_only_app_choices_and_keeps_the_policy_and_permissions()
    -> Result<(), Box<dyn std::error::Error>> {
        let choices = Choices::new()?;
        let id = word();
        choices.choose(&id, Some("ieee"), Some(AppPaste::Paste))?;
        let paths = ["settings.json", "apps.json"].map(|name| choices.directory.path().join(name));
        for path in &paths {
            let mut permissions = fs::metadata(path)?.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(path, permissions)?;
        }
        let before = paths.each_ref().map(|path| {
            (
                fs::read(path).unwrap_or_default(),
                fs::metadata(path).map(|m| m.permissions()),
            )
        });
        assert!(matches!(
            choices.choose(&id, None, Some(AppPaste::CopyOnly)),
            Err(CommandError::Presented(_))
        ));
        assert_eq!(
            choices
                .app
                .state::<Grants>()
                .set_choice(&id, AppPaste::CopyOnly, None)
                .err()
                .map(|error| error.kind()),
            Some(io::ErrorKind::PermissionDenied)
        );
        assert_eq!(paste_of(&choices.app.state::<Grants>(), &id), Ok(Some(AppPaste::Paste)));
        assert_eq!(paste_of(&choices.reopened(), &id), Ok(Some(AppPaste::Paste)));
        for (path, (bytes, permissions)) in paths.iter().zip(before) {
            assert_eq!(fs::read(path)?, bytes);
            assert_eq!(fs::metadata(path)?.permissions(), permissions?);
        }
        Ok(())
    }

    #[test]
    fn a_format_chosen_on_a_website_is_kept_for_the_site_and_paste_for_the_browser() -> io::Result<()> {
        let choices = Choices::new()?;
        let chrome = mac("com.google.Chrome");
        let overleaf = ibis_core::destination::site("https://www.overleaf.com/project/0000")
            .ok_or_else(|| io::Error::other("a site"))?;
        choices
            .app
            .state::<site::FrontSite>()
            .record(Some(ibis_native::site::Front::App {
                app: chrome.clone(),
                pid: 42,
                site: Some(overleaf.clone()),
            }));
        choices
            .choose(&chrome, Some("natbib"), Some(AppPaste::Paste))
            .map_err(|error| io::Error::other(error.to_string()))?;
        assert_eq!(
            choices.format(&overleaf).map(|output| output.id().to_owned()),
            Some("natbib".into())
        );
        assert_eq!(choices.format(&chrome), None);
        assert_eq!(paste_of(&choices.reopened(), &chrome), Ok(Some(AppPaste::Paste)));
        // The settings file holds the site name, never the address.
        let saved = fs::read_to_string(choices.directory.path().join("settings.json"))?;
        assert!(saved.contains("overleaf.com") && !saved.contains("project"));
        // With no site read, the browser is one app again.
        choices.app.state::<site::FrontSite>().record(None);
        choices
            .choose(&chrome, Some("ieee"), None)
            .map_err(|error| io::Error::other(error.to_string()))?;
        assert_eq!(
            choices.format(&chrome).map(|output| output.id().to_owned()),
            Some("ieee".into())
        );
        Ok(())
    }

    #[test]
    fn a_choice_made_on_one_site_is_refused_once_the_browser_shows_another() -> io::Result<()> {
        let choices = Choices::new()?;
        let chrome = mac("com.google.Chrome");
        let site = |address| ibis_core::destination::site(address).ok_or_else(|| io::Error::other("a site"));
        let (overleaf, docs) = (site("https://overleaf.com/")?, site("https://docs.google.com/")?);
        let front = |site: &AppId| {
            choices
                .app
                .state::<site::FrontSite>()
                .record(Some(ibis_native::site::Front::App {
                    app: chrome.clone(),
                    pid: 42,
                    site: Some(site.clone()),
                }));
        };
        front(&overleaf);
        choices
            .choose(&chrome, Some("natbib"), None)
            .map_err(|error| io::Error::other(error.to_string()))?;
        // The picker was reopened over Google Docs before an Overleaf choice ran.
        front(&docs);
        let late = tauri::async_runtime::block_on(remember_app_output(
            choices.app.handle().clone(),
            choices.app.state(),
            choices.app.state(),
            chrome.normalized(),
            Some("ieee".into()),
            None,
            test_identity(&chrome),
            Some("overleaf.com".into()),
        ));
        assert!(late.is_err());
        assert_eq!(choices.format(&docs), None);
        assert_eq!(
            choices.format(&overleaf).map(|output| output.id().to_owned()),
            Some("natbib".into())
        );
        Ok(())
    }
}
