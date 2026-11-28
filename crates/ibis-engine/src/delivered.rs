//! Each delivery the person asked for pastes at most once, whichever process
//! answers it and however often a lost reply makes a shell ask again.
//!
//! A shell names every Return with a new [`DeliveryId`] and keeps it when it
//! asks again. Before Ibis grants a paste it claims that id in
//! `deliveries.json`, beside `settings.json`, under a lock every Ibis process
//! shares; the claim is on disk before the answer leaves. Only the first claim
//! of an id may paste: asking again can at most copy, which is harmless. The
//! file holds ids and times only, at most [`MAX_KEPT`], none older than
//! [`KEPT_FOR`].

use ibis_os::atomic_file::{AtomicFile, lock_within, open_private};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Deliveries remembered at most; the oldest goes first.
pub const MAX_KEPT: usize = 64;
/// A shell that has not heard back by then has told the person already.
pub const KEPT_FOR: Duration = Duration::from_secs(60 * 60);
const MAX_BYTES: u64 = 64 * 1024;
/// A claim waits this long for another process's claim, then this delivery copies.
const LOCK_WAIT: Duration = Duration::from_secs(2);
const VERSION: u32 = 1;

/// A shell's name for one Return: 32 lowercase hexadecimal digits, random.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct DeliveryId(String);

impl TryFrom<String> for DeliveryId {
    type Error = &'static str;
    fn try_from(id: String) -> Result<Self, Self::Error> {
        if id.len() == 32 && id.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')) {
            Ok(Self(id))
        } else {
            Err("a delivery id is 32 lowercase hexadecimal digits")
        }
    }
}

impl From<DeliveryId> for String {
    fn from(id: DeliveryId) -> Self {
        id.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    id: DeliveryId,
    at_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    entries: Vec<Entry>,
}

/// The deliveries of one profile. Holds no state of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deliveries {
    path: PathBuf,
}

impl Deliveries {
    pub fn open(path: PathBuf) -> Self {
        Self { path }
    }

    /// The file beside a profile's `settings.json`.
    pub fn beside(settings: &Path) -> Self {
        Self::open(settings.with_file_name("deliveries.json"))
    }

    /// Claim `id` before granting its paste: true only the first time, once
    /// the claim is on disk. False or an error means this delivery must not
    /// paste. An unreadable file may have held the claim, so the delivery that
    /// finds one does not paste either; the file is replaced for the next.
    pub fn claim(&self, id: &DeliveryId) -> io::Result<bool> {
        if let Some(directory) = self.path.parent().filter(|directory| !directory.as_os_str().is_empty()) {
            fs::create_dir_all(directory)?;
        }
        let lock = open_private(&self.path.with_extension("json.lock"))?;
        lock_within(&lock, LOCK_WAIT)?;
        let (mut file, lost) = match read(&self.path) {
            Ok(file) => (file, false),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (File::default(), false),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => (File::default(), true),
            Err(error) => return Err(error),
        };
        let now = now_ms();
        let oldest = now.saturating_sub(KEPT_FOR.as_millis() as u64);
        file.entries.retain(|entry| entry.at_ms >= oldest);
        if file.entries.iter().any(|entry| entry.id == *id) {
            return Ok(false);
        }
        file.entries.push(Entry {
            id: id.clone(),
            at_ms: now,
        });
        let excess = file.entries.len().saturating_sub(MAX_KEPT);
        file.entries.drain(..excess);
        let json = serde_json::to_string_pretty(&File {
            version: VERSION,
            entries: file.entries,
        })?;
        AtomicFile::new(&self.path, format!("{json}\n").as_bytes())?.replace()?;
        Ok(!lost)
    }
}

fn read(path: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let mut text = String::new();
    fs::File::open(path)?.take(MAX_BYTES + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_BYTES {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let file: File = serde_json::from_str(&text).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
    if file.version != VERSION || file.entries.len() > MAX_KEPT {
        return Err(io::ErrorKind::InvalidData.into());
    }
    Ok(file)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

#[cfg(test)]
mod tests;
