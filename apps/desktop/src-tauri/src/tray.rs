//! The native menu bar (macOS) or tray (Windows, Linux) menu.

use std::path::PathBuf;
use std::sync::Mutex;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use crate::diagnostics::{NativeIssue, report};
use crate::feedback::{idea_url, open_in_browser, problem_url};
use crate::library::{
    Library, LibraryState, LibraryStatus, LoadError, Source, choose_library_file, start_open, use_zotero_library,
};
use crate::picker::show_picker;
use crate::updater::{self, UpdateMenuItem};

fn words(id: &str, args: &[(&str, &str)]) -> String {
    ibis_engine::catalogue::text(id, None, args).unwrap_or_else(|| id.to_owned())
}

/// Each source is a native identity, never a filename used to reopen a file.
/// Keep the last real file available when the person switches to Zotero.
#[derive(Default)]
struct LibraryChoices {
    zotero_detected: bool,
    file: Option<PathBuf>,
    current: Option<Source>,
    requested: Option<Source>,
}

impl LibraryChoices {
    fn observe(&mut self, requested: Option<&Source>, current: Option<&Source>) {
        self.zotero_detected |= [requested, current]
            .into_iter()
            .flatten()
            .any(|s| matches!(s, Source::Zotero(_)));
        if let Some(Source::File(path)) = current.filter(|s| matches!(s, Source::File(_))).or(requested) {
            self.file = Some(path.clone());
        }
        self.current = current.cloned();
        self.requested = requested.cloned();
    }

    fn checked(&self, row: Row) -> bool {
        match row {
            Row::Zotero => matches!(self.current, Some(Source::Zotero(_))),
            Row::File => self
                .file
                .as_ref()
                .is_some_and(|path| self.current == Some(Source::File(path.clone()))),
            _ => false,
        }
    }

    fn target(&self, row: Row) -> Option<Source> {
        match row {
            Row::File => self.file.clone().map(Source::File),
            // A failed switch must retry the attempted source, not its retained snapshot.
            Row::Retry => self.requested.clone().or_else(|| self.current.clone()),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Open,
    Separator(u8),
    Library,
    Choose,
    Zotero,
    File,
    Retry,
    Settings,
    Update,
    Help,
    Quit,
    Shortcuts,
    WhatsNew,
    Problem,
    Idea,
    Reset,
    Version,
    Libraries,
}

impl Row {
    fn label(self, status: &LibraryStatus, choices: &LibraryChoices, name: &str) -> String {
        let id = match self {
            Self::Open => "app-tray-open",
            Self::Library => return library_line(status),
            Self::Choose => "app-tray-choose",
            Self::Zotero => "app-tray-zotero",
            Self::File => {
                return choices
                    .file
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
            Self::Retry => "app-tray-retry",
            Self::Settings => "app-tray-settings",
            Self::Update => "app-tray-update",
            Self::Help => "app-tray-help",
            Self::Quit => return words("app-tray-quit", &[("name", name)]),
            Self::Shortcuts => "app-tray-shortcuts",
            Self::WhatsNew => "app-tray-whats-new",
            Self::Problem => "app-tray-problem",
            Self::Idea => "app-tray-idea",
            Self::Reset => "app-tray-reset",
            Self::Libraries => "app-tray-library-settings",
            Self::Version => return name.to_owned(),
            Self::Separator(_) => return "—".into(),
        };
        words(id, &[])
    }

    fn id(self) -> String {
        format!("tray-{self:?}")
    }
}

fn library_name(status: &LibraryStatus) -> String {
    match status.name.as_deref() {
        Some("Zotero library") => words("app-tray-zotero", &[]),
        Some(name) => name.to_owned(),
        None => words("app-tray-library", &[]),
    }
}

/// A problem has one next action, chosen from codes rather than its wording.
fn problem(status: &LibraryStatus) -> Option<(String, Row)> {
    use ibis_core::contract::{ErrorCode, ErrorReason};
    if matches!(status.state, LibraryState::Loading | LibraryState::Reloading) || status.error.is_none() {
        return None;
    }
    let name = library_name(status);
    let (id, fix) = match status.error_reason {
        Some(LoadError::Zotero(error)) if error.code == ErrorCode::NewerFormat => {
            ("app-tray-zotero-newer", Row::Update)
        }
        Some(LoadError::File(error) | LoadError::FileRefresh(error)) if error.code == ErrorCode::SourceUnavailable => {
            ("app-tray-file-missing", Row::Choose)
        }
        Some(LoadError::Zotero(error)) if error.reason == Some(ErrorReason::ZoteroSharingOff) => {
            ("app-tray-zotero-sharing", Row::Libraries)
        }
        Some(LoadError::Zotero(error)) if error.reason == Some(ErrorReason::ZoteroProfileNeeded) => {
            ("app-tray-zotero-profile", Row::Libraries)
        }
        Some(LoadError::Zotero(error)) if error.reason == Some(ErrorReason::ZoteroNotFound) => {
            ("app-tray-zotero-missing", Row::Choose)
        }
        Some(LoadError::Zotero(error)) if error.reason == Some(ErrorReason::ZoteroNeedsOpening) => {
            ("app-tray-zotero-closed", Row::Retry)
        }
        Some(LoadError::Zotero(error)) if error.reason == Some(ErrorReason::ZoteroNoSpace) => {
            ("app-tray-zotero-no-space", Row::Retry)
        }
        Some(LoadError::Zotero(error)) if error.code == ErrorCode::NotFound => {
            ("app-tray-zotero-group-missing", Row::Libraries)
        }
        Some(LoadError::Zotero(error)) if error.code == ErrorCode::ResourceLimit => {
            ("app-tray-zotero-work-limit", Row::Retry)
        }
        Some(LoadError::FileRefresh(_)) => ("app-tray-not-updated", Row::Retry),
        Some(LoadError::File(error)) if error.reason == Some(ErrorReason::UnknownFormat) => {
            ("app-tray-file-format", Row::Choose)
        }
        Some(LoadError::File(error)) if error.reason == Some(ErrorReason::CsvTemplate) => {
            ("app-tray-file-csv", Row::Choose)
        }
        Some(LoadError::File(error)) if error.code == ErrorCode::ResourceLimit => {
            ("app-tray-file-too-large", Row::Choose)
        }
        Some(LoadError::File(error)) if error.code == ErrorCode::BadInput => ("app-tray-file-invalid", Row::Choose),
        Some(LoadError::File(_) | LoadError::NotLocal) => ("app-tray-file-unreadable", Row::Choose),
        _ if status.stale && status.source_is_requested => ("app-tray-not-updated", Row::Retry),
        _ => ("app-tray-not-opened", Row::Retry),
    };
    Some((words(id, &[("name", &name)]), fix))
}

/// One short, localized line. Native menus cannot wrap long filenames.
pub(crate) fn library_line(status: &LibraryStatus) -> String {
    let name = library_name(status);
    let line = match status.state {
        LibraryState::Loading => words("app-tray-opening", &[("name", &name)]),
        LibraryState::Reloading => words("app-tray-reading", &[("name", &name)]),
        _ => match problem(status) {
            Some((line, _)) => line,
            None if status.name.is_none() => words("app-tray-no-library", &[]),
            None => words(
                if status.references == 1 {
                    "app-tray-count-one"
                } else {
                    "app-tray-count"
                },
                &[("name", &name), ("count", &status.references.to_string())],
            ),
        },
    };
    if line.chars().count() > 90 {
        format!("{}…", line.chars().take(89).collect::<String>())
    } else {
        line
    }
}

fn menu_rows(status: &LibraryStatus, choices: &LibraryChoices) -> Vec<Row> {
    let fix = problem(status).map(|(_, fix)| fix);
    let mut rows = vec![Row::Open, Row::Separator(0), Row::Library];
    rows.extend(fix);
    if choices.zotero_detected {
        rows.push(Row::Zotero);
    }
    if choices.file.is_some() {
        rows.push(Row::File);
    }
    if fix != Some(Row::Choose) {
        rows.push(Row::Choose);
    }
    rows.extend([Row::Separator(1), Row::Settings]);
    if fix != Some(Row::Update) {
        rows.push(Row::Update);
    }
    rows.extend([Row::Help, Row::Separator(2), Row::Quit]);
    rows
}

fn help_rows() -> Vec<Row> {
    vec![
        Row::Shortcuts,
        Row::WhatsNew,
        Row::Separator(3),
        Row::Problem,
        Row::Idea,
        Row::Separator(4),
        Row::Reset,
        Row::Separator(5),
        Row::Version,
    ]
}

fn version_line(name: &str, version: &str) -> String {
    format!("{name} {version}")
}

pub(crate) struct OpenMenuItem(MenuItem<tauri::Wry>);

/// Tauri accepts physical accelerator codes, whose Mac equivalents are US
/// characters. Resolve the registered key through the current layout first.
#[cfg(any(target_os = "macos", test))]
fn macos_shortcut(
    shortcut: tauri_plugin_global_shortcut::Shortcut,
    character: Option<char>,
) -> (String, Option<String>) {
    use tauri_plugin_global_shortcut::{Modifiers, Shortcut};
    let raw = shortcut.to_string();
    let key = character
        .map(|key| key.to_string())
        .unwrap_or_else(|| shortcut.key.to_string());
    let mut parts = Vec::new();
    for (modifier, name) in [
        (Modifiers::CONTROL, "Control"),
        (Modifiers::ALT, "Option"),
        (Modifiers::SHIFT, "Shift"),
        (Modifiers::SUPER, "Command"),
    ] {
        if shortcut.mods.contains(modifier) {
            parts.push(name.to_owned());
        }
    }
    parts.push(key);
    let label = parts.join("+");
    let equivalent = if character.is_some_and(|key| !key.is_ascii())
        || character.is_none() && crate::picker::us_character(shortcut).is_some()
    {
        None // An unresolved printable key must not advertise its US character.
    } else {
        let prefix = raw.rsplit_once('+').map_or("", |(prefix, _)| prefix);
        let key = character.map_or_else(|| shortcut.key.to_string(), |key| key.to_string());
        let equivalent = if prefix.is_empty() {
            key
        } else {
            format!("{prefix}+{key}")
        };
        equivalent.parse::<Shortcut>().ok().map(|_| equivalent)
    };
    (label, equivalent)
}

/// Read fresh state on the main thread, including after a keyboard-layout change.
pub(crate) fn show_shortcut<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            let Some(item) = handle.try_state::<OpenMenuItem>() else {
                return;
            };
            let shortcut = handle
                .state::<Mutex<crate::picker::ShortcutStatus>>()
                .lock()
                .ok()
                .filter(|status| status.registered)
                .map(|status| status.shortcut.clone());
            let mut label = words("app-tray-open", &[]);
            #[cfg(target_os = "macos")]
            let accelerator = shortcut
                .as_ref()
                .and_then(|_| crate::picker::registered_shortcut(&handle))
                .and_then(|key| {
                    use tauri_plugin_global_shortcut::{Modifiers, Shortcut};
                    let unshifted = Shortcut::new(Some(key.mods.difference(Modifiers::SHIFT)), key.key);
                    let (shortcut, equivalent) =
                        macos_shortcut(key, crate::picker::character(unshifted, crate::picker::typing()));
                    if equivalent.is_none() {
                        label = words("app-tray-open-shortcut", &[("shortcut", &shortcut)]);
                    }
                    equivalent
                });
            #[cfg(not(target_os = "macos"))]
            let accelerator = {
                if let Some(shortcut) = &shortcut
                    && shortcut.parse::<tauri_plugin_global_shortcut::Shortcut>().is_err()
                {
                    label = words("app-tray-open-shortcut", &[("shortcut", shortcut)]);
                }
                shortcut
            };
            report(item.0.set_text(label.replace('&', "&&")), NativeIssue::Shortcut);
            report(item.0.set_accelerator(accelerator), NativeIssue::Shortcut);
        }),
        NativeIssue::Shortcut,
    );
}

#[cfg(target_os = "macos")]
static HIDDEN: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
static APPLIED: AtomicU64 = AtomicU64::new(0);

/// Off, the shortcut still opens the picker and opening Ibis opens Settings.
/// Saves can finish out of order; only the main thread applies a revision.
#[cfg(target_os = "macos")]
pub(crate) fn show_in_menu_bar<R: Runtime>(app: &AppHandle<R>, shown: bool, revision: u64) {
    let handle = app.clone();
    report(
        app.run_on_main_thread(move || {
            if revision < APPLIED.load(Ordering::SeqCst) {
                return;
            }
            APPLIED.store(revision, Ordering::SeqCst);
            HIDDEN.store(!shown, Ordering::SeqCst);
            if let Some(tray) = handle.tray_by_id("ibis") {
                report(tray.set_visible(shown), NativeIssue::TrayCreation);
                if shown {
                    crate::menu_bar::watch(&tray);
                }
            }
        }),
        NativeIssue::TrayCreation,
    );
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn show_in_menu_bar<R: Runtime>(_: &AppHandle<R>, _: bool, _: u64) {}

#[cfg(target_os = "macos")]
pub(crate) fn hidden() -> bool {
    HIDDEN.load(Ordering::SeqCst)
}

/// Text from Search with Ibis waiting for the picker: a launch may beat it.
#[derive(Default)]
pub(crate) struct SearchRequest(pub(crate) Mutex<Option<String>>);

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn take_search_request(request: tauri::State<'_, SearchRequest>) -> Option<String> {
    request.0.lock().ok().and_then(|mut waiting| waiting.take())
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) fn alignment_tick() {
    #[cfg(target_os = "macos")]
    crate::menu_bar::tick();
}

struct NativeRow {
    row: Row,
    item: MenuItemKind<tauri::Wry>,
    // Replacing a file/retry target creates a new native ID. A delayed event
    // from the retired item cannot open a newer source.
    target: Option<Source>,
}

struct MenuContents {
    choices: LibraryChoices,
    rows: Vec<NativeRow>,
}

struct TrayMenu {
    menu: Menu<tauri::Wry>,
    contents: Mutex<MenuContents>,
}

fn library_target<'a>(
    id: &tauri::menu::MenuId,
    targets: impl IntoIterator<Item = (&'a tauri::menu::MenuId, &'a Source)>,
) -> Option<Source> {
    targets
        .into_iter()
        .find(|(target_id, _)| *target_id == id)
        .map(|(_, source)| source.clone())
}

fn native_row(app: &AppHandle, row: Row, status: &LibraryStatus, choices: &LibraryChoices) -> tauri::Result<NativeRow> {
    let name = &app.package_info().name;
    let label = row.label(status, choices, name).replace('&', "&&");
    let target = choices.target(row);
    let item = match row {
        Row::Separator(_) => PredefinedMenuItem::separator(app)?.kind(),
        Row::Zotero | Row::File => {
            if row == Row::File {
                CheckMenuItem::new(app, label, true, choices.checked(row), None::<&str>)?.kind()
            } else {
                CheckMenuItem::with_id(app, row.id(), label, true, choices.checked(row), None::<&str>)?.kind()
            }
        }
        Row::Retry => MenuItem::new(app, label, true, None::<&str>)?.kind(),
        Row::Help => {
            let help = Submenu::with_id(app, row.id(), label, true)?;
            for row in help_rows() {
                help.append(&native_row(app, row, status, choices)?.item)?;
            }
            help.kind()
        }
        _ => {
            let label = if row == Row::Version {
                version_line(name, &app.package_info().version.to_string())
            } else {
                label
            };
            let accelerator = match row {
                Row::Settings => Some("CmdOrCtrl+,"),
                Row::Quit => Some("CmdOrCtrl+Q"),
                _ => None,
            };
            MenuItem::with_id(app, row.id(), label, row != Row::Library, accelerator)?.kind()
        }
    };
    Ok(NativeRow { row, item, target })
}

impl TrayMenu {
    /// Preserve native objects; only move rows when the available actions change.
    fn refresh(&self, app: &AppHandle) -> tauri::Result<()> {
        let (status, requested, current) = {
            let library = app.state::<Mutex<Library>>();
            let Ok(library) = library.lock() else { return Ok(()) };
            (
                crate::library::snapshot_status(&library),
                library.requested.clone(),
                library.source.clone(),
            )
        };
        let Ok(mut contents) = self.contents.lock() else {
            return Ok(());
        };
        contents.choices.observe(requested.as_ref(), current.as_ref());
        let wanted = menu_rows(&status, &contents.choices);
        let mut index = 0;
        while index < contents.rows.len() {
            let row = &contents.rows[index];
            if !wanted.contains(&row.row) || row.target != contents.choices.target(row.row) {
                self.menu.remove(&row.item)?;
                contents.rows.remove(index);
            } else {
                index += 1;
            }
        }
        for (index, row) in wanted.into_iter().enumerate() {
            match contents.rows.iter().position(|entry| entry.row == row) {
                Some(old) if old != index => {
                    let entry = &contents.rows[old];
                    self.menu.remove(&entry.item)?;
                    self.menu.insert(&entry.item, index)?;
                    let entry = contents.rows.remove(old);
                    contents.rows.insert(index, entry);
                }
                None => {
                    let entry = native_row(app, row, &status, &contents.choices)?;
                    self.menu.insert(&entry.item, index)?;
                    contents.rows.insert(index, entry);
                }
                _ => {}
            }
        }
        for entry in &contents.rows {
            match &entry.item {
                MenuItemKind::MenuItem(item) if entry.row == Row::Library => {
                    item.set_text(library_line(&status).replace('&', "&&"))?
                }
                MenuItemKind::Check(item) => item.set_checked(contents.choices.checked(entry.row))?,
                _ => {}
            }
        }
        #[cfg(target_os = "macos")]
        crate::menu_bar::show_reading(app, status.state == LibraryState::Reloading);
        Ok(())
    }

    fn target(&self, id: &tauri::menu::MenuId) -> Option<Source> {
        let contents = self.contents.lock().ok()?;
        library_target(
            id,
            contents
                .rows
                .iter()
                .filter_map(|entry| entry.target.as_ref().map(|source| (entry.item.id(), source))),
        )
    }
}

/// Called on the main thread, reading fresh state after queued library changes.
pub(crate) fn refresh_library(app: &AppHandle) {
    if let Some(menu) = app.try_state::<TrayMenu>() {
        report(menu.refresh(app), NativeIssue::TrayLibrary);
    }
}

fn menu_on_left_click(macos: bool) -> bool {
    macos
}

fn handle_tray_event(macos: bool, event: &TrayIconEvent, open: impl FnOnce()) {
    if !macos
        && matches!(
            event,
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
        )
    {
        open();
    }
}

fn menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    // A radio click has a fixed native target; never read its mutable check.
    if let Some(source) = app.try_state::<TrayMenu>().and_then(|menu| menu.target(&event.id)) {
        start_open(app, source, true);
        crate::picker::reveal_picker(app);
    } else if event.id == Row::Open.id() {
        show_picker(app);
    } else if event.id == Row::Choose.id() {
        choose_library_file(app);
    } else if event.id == Row::Zotero.id() {
        use_zotero_library(app);
    } else if event.id == Row::Settings.id() {
        crate::settings_window::open(app, None);
    } else if event.id == Row::Shortcuts.id() {
        crate::settings_window::open(app, Some(crate::settings_window::Page::Shortcuts));
    } else if event.id == Row::Libraries.id() {
        crate::settings_window::open(app, Some(crate::settings_window::Page::Libraries));
    } else if event.id == Row::WhatsNew.id() {
        open_in_browser(app, "https://github.com/ibiscite/ibis/releases".into());
    } else if event.id == Row::Problem.id() {
        open_in_browser(app, problem_url(&app.package_info().version.to_string()));
    } else if event.id == Row::Idea.id() {
        open_in_browser(app, idea_url());
    } else if event.id == Row::Reset.id() {
        crate::sizing::reset_position(app);
    } else if event.id == Row::Update.id() {
        updater::setting::check_from_menu(app);
    } else if event.id == Row::Version.id() {
        if app
            .clipboard()
            .write_text(version_line(
                &app.package_info().name,
                &app.package_info().version.to_string(),
            ))
            .is_err()
        {
            app.dialog()
                .message(words("app-tray-version-copy-failed", &[]))
                .title(&app.package_info().name)
                .show(|_| {});
        }
    } else if event.id == Row::Quit.id() {
        app.exit(0);
    }
    refresh_library(app);
}

pub(crate) fn create_tray(app: &AppHandle, shortcut: Option<&str>) -> tauri::Result<()> {
    let status = crate::library::current_status(app);
    let choices = LibraryChoices::default();
    let menu = Menu::new(app)?;
    let mut rows = Vec::new();
    for row in menu_rows(&status, &choices) {
        let entry = native_row(app, row, &status, &choices)?;
        if let MenuItemKind::MenuItem(item) = &entry.item {
            match row {
                Row::Open => {
                    item.set_accelerator(shortcut)?;
                    app.manage(OpenMenuItem(item.clone()));
                }
                Row::Update => {
                    app.manage(UpdateMenuItem(item.clone()));
                }
                _ => {}
            }
        }
        menu.append(&entry.item)?;
        rows.push(entry);
    }
    app.manage(TrayMenu {
        menu: menu.clone(),
        contents: Mutex::new(MenuContents { choices, rows }),
    });
    refresh_library(app);
    let mut tray = TrayIconBuilder::with_id("ibis")
        .tooltip(&app.package_info().name)
        .menu(&menu)
        .show_menu_on_left_click(menu_on_left_click(cfg!(target_os = "macos")))
        .on_menu_event(menu_event)
        .on_tray_icon_event(|tray, event| {
            handle_tray_event(cfg!(target_os = "macos"), &event, || show_picker(tray.app_handle()))
        });
    #[cfg(target_os = "macos")]
    {
        tray = tray.icon(crate::menu_bar::BIRD).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    let _tray = tray.build(app)?;
    #[cfg(target_os = "macos")]
    crate::menu_bar::accept_drops(app, &_tray);
    // Local profile discovery runs once, off the main thread; nothing polls.
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let detected = crate::library::zotero_detected();
        let app = handle.clone();
        report(
            handle.run_on_main_thread(move || {
                if let Some(menu) = app.try_state::<TrayMenu>()
                    && let Ok(mut contents) = menu.contents.lock()
                {
                    contents.choices.zotero_detected |= detected;
                }
                refresh_library(&app);
            }),
            NativeIssue::TrayLibrary,
        );
    });
    Ok(())
}

#[cfg(test)]
mod tests;
