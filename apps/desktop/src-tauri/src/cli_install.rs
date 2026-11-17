//! Fixed native command locations; Settings never supplies a path or executable.
use ibis_core::command_install::{CommandInstall, CommandStatus, PROFILE_LINE};
use tauri::Manager;

fn installer(app: &tauri::AppHandle) -> Result<CommandInstall, String> {
    let home = app.path().home_dir().map_err(|_| "Could not find your home folder.")?;
    let exe = std::env::current_exe().map_err(|_| "Could not find the Ibis app.")?;
    let directory = exe.parent().ok_or("Could not find the Ibis app.")?;
    let bundled = if cfg!(target_os = "linux") {
        app.path()
            .resource_dir()
            .map_err(|_| "Could not find the bundled command.")?
            .join("binaries/ibis")
    } else {
        directory.join(format!("ibis{}", std::env::consts::EXE_SUFFIX))
    };
    let path: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    // A just-installed Windows app can inherit the installer's old environment.
    // Read the newly saved user PATH too, without changing this process's PATH.
    #[cfg(target_os = "windows")]
    let path = with_registered_path(path, registered_user_path()?.as_deref());
    // Finder often omits Homebrew from PATH. Include its known command locations
    // for conflict detection, without running a login shell or its profiles.
    #[cfg(target_os = "macos")]
    let path = path
        .into_iter()
        .chain([
            std::path::PathBuf::from("/opt/homebrew/bin"),
            std::path::PathBuf::from("/usr/local/bin"),
        ])
        .collect();
    #[cfg(target_os = "linux")]
    let appimage = ibis_core::command_install::appimage_for(
        &exe,
        std::env::var_os("APPIMAGE").map(std::path::PathBuf::from),
        std::env::var_os("APPDIR").map(std::path::PathBuf::from),
    );
    #[cfg(not(target_os = "linux"))]
    let appimage = None;
    Ok(CommandInstall::new(
        home,
        path,
        bundled,
        appimage,
        ibis_core::VERSION.into(),
    ))
}

/// Connections use the installer's discovery locations, plus the per-user
/// command even when a Dock-launched app has not inherited that PATH yet.
pub(crate) fn agent_home(app: &tauri::AppHandle) -> Result<ibis_engine::agent_apps::Home, String> {
    agent_environment(
        installer(app)?,
        app.path()
            .config_dir()
            .map_err(|_| "Could not find your settings folder.")?,
        std::env::var_os("CODEX_HOME")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute()),
    )
}

fn agent_environment(
    install: CommandInstall,
    config: std::path::PathBuf,
    codex_home: Option<std::path::PathBuf>,
) -> Result<ibis_engine::agent_apps::Home, String> {
    let status = install.status().map_err(|_| "Could not check the ibis command.")?;
    let destination = install.destination();
    let bundled = (!install.temporary()).then_some(install.bundled);
    let mut path = install.path;
    path.push(install.home.join(".local/bin"));
    let mut home = ibis_engine::agent_apps::Home {
        root: install.home,
        config: Some(config),
        path,
        codex_home,
        ibis: None,
    };
    home.ibis = if status.removable && ibis_engine::agent_apps::executable(&destination) {
        Some(destination)
    } else {
        home.find("ibis").or(bundled)
    };
    Ok(home)
}

#[cfg(any(test, target_os = "windows"))]
fn with_registered_path(
    mut inherited: Vec<std::path::PathBuf>,
    registered: Option<&std::ffi::OsStr>,
) -> Vec<std::path::PathBuf> {
    if let Some(registered) = registered {
        inherited.extend(std::env::split_paths(registered));
    }
    inherited
}

#[cfg(target_os = "windows")]
fn registered_user_path() -> Result<Option<std::ffi::OsString>, String> {
    use winsafe::{HKEY, RegistryValue, co};
    match HKEY::CURRENT_USER.RegGetValue(
        Some("Environment"),
        Some("Path"),
        co::RRF::RT_REG_SZ | co::RRF::RT_REG_EXPAND_SZ,
    ) {
        Ok(RegistryValue::Sz(path)) => Ok(Some(path.into())),
        Ok(RegistryValue::ExpandSz(path)) => winsafe::ExpandEnvironmentStrings(&path)
            .map(|path| Some(path.into()))
            .map_err(|_| "Could not read the installed command's PATH.".into()),
        Err(error) if error == co::ERROR::FILE_NOT_FOUND => Ok(None),
        _ => Err("Could not read the installed command's PATH.".into()),
    }
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn cli_status(app: tauri::AppHandle) -> Result<CommandStatus, String> {
    operation(app, Step::Status).await
}

#[tauri::command]
#[ibis_desktop_macros::command]
pub(crate) async fn set_cli_installed(app: tauri::AppHandle, installed: bool) -> Result<CommandStatus, String> {
    operation(app, if installed { Step::Install } else { Step::Remove }).await
}

/// Release launches maintain the per-user command; Windows's installer owns it.
pub(crate) fn keep_at_launch(app: &tauri::AppHandle) {
    if cfg!(debug_assertions) || cfg!(windows) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = operation(app, Step::Keep).await;
    });
}

enum Step {
    Status,
    Install,
    Remove,
    Keep,
}

async fn operation(app: tauri::AppHandle, step: Step) -> Result<CommandStatus, String> {
    static OWNER: tauri::async_runtime::Mutex<()> = tauri::async_runtime::Mutex::const_new(());
    // Lock before scheduling workers so a later removal cannot overtake an install.
    with_owner(&OWNER, move || {
        let install = installer(&app)?;
        match step {
            Step::Status => install.status(),
            Step::Install => install.install(),
            Step::Remove => install.remove(),
            Step::Keep => install.keep_installed(),
        }
        .map_err(|_| "Could not change the ibis command. Check the folder permissions and try again.".into())
    })
    .await
}

pub(crate) async fn with_owner<T: Send + 'static>(
    owner: &tauri::async_runtime::Mutex<()>,
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let _owner = owner.lock().await;
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "Could not complete the operation.".to_owned())?
}

#[tauri::command]
#[ibis_desktop_macros::command(main)]
pub(crate) fn copy_cli_profile(app: tauri::AppHandle) -> Result<(), String> {
    crate::copy::write_plain(&app, PROFILE_LINE)
}

#[cfg(test)]
#[cfg(unix)]
#[path = "cli_install_tests.rs"]
mod tests;
