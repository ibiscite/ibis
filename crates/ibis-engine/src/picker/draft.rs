//! The citation being built, saved beside the picker's settings so that an
//! engine that restarts, or the private one a shell starts when the daemon is
//! out of reach, picks it up where it was (decision 0065).
//!
//! `picker-draft.json` holds the references added, their page, prefix,
//! suffix and form, the kept order and a format used once. Never the typed
//! search. It is removed once the citation is delivered or emptied, and is
//! ignored when older than [`KEPT_FOR`].

use super::PickerState;
use super::session::{PickerDetails, PickerSession};
use crate::{Engine, request};
use ibis_core::contract::ItemRef;
use ibis_os::atomic_file::AtomicFile;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A draft older than this belongs to a different moment of writing.
pub const KEPT_FOR: Duration = Duration::from_secs(10 * 60);
const MAX_BYTES: u64 = 256 * 1024;
const VERSION: u32 = 1;

/// What a picker session keeps of its citation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PickerDraft {
    pub items: Vec<(ItemRef, PickerDetails)>,
    pub keep_order: bool,
    pub held: Option<String>,
}

impl PickerDraft {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    version: u32,
    at_ms: u64,
    draft: PickerDraft,
}

/// The draft file of one picker profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drafts {
    path: PathBuf,
}

impl Drafts {
    pub fn beside(settings: &Path) -> Self {
        Self {
            path: settings.with_file_name("picker-draft.json"),
        }
    }

    /// Save `draft`, or remove the file when it is empty. Only one picker
    /// is open at a time, so the last engine to write wins.
    pub fn save(&self, draft: &PickerDraft) -> io::Result<()> {
        if draft.is_empty() {
            return match fs::remove_file(&self.path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        let json = serde_json::to_vec(&File {
            version: VERSION,
            at_ms: now_ms(),
            draft: draft.clone(),
        })?;
        if json.len() as u64 > MAX_BYTES {
            return Err(io::ErrorKind::InvalidData.into());
        }
        if let Some(directory) = self.path.parent().filter(|directory| !directory.as_os_str().is_empty()) {
            fs::create_dir_all(directory)?;
        }
        AtomicFile::new(&self.path, &json)?.replace()
    }

    /// The saved draft, when there is a recent one Ibis wrote.
    pub fn load(&self) -> Option<PickerDraft> {
        if !fs::symlink_metadata(&self.path).ok()?.is_file() {
            return None;
        }
        let mut bytes = Vec::new();
        fs::File::open(&self.path)
            .ok()?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .ok()?;
        let file: File = serde_json::from_slice(&bytes).ok()?;
        let fresh = now_ms().saturating_sub(file.at_ms) <= KEPT_FOR.as_millis() as u64;
        (file.version == VERSION && fresh).then_some(file.draft)
    }
}

impl PickerSession {
    /// The citation as a draft to save.
    pub fn draft(&self) -> PickerDraft {
        PickerDraft {
            items: self
                .added()
                .map(|(row, details)| (row.id.clone(), details.clone()))
                .collect(),
            keep_order: self.keeps_order(),
            held: self.held_format().map(str::to_owned),
        }
    }

    /// Take back a saved citation in a new opening and redraw. A reference
    /// the open library no longer has is left out; the others are read afresh.
    pub fn restore(&mut self, engine: &Engine, draft: PickerDraft) -> PickerState {
        let items = draft
            .items
            .into_iter()
            .filter_map(|(id, details)| {
                let item = engine
                    .show(&request::ShowRequest {
                        item: request::ItemSelector::Id(id),
                    })
                    .ok()?;
                Some((super::row(&item, self.language())?, details))
            })
            .collect();
        self.restored(items, draft.keep_order, draft.held);
        self.refresh(engine)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

#[cfg(test)]
mod tests;
