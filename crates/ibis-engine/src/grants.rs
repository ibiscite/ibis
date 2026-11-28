//! What each app may receive: the per-app paste choices (decision 0031,
//! docs/decisions/0031-what-ibis-remembers-per-app.md), shared by every Ibis
//! process that delivers.
//!
//! `apps.json`, beside `settings.json`, holds at most [`MAX_APPS`] app IDs,
//! each with its paste/copy-only choice. Never a window title, document name or
//! citation text. The file is the only copy: every answer reads it and every
//! change rewrites it under a lock that other Ibis processes share, so a
//! desktop app and the daemon never undo each other's choices. An unreadable
//! file grants nothing and keeps a durable delivery-recovery refusal until
//! each app gets an explicit delivery choice.

use ibis_core::atomic_file::{AtomicFile, open_private};
use ibis_core::destination::{AppId, AppPaste, CopyReason, PasteSafety, has_citation_plugins, lookup};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Apps remembered; the least recently chosen goes first.
pub const MAX_APPS: usize = 200;
/// A file this large was not written by Ibis.
const MAX_BYTES: u64 = 256 * 1024;
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Remembered {
    app: AppId,
    /// Formats moved to settings.json (decision 0040); one written here before
    /// is read and dropped, so the delivery choices beside it keep working.
    #[serde(default, skip_serializing, deserialize_with = "dropped")]
    output: (),
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paste: Option<AppPaste>,
    /// Only unlisted apps approved for Paste need a location binding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    identity: Option<String>,
    /// The one-time note about reference manager plugins has been shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    noted: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    apps: Vec<Remembered>,
    /// Lost delivery choices remain refused until deliberately resolved.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    recovery: bool,
}

fn dropped<'de, D: serde::Deserializer<'de>>(value: D) -> Result<(), D::Error> {
    serde::de::IgnoredAny::deserialize(value).map(|_| ())
}

fn normalized(app: &AppId) -> AppId {
    AppId {
        system: app.system,
        id: app.normalized(),
    }
}

/// The per-app choices in one file. Holds no state of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grants {
    path: PathBuf,
}

impl Grants {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    /// The file beside a profile's `settings.json`.
    pub fn beside(settings: &Path) -> Self {
        Self::open(settings.with_file_name("apps.json"))
    }

    /// The remembered delivery for `app`. An approval to paste into an app
    /// Ibis does not list holds only for the program it was given to, named by
    /// `identity`. While the file needs recovery, an app with no choice is
    /// refused rather than asked.
    pub fn paste_for(&self, app: &AppId, identity: Option<&str>) -> Result<Option<AppPaste>, CopyReason> {
        let app = normalized(app);
        let file = self.current().map_err(|_| CopyReason::PasteUnavailable)?;
        let entry = file.apps.iter().find(|entry| entry.app == app);
        let paste = entry.and_then(|entry| entry.paste).filter(|paste| {
            *paste != AppPaste::Paste
                || lookup(&app).is_some()
                || identity.is_some() && entry.and_then(|entry| entry.identity.as_deref()) == identity
        });
        if file.recovery && paste.is_none() {
            return Err(CopyReason::PasteUnavailable);
        }
        Ok(paste)
    }

    /// Remember how to deliver into `app`. An unlisted app approved for Paste
    /// must name the program, and an app Ibis refuses cannot be approved.
    pub fn set_choice(&self, app: &AppId, paste: AppPaste, identity: Option<String>) -> io::Result<()> {
        let app = normalized(app);
        if paste == AppPaste::Paste && lookup(&app).is_some_and(|entry| entry.safety != PasteSafety::Normal) {
            return Err(io::Error::other("this app cannot be approved for automatic paste"));
        }
        if paste == AppPaste::Paste && lookup(&app).is_none() && identity.is_none() {
            return Err(io::Error::other("this app has no proven identity to approve"));
        }
        let identity = (paste == AppPaste::Paste && lookup(&app).is_none())
            .then_some(identity)
            .flatten();
        self.change(|apps| {
            let noted = apps.iter().any(|entry| entry.app == app && entry.noted);
            apps.retain(|entry| entry.app != app);
            apps.push(Remembered {
                app,
                output: (),
                paste: Some(paste),
                identity,
                noted,
            });
            Ok(())
        })
    }

    /// Whether a paste into `app` should show the one-time plugin note. It
    /// depends only on the app ID. Saying yes records it, so it shows once; a
    /// failed save says no, and the next paste tries again.
    pub fn take_plugin_note(&self, app: &AppId) -> bool {
        if !has_citation_plugins(app) || self.current().is_ok_and(|file| file.recovery) {
            return false;
        }
        let app = normalized(app);
        self.change(|apps| match apps.iter_mut().find(|entry| entry.app == app) {
            Some(entry) if entry.noted => Err(io::ErrorKind::AlreadyExists.into()),
            Some(entry) => {
                entry.noted = true;
                Ok(())
            }
            None => {
                apps.push(Remembered {
                    app,
                    output: (),
                    paste: None,
                    identity: None,
                    noted: true,
                });
                Ok(())
            }
        })
        .is_ok()
    }

    /// The saved file, or an empty one that needs recovery when what is saved
    /// is not a file Ibis wrote. A read that fails for another reason, such as
    /// a moment's sharing violation on Windows, is an error and changes nothing.
    fn current(&self) -> io::Result<File> {
        match read(&self.path) {
            Ok(file) => Ok(file),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(File::default()),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => Ok(File {
                recovery: true,
                ..File::default()
            }),
            Err(error) => Err(error),
        }
    }

    /// Read, change and replace the file while holding its lock, so two
    /// processes changing different apps both keep their change.
    fn change(&self, change: impl FnOnce(&mut Vec<Remembered>) -> io::Result<()>) -> io::Result<()> {
        if let Some(directory) = self.path.parent().filter(|directory| !directory.as_os_str().is_empty()) {
            fs::create_dir_all(directory)?;
        }
        let lock = open_private(&self.path.with_extension("json.lock"))?;
        lock.lock()?;
        let mut file = self.current()?;
        change(&mut file.apps)?;
        let excess = file.apps.len().saturating_sub(MAX_APPS);
        file.apps.drain(..excess);
        write(&self.path, &file)
    }
}

fn read(path: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut text = String::new();
    fs::File::open(path)?.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    let mut file: File = serde_json::from_str(&text).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if file.version != VERSION || file.apps.len() > MAX_APPS {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut ids = std::collections::HashSet::new();
    for entry in &mut file.apps {
        entry.app = normalized(&entry.app);
        if entry.app.id.is_empty() || entry.app.id.chars().any(char::is_control) || !ids.insert(entry.app.clone()) {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
        if entry
            .identity
            .as_ref()
            .is_some_and(|identity| identity.len() != 64 || !identity.bytes().all(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    }
    Ok(file)
}

/// Write the whole file beside the old one, then swap it in, so a crash
/// leaves either file whole.
fn write(path: &Path, file: &File) -> io::Result<()> {
    let json = serde_json::to_string_pretty(&File {
        version: VERSION,
        apps: file.apps.clone(),
        recovery: file.recovery,
    })?;
    if json.len() as u64 + 1 > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
    }
    AtomicFile::new(path, format!("{json}\n").as_bytes())?.replace()
}

#[cfg(test)]
mod tests;
