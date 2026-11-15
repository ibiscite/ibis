//! App-owned command installation. Shared with terminal setup; never launches a
//! command found on PATH or changes a shell profile. All locations are native-owned.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const PROFILE_LINE: &str = "export PATH=\"$HOME/.local/bin:$PATH\"";

/// APPIMAGE/APPDIR are inherited by children of every AppImage. They name this
/// app only while its executable is actually inside that non-root mount.
pub fn appimage_for(exe: &Path, image: Option<PathBuf>, mount: Option<PathBuf>) -> Option<PathBuf> {
    image
        .zip(mount)
        .filter(|(image, mount)| {
            image.is_absolute() && mount.is_absolute() && mount.parent().is_some() && exe.starts_with(mount)
        })
        .map(|(image, _)| image)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CommandState {
    NotInstalled,
    Matching,
    Different,
    Foreign,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
pub struct CommandStatus {
    pub state: CommandState,
    pub version: Option<String>,
    pub used_path: Option<PathBuf>,
    pub profile_line: Option<String>,
    pub removable: bool,
    pub installable: bool,
}

#[derive(Clone)]
pub struct CommandInstall {
    pub home: PathBuf,
    pub path: Vec<PathBuf>,
    pub bundled: PathBuf,
    pub app: Option<PathBuf>,
    pub version: String,
}

#[derive(Serialize, Deserialize)]
struct Receipt {
    target: PathBuf,
    version: String,
    digest: String,
    copy: bool,
    app: Option<PathBuf>,
}

fn digest(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 65_536];
    loop {
        let size = file.read(&mut buffer)?;
        if size == 0 {
            break;
        }
        hash.update(&buffer[..size]);
    }
    use std::fmt::Write as _;
    Ok(hash.finalize().iter().fold(String::with_capacity(64), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    }))
}

fn same_file(a: &Path, b: &Path) -> bool {
    a.canonicalize()
        .ok()
        .zip(b.canonicalize().ok())
        .is_some_and(|(a, b)| a == b)
}

fn is_command(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use nix::fcntl::{AT_FDCWD, AtFlags};
        use nix::unistd::{AccessFlags, faccessat};
        // Match command lookup using effective credentials, including ACLs and
        // directory traversal permissions, without ever executing the file.
        faccessat(AT_FDCWD, path, AccessFlags::X_OK, AtFlags::AT_EACCESS).is_ok()
    }
    #[cfg(not(unix))]
    {
        true
    }
}

struct CommandOwner(File);

impl Drop for CommandOwner {
    fn drop(&mut self) {
        // A concurrent fork can retain this file description after we close it.
        // Release its lock when the operation ends, before closing our handle.
        let _ = self.0.unlock();
    }
}

impl CommandInstall {
    pub fn new(home: PathBuf, path: Vec<PathBuf>, bundled: PathBuf, app: Option<PathBuf>, version: String) -> Self {
        Self {
            home,
            path,
            bundled,
            app,
            version,
        }
    }

    pub fn destination(&self) -> PathBuf {
        self.home.join(".local/bin/ibis")
    }
    pub fn receipt(&self) -> PathBuf {
        self.home.join(".local/share/ibis/command.json")
    }
    /// Left once the app has put the command in place at a launch, or the
    /// person chose in Settings: later launches leave that choice alone.
    fn settled(&self) -> PathBuf {
        self.home.join(".local/share/ibis/command-settled")
    }

    /// An app still on its disk image, translocated by macOS or run from an
    /// unregistered temporary mount: its command would vanish with it.
    fn temporary(&self) -> bool {
        let text = self.bundled.to_string_lossy();
        if cfg!(target_os = "macos") {
            text.starts_with("/Volumes/") || text.contains("/AppTranslocation/")
        } else {
            self.app.is_none() && text.contains("/.mount_")
        }
    }

    /// What the app does at launch, so installing the app is all it takes:
    /// the first launch puts the command on PATH when nothing else holds that
    /// place, and later launches bring only this app's own copy up to date.
    /// Anything another installer owns (Homebrew, a download), another Ibis
    /// channel's command, and a command the person removed are left alone.
    pub fn keep_installed(&self) -> io::Result<CommandStatus> {
        if self.temporary() {
            return self.status();
        }
        let first =
            matches!(fs::symlink_metadata(self.settled()), Err(error) if error.kind() == io::ErrorKind::NotFound);
        let status = self.status()?;
        let own_copy = status.state == CommandState::Different
            && status.removable
            && self
                .read_receipt()?
                .is_some_and(|receipt| receipt.copy && receipt.app.is_some() && receipt.app == self.app);
        let status = if own_copy {
            self.remove_owned()?;
            self.install()?
        } else if first {
            self.install()?
        } else {
            status
        };
        if first {
            self.settle()?;
        }
        Ok(status)
    }

    fn settle(&self) -> io::Result<()> {
        fs::create_dir_all(self.home.join(".local/share/ibis"))?;
        match crate::atomic_file::create_private(&self.settled()) {
            Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(error),
            _ => Ok(()),
        }
    }

    fn read_receipt(&self) -> io::Result<Option<Receipt>> {
        match fs::read(self.receipt()) {
            Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn owned(&self, receipt: &Receipt) -> bool {
        let destination = self.destination();
        if receipt.copy {
            fs::symlink_metadata(&destination).is_ok_and(|m| m.is_file() && !m.is_symlink())
                && digest(&destination).is_ok_and(|hash| hash == receipt.digest)
        } else {
            fs::read_link(destination).is_ok_and(|target| target == receipt.target)
        }
    }

    /// A verified copied AppImage command remembers the persistent AppImage,
    /// rather than the temporary mount in which it was built or installed.
    pub fn installed_app(&self) -> io::Result<Option<PathBuf>> {
        Ok(self.read_receipt()?.filter(|r| self.owned(r)).and_then(|r| r.app))
    }

    pub fn status(&self) -> io::Result<CommandStatus> {
        let destination = self.destination();
        let receipt = match fs::symlink_metadata(&destination) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
            Ok(_) => self.read_receipt()?,
        };
        let owned = receipt.as_ref().is_some_and(|r| self.owned(r));
        let used = self
            .path
            .iter()
            .map(|p| p.join(format!("ibis{}", std::env::consts::EXE_SUFFIX)))
            .find(|p| is_command(p));
        let local_exists = fs::symlink_metadata(&destination).is_ok();
        let used_is_local = used.as_ref().is_some_and(|p| p == &destination);
        let bundled_used = used.as_ref().is_some_and(|p| same_file(p, &self.bundled));
        let foreign = used.is_some() && !bundled_used && !(used_is_local && owned);
        let local_foreign = local_exists && !owned && !same_file(&destination, &self.bundled);
        let installed = owned || bundled_used || (local_exists && same_file(&destination, &self.bundled));
        let current = installed
            && (same_file(&destination, &self.bundled)
                || bundled_used
                || digest(&destination)
                    .ok()
                    .zip(digest(&self.bundled).ok())
                    .is_some_and(|(a, b)| a == b));
        Ok(CommandStatus {
            state: if foreign || local_foreign {
                CommandState::Foreign
            } else if current {
                CommandState::Matching
            } else if installed {
                CommandState::Different
            } else {
                CommandState::NotInstalled
            },
            version: if foreign || local_foreign {
                None
            } else if current {
                Some(self.version.clone())
            } else {
                receipt.as_ref().filter(|_| owned).map(|r| r.version.clone())
            },
            used_path: used.or_else(|| local_exists.then_some(destination)),
            profile_line: (!foreign
                && !local_foreign
                && !cfg!(windows)
                && !self.path.iter().any(|p| p == &self.home.join(".local/bin"))
                && !bundled_used)
                .then(|| PROFILE_LINE.to_owned()),
            removable: owned,
            installable: !cfg!(windows) && !installed && !foreign && !local_foreign,
        })
    }

    pub fn install(&self) -> io::Result<CommandStatus> {
        let before = self.status()?;
        if !before.installable {
            return Ok(before);
        }
        fs::create_dir_all(self.home.join(".local/share/ibis"))?;
        let _owner = self.owner()?;
        // Recheck under the cross-process owner. An interrupted receipt without
        // a command is recoverable, even if its write was interrupted midway.
        let before = self.status()?;
        if !before.installable {
            return Ok(before);
        }
        let destination = self.destination();
        if matches!(fs::symlink_metadata(&destination), Err(error) if error.kind() == io::ErrorKind::NotFound) {
            match fs::remove_file(self.receipt()) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        let record = Receipt {
            target: self.bundled.clone(),
            version: self.version.clone(),
            digest: digest(&self.bundled)?,
            // Only AppImages need a durable copy outside their temporary mount.
            // macOS and Linux deb/rpm link to the installed bundle instead.
            copy: self.app.is_some(),
            app: self.app.clone(),
        };
        fs::create_dir_all(self.home.join(".local/bin"))?;
        fs::create_dir_all(self.home.join(".local/share/ibis"))?;
        // Claim the receipt before publishing anything. create_new and symlink /
        // hard_link both refuse an existing path, including a dangling symlink.
        let mut receipt = OpenOptions::new().write(true).create_new(true).open(self.receipt())?;
        let result = (|| {
            receipt.write_all(&serde_json::to_vec(&record).map_err(io::Error::other)?)?;
            receipt.sync_all()?;
            if record.copy {
                self.copy_command(&destination)?;
            } else {
                self.link_command(&destination)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(self.receipt());
            return Err(error);
        }
        self.status()
    }

    fn copy_command(&self, destination: &Path) -> io::Result<()> {
        let staging = destination.with_file_name(format!(".ibis-install-{}", std::process::id()));
        let mut output = OpenOptions::new().write(true).create_new(true).open(&staging)?;
        let result = (|| {
            io::copy(&mut File::open(&self.bundled)?, &mut output)?;
            #[allow(unused_mut)]
            let mut permissions = fs::metadata(&self.bundled)?.permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                permissions.set_mode(permissions.mode() | 0o111);
            }
            output.set_permissions(permissions)?;
            output.sync_all()?;
            fs::hard_link(&staging, destination)
        })();
        let _ = fs::remove_file(staging);
        result
    }

    #[cfg(unix)]
    fn link_command(&self, destination: &Path) -> io::Result<()> {
        std::os::unix::fs::symlink(&self.bundled, destination)
    }
    #[cfg(not(unix))]
    fn link_command(&self, _destination: &Path) -> io::Result<()> {
        Err(io::Error::other("the installer owns the Windows command"))
    }

    // Lock the existing app-owned directory itself: no persistent lock artifact
    // and no lock file that can become stale after a crash. try_lock never waits.
    fn owner(&self) -> io::Result<CommandOwner> {
        let owner = File::open(self.home.join(".local/share/ibis"))?;
        owner.try_lock().map_err(io::Error::from)?;
        Ok(CommandOwner(owner))
    }

    /// Remove Command: the person's choice, which later launches respect.
    pub fn remove(&self) -> io::Result<CommandStatus> {
        self.remove_owned()?;
        self.settle()?;
        self.status()
    }

    fn remove_owned(&self) -> io::Result<CommandStatus> {
        let _owner = self.owner()?;
        let receipt = self
            .read_receipt()?
            .ok_or_else(|| io::Error::other("not an Ibis installation"))?;
        if !self.owned(&receipt) {
            return Err(io::Error::other("another command owns this path"));
        }
        fs::remove_file(self.destination())?;
        fs::remove_file(self.receipt())?;
        // Remove only empty directories; shared user directories are preserved.
        let _ = fs::remove_dir(self.home.join(".local/share/ibis"));
        self.status()
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn the_first_launch_installs_the_command_and_later_launches_respect_the_choice() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("fixture: {e}"));
        let home = dir.path().join("home");
        let bundled = dir.path().join("bundled-ibis");
        fs::write(&bundled, b"invented CLI").unwrap_or_else(|e| panic!("bundle: {e}"));
        fs::set_permissions(&bundled, fs::Permissions::from_mode(0o755)).unwrap_or_else(|e| panic!("mode: {e}"));
        let install = CommandInstall::new(
            home.clone(),
            vec![home.join(".local/bin")],
            bundled,
            None,
            "1.2.3".into(),
        );
        let launched = || install.keep_installed().unwrap_or_else(|e| panic!("launch: {e}")).state;
        assert_eq!(launched(), CommandState::Matching);
        assert_eq!(launched(), CommandState::Matching, "a second launch changes nothing");
        install.remove().unwrap_or_else(|e| panic!("remove: {e}"));
        assert_eq!(launched(), CommandState::NotInstalled, "a removal is respected");
        install.install().unwrap_or_else(|e| panic!("install: {e}"));
        install.remove_owned().unwrap_or_else(|e| panic!("remove: {e}"));
        assert_eq!(
            launched(),
            CommandState::NotInstalled,
            "only the first launch puts the command in place; later the person's choice stands"
        );

        // Another channel's command is never taken over, so Stable and Nightly don't trade it.
        let other_channel = dir.path().join("nightly-ibis");
        fs::write(&other_channel, b"invented nightly CLI").unwrap_or_else(|e| panic!("nightly: {e}"));
        fs::set_permissions(&other_channel, fs::Permissions::from_mode(0o755)).unwrap_or_else(|e| panic!("mode: {e}"));
        let nightly = CommandInstall::new(
            home.clone(),
            vec![home.join(".local/bin")],
            other_channel.clone(),
            None,
            "1.2.4".into(),
        );
        nightly.install().unwrap_or_else(|e| panic!("install: {e}"));
        assert_eq!(launched(), CommandState::Different);
        assert_eq!(
            fs::read_link(install.destination()).unwrap_or_else(|e| panic!("link: {e}")),
            other_channel
        );

        // An app still on its disk image never installs one.
        let mounted = CommandInstall::new(
            dir.path().join("mounted"),
            vec![],
            PathBuf::from(if cfg!(target_os = "macos") {
                "/Volumes/Ibis/Ibis.app/Contents/MacOS/ibis"
            } else {
                "/tmp/.mount_IbisInvented/usr/bin/ibis"
            }),
            None,
            "1.2.3".into(),
        );
        assert_eq!(
            mounted.keep_installed().unwrap_or_else(|e| panic!("launch: {e}")).state,
            CommandState::NotInstalled
        );
        assert!(fs::symlink_metadata(mounted.destination()).is_err());

        // A command another installer put first on PATH is never replaced.
        let other = dir.path().join("homebrew");
        fs::create_dir_all(&other).unwrap_or_else(|e| panic!("dir: {e}"));
        fs::write(other.join("ibis"), b"other").unwrap_or_else(|e| panic!("other: {e}"));
        fs::set_permissions(other.join("ibis"), fs::Permissions::from_mode(0o755))
            .unwrap_or_else(|e| panic!("mode: {e}"));
        let fresh = dir.path().join("fresh");
        let beside = CommandInstall::new(
            fresh.clone(),
            vec![other.clone()],
            install.bundled.clone(),
            None,
            "1.2.3".into(),
        );
        assert_eq!(
            beside.keep_installed().unwrap_or_else(|e| panic!("launch: {e}")).state,
            CommandState::Foreign
        );
        assert!(fs::symlink_metadata(fresh.join(".local/bin/ibis")).is_err());
        assert_eq!(
            fs::read(other.join("ibis")).unwrap_or_else(|e| panic!("read: {e}")),
            b"other"
        );
    }

    #[test]
    fn an_inherited_descriptor_cannot_keep_a_finished_install_locked() {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("fixture: {e}"));
        let bundled = dir.path().join("bundled-ibis");
        fs::write(&bundled, b"invented CLI").unwrap_or_else(|e| panic!("bundle: {e}"));
        let install = CommandInstall::new(dir.path().join("home"), vec![], bundled, None, "1.2.3".into());
        install.install().unwrap_or_else(|e| panic!("install: {e}"));
        let owner = install.owner().unwrap_or_else(|e| panic!("owner: {e}"));
        // A concurrent fork can retain the same open file description until exec.
        let inherited = owner.0.try_clone().unwrap_or_else(|e| panic!("inherited owner: {e}"));
        assert!(matches!(install.remove(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
        drop(owner);
        assert_eq!(
            install.remove().unwrap_or_else(|e| panic!("remove: {e}")).state,
            CommandState::NotInstalled
        );
        drop(inherited);
    }

    #[test]
    fn non_executable_path_files_are_skipped_without_overwriting_them() {
        for link in [false, true] {
            let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("fixture: {e}"));
            let home = dir.path().join("home");
            let earlier = dir.path().join("earlier");
            fs::create_dir_all(&earlier).unwrap_or_else(|e| panic!("earlier directory: {e}"));
            let ignored = earlier.join("ibis");
            let bundled = dir.path().join("bundled-ibis");
            fs::write(&bundled, b"invented bundled command").unwrap_or_else(|e| panic!("bundle: {e}"));
            fs::set_permissions(&bundled, fs::Permissions::from_mode(0o755))
                .unwrap_or_else(|e| panic!("executable bundle: {e}"));
            let install = CommandInstall::new(
                home.clone(),
                vec![earlier, home.join(".local/bin")],
                bundled.clone(),
                None,
                "1.2.3".into(),
            );
            let non_executable = dir.path().join("plain-file");
            fs::write(&non_executable, b"plain file").unwrap_or_else(|e| panic!("plain file: {e}"));
            fs::set_permissions(&non_executable, fs::Permissions::from_mode(0o644))
                .unwrap_or_else(|e| panic!("non-executable: {e}"));
            if link {
                symlink(&non_executable, &ignored).unwrap_or_else(|e| panic!("link: {e}"));
            } else {
                fs::copy(&non_executable, &ignored).unwrap_or_else(|e| panic!("earlier file: {e}"));
            }
            assert!(install.status().unwrap_or_else(|e| panic!("status: {e}")).installable);
            let status = install.install().unwrap_or_else(|e| panic!("install: {e}"));
            assert_eq!(status.state, CommandState::Matching);
            assert_eq!(status.used_path, Some(install.destination()));
            assert_eq!(
                fs::read(&ignored).unwrap_or_else(|e| panic!("preserved: {e}")),
                b"plain file"
            );
        }
    }
}
