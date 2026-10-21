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
    /// Written beside the entry and renamed, so a session starting meanwhile
    /// never reads half a file.
    fn write(&self, contents: &str) -> Result<(), Failed> {
        let folder = self.path.parent().ok_or(Failed)?;
        std::fs::create_dir_all(folder).map_err(|_| Failed)?;
        let partial = self.path.with_extension("desktop.partial");
        std::fs::write(&partial, contents).map_err(|_| Failed)?;
        std::fs::rename(&partial, &self.path).map_err(|_| {
            let _ = std::fs::remove_file(&partial);
            Failed
        })
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
    fn appimage_counts_only_from_inside_its_own_mount() {
        let p = |s: &str| Some(PathBuf::from(s));
        // Ibis's own AppImage: the entry names the .AppImage file.
        assert_eq!(
            launch_program(
                p("/tmp/.mount_IbisAb/usr/bin/ibis-desktop"),
                p("/home/a/Apps/Ibis-Linux.AppImage"),
                p("/tmp/.mount_IbisAb"),
                None,
            ),
            p("/home/a/Apps/Ibis-Linux.AppImage")
        );
        // A .deb Ibis started from another AppImage (a terminal, say) inherits
        // that app's variables: they must not name it.
        assert_eq!(
            launch_program(
                p("/usr/bin/ibis-desktop"),
                p("/home/a/Apps/Terminal.AppImage"),
                p("/tmp/.mount_TermXy"),
                None,
            ),
            p("/usr/bin/ibis-desktop")
        );
        assert_eq!(
            launch_program(
                p("/usr/bin/ibis-desktop"),
                p("/home/a/Apps/Terminal.AppImage"),
                None,
                None
            ),
            p("/usr/bin/ibis-desktop")
        );
        assert_eq!(
            launch_program(p("/usr/bin/ibis-desktop"), p("/x.AppImage"), p("/"), None),
            p("/usr/bin/ibis-desktop")
        );
        assert_eq!(
            launch_program(
                p("/tmp/.mount_a/ibis"),
                p("relative.AppImage"),
                p("/tmp/.mount_a"),
                None
            ),
            p("/tmp/.mount_a/ibis")
        );
        assert_eq!(launch_program(None, p("/x.AppImage"), p("/tmp/.mount_a"), None), None);
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

    #[test]
    fn hidden_entries_count_as_off() {
        assert!(!switched_off(
            "[Desktop Entry]\nName=Ibis\nX-GNOME-Autostart-enabled=true\n"
        ));
        assert!(switched_off("[Desktop Entry]\nName=Ibis\nHidden=true\n"));
        assert!(switched_off("[Desktop Entry]\nX-GNOME-Autostart-enabled = false\n"));
        // Keys of other groups are not the entry's.
        assert!(!switched_off(
            "[Desktop Entry]\nName=Ibis\n[Desktop Action x]\nHidden=true\n"
        ));
    }

    /// The entry appears and disappears in a temporary configuration folder,
    /// the way it does in `$XDG_CONFIG_HOME/autostart`.
    #[test]
    fn the_autostart_file_appears_and_disappears() {
        let home = tempfile::tempdir().unwrap_or_else(|e| panic!("temporary folder: {e}"));
        let path = home
            .path()
            .join("config dir/autostart/io.github.ibiscite.ibis.nightly.desktop");
        let stable = home.path().join("config dir/autostart/io.github.ibiscite.ibis.desktop");
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
        assert!(!path.with_extension("desktop.partial").exists());

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
    fn refresh_repoints_only_its_own_entry_whose_program_is_gone() {
        let home = tempfile::tempdir().unwrap_or_else(|e| panic!("temporary folder: {e}"));
        let path = home.path().join("autostart/ibis.desktop");
        let here = home.path().join("now/Ibis Linux.AppImage");
        std::fs::create_dir_all(here.parent().unwrap_or(home.path())).unwrap_or_default();
        std::fs::write(&here, "").unwrap_or_default();
        let moved = home.path().join("gone/Ibis-Linux.AppImage");
        let entry = file(path.clone(), "Ibis", &here.to_string_lossy());

        // Written by an earlier copy that has since moved, then edited by hand.
        let old = desktop_entry("Ibis", &moved).unwrap_or_default() + "X-GNOME-Autostart-Delay=5\n";
        std::fs::create_dir_all(home.path().join("autostart")).unwrap_or_default();
        std::fs::write(&path, &old).unwrap_or_default();
        assert_eq!(entry.refresh(), Ok(()));
        let now = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(now.contains(&format!("TryExec={}\n", here.display())), "{now}");
        assert!(
            now.contains(&format!("Exec=\"{}\" --at-login\n", here.display())),
            "{now}"
        );
        assert!(now.contains("X-GNOME-Autostart-Delay=5\n"), "the person's keys stay");
        assert!(!now.contains("gone/"));

        // Keys written with spaces around `=` are recognised too.
        let spaced = old.replace("TryExec=", "TryExec = ").replace("\nExec=", "\nExec = ");
        std::fs::write(&path, &spaced).unwrap_or_default();
        assert_eq!(entry.refresh(), Ok(()));
        let now = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(now.contains(&format!("TryExec={}\n", here.display())), "{now}");
        assert!(!now.contains("gone/"), "{now}");

        // A program that still exists is someone's choice: left alone.
        let other = home.path().join("other-ibis");
        std::fs::write(&other, "").unwrap_or_default();
        let theirs = desktop_entry("Ibis", &other).unwrap_or_default();
        std::fs::write(&path, &theirs).unwrap_or_default();
        assert_eq!(entry.refresh(), Ok(()));
        assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), theirs);

        // A file Ibis did not write is left alone, even with a missing program.
        let foreign = format!("[Desktop Entry]\nTryExec={}\nExec=x\n", moved.display());
        std::fs::write(&path, &foreign).unwrap_or_default();
        assert_eq!(entry.refresh(), Ok(()));
        assert_eq!(std::fs::read_to_string(&path).unwrap_or_default(), foreign);

        // A symbolic link stays a link.
        let target = home.path().join("kept.desktop");
        std::fs::write(&target, &old).unwrap_or_default();
        std::fs::remove_file(&path).unwrap_or_default();
        let linked = std::os::unix::fs::symlink(&target, &path);
        assert!(linked.is_ok());
        assert_eq!(entry.refresh(), Ok(()));
        assert!(std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()));
        assert_eq!(std::fs::read_to_string(&target).unwrap_or_default(), old);
    }

    /// A package upgrade replaces the running program: Linux then names it
    /// "... (deleted)". The new program at that path is used; with none, the
    /// switch is refused rather than writing a path that never starts.
    #[test]
    fn a_replaced_program_is_found_again_or_refused() {
        let home = tempfile::tempdir().unwrap_or_else(|e| panic!("temporary folder: {e}"));
        let upgraded = home.path().join("ibis-desktop");
        std::fs::write(&upgraded, "").unwrap_or_default();
        let deleted = |path: &Path| PathBuf::from(format!("{} (deleted)", path.display()));
        assert_eq!(
            launch_program(Some(deleted(&upgraded)), None, None, None),
            Some(upgraded.clone())
        );

        let removed = home.path().join("removed-ibis");
        let dead = deleted(&removed);
        assert_eq!(launch_program(Some(dead.clone()), None, None, None), Some(dead.clone()));
        let path = home.path().join("autostart/ibis.desktop");
        let entry = file(path.clone(), "Ibis", &dead.to_string_lossy());
        assert_eq!(
            change(Some(&entry), true, false),
            item(false, Some(LoginItemProblem::Refused))
        );
        assert!(!path.exists(), "no entry for a program that is gone");
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
