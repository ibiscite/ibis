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
    fn owner(&self) -> io::Result<File> {
        let owner = File::open(self.home.join(".local/share/ibis"))?;
        owner.try_lock().map_err(io::Error::from)?;
        Ok(owner)
    }

    pub fn remove(&self) -> io::Result<CommandStatus> {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

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
