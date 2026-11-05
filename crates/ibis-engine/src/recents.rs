//! Bounded recent identities on disk; session references and undo only in memory.
use crate::MAX_CITATION_ITEMS;
use ibis_core::contract::{ItemRef, LibraryObject, Snapshot, SnapshotObject};
use std::{
    io::{self, Read},
    path::PathBuf,
    sync::{Arc, Mutex},
};

const MAX_UNDO: usize = 20;

/// Undo restores removed identities without discarding newer citations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undo {
    Removed { item: ItemRef, at: usize },
    Reset { items: Vec<ItemRef>, overflowed: bool },
}

#[derive(Debug, Default)]
pub struct List {
    path: Option<PathBuf>,
    entries: Vec<(String, Vec<ItemRef>)>,
    enabled: bool,
    pub forget_failed: bool,
    epoch: Arc<()>,
    /// In the order first cited; each record once.
    pub items: Vec<ItemRef>,
    pub undo: Vec<Undo>,
    /// References left off because the list reached the citation limit.
    pub overflowed: bool,
    pub revision: u32,
}

impl List {
    fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn add(&mut self, cited: impl IntoIterator<Item = ItemRef>) {
        for item in cited {
            if !self.items.contains(&item) {
                if self.items.len() >= MAX_CITATION_ITEMS {
                    self.overflowed = true;
                } else {
                    self.items.push(item);
                }
                self.changed();
            }
        }
    }

    fn push_undo(&mut self, undo: Undo) {
        if self.undo.len() >= MAX_UNDO {
            self.undo.remove(0);
        }
        self.undo.push(undo);
    }

    pub fn remove(&mut self, item: &ItemRef) {
        if let Some(at) = self.items.iter().position(|existing| existing == item) {
            let item = self.items.remove(at);
            self.push_undo(Undo::Removed { item, at });
            self.changed();
        }
    }

    pub fn reset(&mut self) {
        if !self.items.is_empty() {
            let items = std::mem::take(&mut self.items);
            let overflowed = std::mem::take(&mut self.overflowed);
            self.push_undo(Undo::Reset { items, overflowed });
            self.changed();
        }
    }

    pub fn undo(&mut self) {
        let (mut restored, overflowed) = match self.undo.pop() {
            Some(Undo::Removed { item, at }) if !self.items.contains(&item) => {
                let mut restored = self.items.clone();
                restored.insert(at.min(restored.len()), item);
                (restored, false)
            }
            Some(Undo::Reset { items, overflowed }) => (items, overflowed),
            _ => return,
        };
        let newer = std::mem::take(&mut self.items);
        let mut room = MAX_CITATION_ITEMS - newer.len();
        restored.retain(|item| {
            if newer.contains(item) {
                return true;
            }
            let restore = room > 0;
            room = room.saturating_sub(1);
            self.overflowed |= !restore;
            restore
        });
        self.overflowed |= overflowed;
        self.items = restored;
        self.add(newer);
        self.changed();
    }
}

#[derive(Default)]
pub struct SessionReferences(Mutex<List>);

impl SessionReferences {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, List> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Record a citation that reached the clipboard.
    pub fn cited(&self, items: Vec<ItemRef>) {
        self.lock().add(items);
    }

    pub fn items(&self) -> Vec<ItemRef> {
        self.lock().items.clone()
    }
}

const MAX_BYTES: usize = 256 * 1024;
fn valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}
impl List {
    pub fn open(path: PathBuf, enabled: bool) -> Self {
        let entries = (|| {
            if !path.is_file() {
                return None;
            }
            let mut reader = std::fs::File::open(&path).ok()?.take(MAX_BYTES as u64 + 1);
            let entries: Vec<(String, Vec<ItemRef>)> = serde_json::from_reader(io::BufReader::new(&mut reader)).ok()?;
            if reader.limit() == 0 {
                return None;
            }
            (entries.len() <= 20
                && entries.iter().enumerate().all(|(i, (app, items))| {
                    valid(app)
                        && entries[..i].iter().all(|(other, _)| other != app)
                        && items.len() <= 30
                        && items
                            .iter()
                            .enumerate()
                            .all(|(i, item)| valid(&item.key) && !items[..i].contains(item))
                }))
            .then_some(entries)
        })()
        .unwrap_or_default();
        let mut history = Self {
            path: Some(path),
            entries,
            enabled,
            ..Self::default()
        };
        if !enabled {
            let _ = history.forget();
        }
        history
    }
    pub fn epoch(&self) -> Arc<()> {
        Arc::clone(&self.epoch)
    }
    pub fn has_history(&self) -> bool {
        !self.entries.is_empty()
    }
    pub fn ids(&self, app: Option<&str>) -> &[ItemRef] {
        self.entries
            .iter()
            .find(|(id, _)| Some(id.as_str()) == app)
            .map_or(&[], |(_, items)| items)
    }
    pub fn set_enabled(&mut self, enabled: bool) -> io::Result<()> {
        self.enabled = enabled;
        if enabled { Ok(()) } else { self.forget() }
    }
    pub fn forget(&mut self) -> io::Result<()> {
        self.epoch = Arc::new(());
        self.entries.clear();
        self.save().inspect_err(|_| self.forget_failed = true)
    }
    pub fn record(&mut self, epoch: &Arc<()>, app: Option<&str>, items: &[ItemRef]) -> io::Result<()> {
        let Some(app) = app.filter(|app| valid(app)) else {
            return Ok(());
        };
        if !self.enabled || !Arc::ptr_eq(epoch, &self.epoch) {
            return Ok(());
        }
        let mut recent = Vec::new();
        for item in items.iter().chain(self.ids(Some(app))) {
            if valid(&item.key) && !recent.contains(item) && recent.len() < 30 {
                recent.push(item.clone());
            }
        }
        if recent.is_empty() {
            return Ok(());
        }
        self.entries.retain(|(id, _)| id != app);
        self.entries.insert(0, (app.into(), recent));
        self.entries.truncate(20);
        self.save()
    }
    fn save(&mut self) -> io::Result<()> {
        let mut text = serde_json::to_string(&self.entries)?;
        while text.len() > MAX_BYTES {
            self.entries.pop();
            text = serde_json::to_string(&self.entries)?;
        }
        let saved = self
            .path
            .as_ref()
            .map_or(Ok(()), |path| ibis_core::settings::backup::save(path, &text, true));
        self.forget_failed &= saved.is_err();
        saved
    }
}

/// Resolve current records, then source-added records, without repeating rows.
pub fn resolve<'a>(snapshot: &'a Snapshot, ids: &[ItemRef], include_added: bool) -> Vec<(&'a SnapshotObject, bool)> {
    let mut picked: Vec<(&SnapshotObject, bool)> = Vec::new();
    for id in ids {
        if let Some(object) = snapshot.get(id)
            && matches!(object.object, LibraryObject::Record(_))
            && !picked.iter().any(|(object, _)| object.object.id() == id)
            && picked.len() < 3
        {
            picked.push((object, true));
        }
    }
    if include_added {
        let mut added = Vec::new();
        for object in snapshot.objects() {
            if let LibraryObject::Record(record) = &object.object
                && !picked.iter().any(|(picked, _)| picked.object.id() == &record.id)
                && let Some(date) = record.added_at.as_deref().and_then(ibis_core::contract::added_date)
            {
                added.push((date, object.as_ref()));
            }
        }
        added.sort_by(|(a, ar), (b, br)| b.cmp(a).then_with(|| ar.object.id().cmp(br.object.id())));
        picked.extend(added.into_iter().take(3).map(|(_, object)| (object, false)));
    }
    picked
}
#[cfg(test)]
mod tests;
