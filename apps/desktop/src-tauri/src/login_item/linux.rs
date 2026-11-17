//! The Linux login entry: an XDG autostart `.desktop` file.

use std::path::{Path, PathBuf};

use super::{AT_LOGIN_ARG, Entry, Failed};

/// Marks a file as one Ibis wrote, so a later Ibis may repoint it.
const MARK: &str = "X-Ibis-Autostart=true";

/// The program a login entry should start. `APPIMAGE` and `APPDIR` are
/// inherited by every child of any AppImage, so `APPIMAGE` counts only when
/// this process runs from inside that mount; otherwise it names someone
/// else's program (#530 review).
/// A packager can embed the public launcher path at compile time when the
/// executable itself needs a wrapper's environment. Invalid paths refuse
/// autostart rather than silently falling back to an unwrapped executable.
pub(crate) fn launch_program(
    current_exe: Option<PathBuf>,
    appimage: Option<PathBuf>,
    appdir: Option<PathBuf>,
    package_launcher: Option<&str>,
) -> Option<PathBuf> {
    match (current_exe.map(live_path), appimage, appdir) {
        (Some(exe), Some(image), Some(dir))
            if image.is_absolute() && dir.is_absolute() && dir.parent().is_some() && exe.starts_with(&dir) =>
        {
            Some(image)
        }
        (exe, _, _) => match package_launcher {
            None => exe,
            Some(path) if Path::new(path).is_absolute() && !path.chars().any(char::is_control) => {
                Some(PathBuf::from(path))
            }
            Some(_) => None,
        },
    }
}

/// After a package upgrade replaces the running program, Linux reports it as
/// "/usr/bin/ibis-desktop (deleted)". The new program at the same path is
/// the one to start; when there is none, the dead path is kept and `enable`
/// refuses it, so the person sees why.
fn live_path(exe: PathBuf) -> PathBuf {
    match exe.to_str().and_then(|path| path.strip_suffix(" (deleted)")) {
        Some(path) if Path::new(path).is_file() => PathBuf::from(path),
        _ => exe,
    }
}

/// An XDG autostart entry: on while the file exists and is not hidden.
pub(crate) struct DesktopFile {
    pub(crate) path: PathBuf,
    pub(crate) name: String,
    pub(crate) program: PathBuf,
}

impl Entry for DesktopFile {
    fn is_enabled(&self) -> Result<bool, Failed> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(!switched_off(&text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err(Failed),
        }
    }

    fn enable(&self) -> Result<(), Failed> {
        // An entry for a missing program would show as on and never start.
        if !self.program.is_file() {
            return Err(Failed);
        }
        let contents = desktop_entry(&self.name, &self.program).ok_or(Failed)?;
        self.write(&contents)
    }

    fn disable(&self) -> Result<(), Failed> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(Failed),
        }
    }

    /// Repoint an entry Ibis wrote at this copy when its program is gone (a
    /// moved or renamed AppImage). Only `Exec` and `TryExec` change, so the
    /// person's own keys stay. A symbolic link, a file Ibis did not write,
    /// or one whose program still exists is left alone.
    fn refresh(&self) -> Result<(), Failed> {
        let Ok(metadata) = std::fs::symlink_metadata(&self.path) else {
            return Ok(());
        };
        if !metadata.is_file() {
            return Ok(());
        }
        let text = std::fs::read_to_string(&self.path).map_err(|_| Failed)?;
        let Some(updated) = repointed(&text, &self.program) else {
            return Ok(());
        };
        self.write(&updated)
    }
}

impl DesktopFile {
    pub(crate) fn for_identity(
        folder: &Path,
        identifier: &str,
        development: bool,
        name: &str,
        program: PathBuf,
    ) -> Self {
        let file = if development {
            format!("{identifier}.development.desktop")
        } else {
            format!("{identifier}.desktop")
        };
        Self {
            path: folder.join(file),
            name: name.to_string(),
            program,
        }
    }

    /// Written beside the entry and renamed, so a session starting meanwhile
    /// never reads half a file.
    fn write(&self, contents: &str) -> Result<(), Failed> {
        let folder = self.path.parent().ok_or(Failed)?;
        std::fs::create_dir_all(folder).map_err(|_| Failed)?;
        ibis_core::atomic_file::AtomicFile::new(&self.path, contents.as_bytes())
            .and_then(|file| file.replace())
            .map_err(|_| Failed)
    }
}

/// `text` with its `Exec` and `TryExec` pointing at `program`, or `None` when
/// it should be left as it is.
fn repointed(text: &str, program: &Path) -> Option<String> {
    let (try_exec, exec) = program_lines(program)?;
    let mut in_entry = false;
    let mut ours = false;
    let mut old_program = None;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
        } else if in_entry && (line == MARK || key_value(line) == Some(("X-Ibis-Autostart", "true"))) {
            ours = true;
        } else if in_entry && let Some(("TryExec", value)) = key_value(line) {
            old_program = Some(unescape_string(value));
        }
    }
    let old_program = old_program?;
    if !ours || Path::new(&old_program) == program || Path::new(&old_program).exists() {
        return None;
    }
    in_entry = false;
    let mut updated = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_entry = trimmed == "[Desktop Entry]";
        }
        let replaced = match key_value(trimmed) {
            Some(("TryExec", _)) if in_entry => &try_exec,
            Some(("Exec", _)) if in_entry => &exec,
            _ => line,
        };
        updated.push_str(replaced);
        updated.push('\n');
    }
    Some(updated)
}

/// A `Key=value` line's key and value, with spaces around `=` allowed.
fn key_value(line: &str) -> Option<(&str, &str)> {
    line.split_once('=')
        .map(|(key, value)| (key.trim(), value.trim_start()))
}

/// Desktops switch an entry off by hiding it rather than deleting it.
pub(crate) fn switched_off(text: &str) -> bool {
    let mut in_entry = false;
    text.lines().map(str::trim).any(|line| {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            return false;
        }
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        let (key, value) = (key.trim(), value.trim());
        in_entry && ((key == "Hidden" && value == "true") || (key == "X-GNOME-Autostart-enabled" && value == "false"))
    })
}

/// The `TryExec` and `Exec` lines for `program`, or `None` when its path
/// cannot be represented (relative, not Unicode, or with control characters).
fn program_lines(program: &Path) -> Option<(String, String)> {
    let program = program.to_str()?;
    if !program.starts_with('/') || program.chars().any(char::is_control) {
        return None;
    }
    Some((
        // When the program is removed, the session skips the entry.
        format!("TryExec={}", escape_string(program)),
        format!("Exec={} {AT_LOGIN_ARG}", escape_string(&quote_argument(program))),
    ))
}

/// The autostart entry that starts `program` hidden, or `None` when it
/// cannot be represented.
pub(crate) fn desktop_entry(name: &str, program: &Path) -> Option<String> {
    let (try_exec, exec) = program_lines(program)?;
    if name.chars().any(char::is_control) {
        return None;
    }
    Some(format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={name}\n\
         Comment=Keeps the {name} shortcut ready after you log in\n\
         {try_exec}\n\
         {exec}\n\
         Terminal=false\n\
         StartupNotify=false\n\
         X-GNOME-Autostart-enabled=true\n\
         {MARK}\n",
        name = escape_string(name),
    ))
}

/// An `Exec` argument, quoted as the Desktop Entry specification says.
fn quote_argument(argument: &str) -> String {
    let mut quoted = String::from("\"");
    for c in argument.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            // A literal percent sign; `%f` and the like are field codes.
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// A desktop entry string value: backslashes are escaped once more.
fn escape_string(value: &str) -> String {
    value.replace('\\', "\\\\")
}

/// The string escapes of the Desktop Entry specification, undone.
fn unescape_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::{LoginItemProblem, LoginItems, Reading, change};
    use super::*;

    const fn item(enabled: bool, problem: Option<LoginItemProblem>) -> Reading {
        Reading { enabled, problem }
    }

    fn file(path: PathBuf, name: &str, program: &str) -> DesktopFile {
        DesktopFile {
            path,
            name: name.to_string(),
            program: PathBuf::from(program),
        }
    }

    #[test]
    fn each_identity_and_development_build_has_its_own_slot() {
        for (identifier, development, filename) in [
            ("com.ibiscite.ibis", false, "com.ibiscite.ibis.desktop"),
            ("com.ibiscite.ibis.nightly", false, "com.ibiscite.ibis.nightly.desktop"),
            ("com.ibiscite.ibis", true, "com.ibiscite.ibis.development.desktop"),
            (
                "com.ibiscite.ibis.nightly",
                true,
                "com.ibiscite.ibis.nightly.development.desktop",
            ),
            ("example.other", false, "example.other.desktop"),
        ] {
            let entry =
                DesktopFile::for_identity(Path::new("/synthetic"), identifier, development, "Ibis", "/ibis".into());
            assert_eq!(entry.path, Path::new("/synthetic").join(filename));
        }
    }

    #[test]
    fn packaged_login_uses_the_launcher_instead_of_the_wrapped_program() {
        let executable = Some(PathBuf::from("/nix/store/example-ibis/bin/.ibis-desktop-wrapped"));
        let launcher = "/nix/store/example-ibis/bin/ibis-desktop";
        assert_eq!(
            launch_program(executable.clone(), None, None, Some(launcher)),
            Some(PathBuf::from(launcher))
        );
        assert_eq!(launch_program(executable.clone(), None, None, None), executable);
        for invalid in ["", "relative/ibis", "/nix/store/ibis\nother"] {
            assert_eq!(launch_program(executable.clone(), None, None, Some(invalid)), None);
        }
        // The launcher's existence is checked by DesktopFile::enable.
        assert_eq!(
            launch_program(None, None, None, Some(launcher)),
            Some(PathBuf::from(launcher))
        );
        // Even a configured package keeps the existing AppImage ownership rule.
        assert_eq!(
            launch_program(
                Some(PathBuf::from("/tmp/.mount_Ibis/bin/ibis-desktop")),
                Some(PathBuf::from("/home/a/Ibis.AppImage")),
                Some(PathBuf::from("/tmp/.mount_Ibis")),
                Some(launcher),
            ),
            Some(PathBuf::from("/home/a/Ibis.AppImage"))
        );
    }

    #[test]
    fn exec_paths_with_reserved_characters_are_quoted() {
        let Some(entry) = desktop_entry(
            "Ibis Nightly",
            Path::new("/home/a b/$x/`c`/d\"e/f\\g/50%/Ibis.AppImage"),
        ) else {
            panic!("a Unicode absolute path is representable");
        };
        assert!(
            entry.contains("Exec=\"/home/a b/\\\\$x/\\\\`c\\\\`/d\\\\\"e/f\\\\\\\\g/50%%/Ibis.AppImage\" --at-login\n")
        );
        assert!(entry.contains("TryExec=/home/a b/$x/`c`/d\"e/f\\\\g/50%/Ibis.AppImage\n"));
        assert!(entry.contains("Name=Ibis Nightly\n"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert_eq!(
            unescape_string("/home/a b/$x/`c`/d\"e/f\\\\g/50%/Ibis.AppImage"),
            "/home/a b/$x/`c`/d\"e/f\\g/50%/Ibis.AppImage"
        );
    }

    #[test]
    fn unrepresentable_paths_are_refused_not_mangled() {
        assert_eq!(desktop_entry("Ibis", Path::new("relative/ibis")), None);
        assert_eq!(desktop_entry("Ibis", Path::new("/tmp/a\nExec=evil")), None);
        assert_eq!(desktop_entry("Ibis\nExec=evil", Path::new("/usr/bin/ibis")), None);
        {
            use std::os::unix::ffi::OsStrExt;
            let invalid = std::ffi::OsStr::from_bytes(b"/tmp/\xff/ibis");
            assert_eq!(desktop_entry("Ibis", Path::new(invalid)), None);
        }
        let entry = file(PathBuf::from("/nonexistent/autostart/ibis.desktop"), "Ibis", "relative");
        assert_eq!(entry.enable(), Err(Failed));
    }

    /// The entry appears and disappears in a temporary configuration folder,
    /// the way it does in `$XDG_CONFIG_HOME/autostart`.
    #[test]
    fn the_autostart_file_appears_and_disappears() {
        let home = tempfile::tempdir().unwrap_or_else(|e| panic!("temporary folder: {e}"));
        let path = home
            .path()
            .join("config dir/autostart/com.ibiscite.ibis.nightly.desktop");
        let stable = home.path().join("config dir/autostart/com.ibiscite.ibis.desktop");
        let program = home.path().join("Ibis Nightly/ibis");
        std::fs::create_dir_all(home.path().join("Ibis Nightly")).unwrap_or_default();
        std::fs::write(&program, "").unwrap_or_default();
        let items = LoginItems::new(
            "Ibis Nightly",
            Some(Box::new(file(path.clone(), "Ibis Nightly", &program.to_string_lossy()))),
        );
        assert_eq!(items.reading(), item(false, None));
        assert!(items.change(true).enabled);
        let written = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            written.contains(&format!("Exec=\"{}\" --at-login\n", program.display())),
            "{written}"
        );
        assert!(!stable.exists(), "Nightly must not write the stable entry");

        // Switched off in the desktop's settings: shown as off, and turning it
        // on again writes a fresh entry.
        std::fs::write(&path, format!("{written}Hidden=true\n")).unwrap_or_default();
        assert_eq!(items.reading(), item(false, None));
        assert_eq!(items.change(true).problem, None);
        assert_eq!(items.reading(), item(true, None));

        // Deleted by hand: shown as off.
        std::fs::remove_file(&path).unwrap_or_default();
        assert_eq!(items.reading(), item(false, None));

        assert!(items.change(true).enabled);
        assert!(!items.change(false).enabled);
        assert!(!path.exists());
        assert_eq!(items.change(false), items.read());
    }

    #[test]
    fn an_unwritable_folder_is_a_visible_refusal() {
        use std::os::unix::fs::PermissionsExt;
        let home = tempfile::tempdir().unwrap_or_else(|e| panic!("temporary folder: {e}"));
        let folder = home.path().join("autostart");
        std::fs::create_dir(&folder).unwrap_or_default();
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o500)).unwrap_or_default();
        let probe = folder.join("probe");
        if std::fs::write(&probe, "").is_ok() {
            // Running as root: permissions do not apply, nothing to observe.
            let _ = std::fs::remove_file(probe);
            return;
        }
        let program = home.path().join("ibis");
        std::fs::write(&program, "").unwrap_or_default();
        let entry = file(folder.join("ibis.desktop"), "Ibis", &program.to_string_lossy());
        assert_eq!(
            change(Some(&entry), true, false),
            item(false, Some(LoginItemProblem::Refused))
        );
        std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700)).unwrap_or_default();
    }
}
