//! The Microsoft Store copy: the same app in an MSIX package
//! (`packaging/msix/AppxManifest.xml`), which Windows runs with a package
//! identity. The Store updates it, its startup task starts it at login, and
//! its app execution alias puts `ibis` on PATH, so those three jobs leave the
//! installer's ways (the updater, the Run key, the PATH entry) unused.

/// The startup task's id in the package manifest.
#[cfg(any(test, target_os = "windows"))]
const STARTUP_TASK: &str = "IbisAtLogin";

/// Whether this copy came from the Store package.
pub(crate) fn installed() -> bool {
    #[cfg(target_os = "windows")]
    {
        static PACKAGED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *PACKAGED.get_or_init(|| windows::ApplicationModel::Package::Current().is_ok())
    }
    #[cfg(not(target_os = "windows"))]
    false
}

/// The `ibis` command Windows keeps on PATH for the package, at the same
/// place across Store updates; agent apps are pointed at it.
pub(crate) fn command() -> Option<std::path::PathBuf> {
    if !installed() {
        return None;
    }
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(std::path::Path::new(&local).join(r"Microsoft\WindowsApps\ibis.exe"))
}

/// Whether the package's startup task started this process.
pub(crate) fn started_at_login() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::ApplicationModel::Activation::ActivationKind;
        installed()
            && windows::ApplicationModel::AppInstance::GetActivatedEventArgs()
                .and_then(|args| args.Kind())
                .is_ok_and(|kind| kind == ActivationKind::StartupTask)
    }
    #[cfg(not(target_os = "windows"))]
    false
}

/// The login entry of the Store copy: its startup task, which Task Manager's
/// Startup apps lists and can switch off. Ibis only asks; a person who
/// switched it off there must switch it on there again.
#[cfg(target_os = "windows")]
pub(crate) struct StartupTask;

#[cfg(target_os = "windows")]
impl StartupTask {
    fn task() -> Result<windows::ApplicationModel::StartupTask, crate::login_item::Failed> {
        windows::ApplicationModel::StartupTask::GetAsync(&windows::core::HSTRING::from(STARTUP_TASK))
            .and_then(|operation| operation.join())
            .map_err(|_| crate::login_item::Failed)
    }
}

#[cfg(target_os = "windows")]
impl crate::login_item::Entry for StartupTask {
    fn is_enabled(&self) -> Result<bool, crate::login_item::Failed> {
        use windows::ApplicationModel::StartupTaskState as State;
        let state = Self::task()?.State().map_err(|_| crate::login_item::Failed)?;
        Ok(state == State::Enabled || state == State::EnabledByPolicy)
    }
    fn enable(&self) -> Result<(), crate::login_item::Failed> {
        Self::task()?
            .RequestEnableAsync()
            .and_then(|operation| operation.join())
            .map(|_| ())
            .map_err(|_| crate::login_item::Failed)
    }
    fn disable(&self) -> Result<(), crate::login_item::Failed> {
        Self::task()?.Disable().map_err(|_| crate::login_item::Failed)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_manifest_declares_the_startup_task_and_the_command_alias() {
        let manifest = include_str!("../packaging/msix/AppxManifest.xml");
        assert!(manifest.contains(&format!(r#"TaskId="{}""#, super::STARTUP_TASK)));
        assert!(manifest.contains(r#"Alias="ibis.exe""#));
    }
}
