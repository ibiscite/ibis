//! Bounded recent identities on disk; session references and undo only in memory.
use crate::MAX_CITATION_ITEMS;
use ibis_core::contract::{ItemRef, LibraryObject, Snapshot, SnapshotObject};
use std::{
    io::{self, Read},
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
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
    entries: Arc<Vec<(String, Vec<ItemRef>)>>,
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
    pub fn open(path: PathBuf, enabled: bool) -> Self {
        let entries = (|| {
            if !enabled || !path.is_file() {
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
        Self {
            path: Some(path),
            entries: Arc::new(entries),
            enabled,
            forget_failed: !enabled,
            ..Self::default()
        }
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
    fn forget(&mut self) {
        self.epoch = Arc::new(());
        self.entries = Arc::default();
        self.forget_failed = true;
    }
    fn record(&mut self, epoch: &Arc<()>, app: Option<&str>, items: &[ItemRef]) {
        let Some(app) = app.filter(|app| valid(app) && self.enabled && Arc::ptr_eq(epoch, &self.epoch)) else {
            return;
        };
        let mut recent = Vec::new();
        for item in items.iter().chain(self.ids(Some(app))) {
            if valid(&item.key) && !recent.contains(item) && recent.len() < 30 {
                recent.push(item.clone());
            }
        }
        if recent.is_empty() {
            return;
        }
        let entries = Arc::make_mut(&mut self.entries);
        entries.retain(|(id, _)| id != app);
        entries.insert(0, (app.into(), recent));
        entries.truncate(20);
    }
    pub fn add(&mut self, cited: impl IntoIterator<Item = ItemRef>) {
        for item in cited {
            if !self.items.contains(&item) {
                if self.items.len() >= MAX_CITATION_ITEMS {
                    self.overflowed = true;
                } else {
                    self.items.push(item);
                }
                self.revision = self.revision.wrapping_add(1);
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
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn reset(&mut self) {
        if !self.items.is_empty() {
            let items = std::mem::take(&mut self.items);
            let overflowed = std::mem::take(&mut self.overflowed);
            self.push_undo(Undo::Reset { items, overflowed });
            self.revision = self.revision.wrapping_add(1);
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
        self.revision = self.revision.wrapping_add(1);
    }
}

pub struct SessionReferences {
    list: Arc<Mutex<List>>,
    writer: mpsc::SyncSender<Option<mpsc::SyncSender<io::Result<()>>>>,
}
impl Default for SessionReferences {
    fn default() -> Self {
        let list = Arc::new(Mutex::new(List::default()));
        let (writer, work) = mpsc::sync_channel::<Option<mpsc::SyncSender<io::Result<()>>>>(1);
        let state = Arc::clone(&list);
        std::thread::spawn(move || {
            while let Ok(reply) = work.recv() {
                let (path, observed) = {
                    let current = state.lock().unwrap_or_else(|e| e.into_inner());
                    (current.path.clone(), Arc::clone(&current.entries))
                };
                let mut entries = (*observed).clone();
                let saved = save(path, &mut entries);
                let mut current = state.lock().unwrap_or_else(|e| e.into_inner());
                if Arc::ptr_eq(&observed, &current.entries) {
                    current.forget_failed &= saved.is_err();
                    current.entries = Arc::new(entries);
                }
                drop(current);
                if let Some(reply) = reply {
                    let _ = reply.send(saved);
                }
            }
        });
        Self { list, writer }
    }
}
impl SessionReferences {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, List> {
        self.list.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn items(&self) -> Vec<ItemRef> {
        self.lock().items.clone()
    }
    pub fn record(&self, epoch: &Arc<()>, app: Option<&str>, items: &[ItemRef]) {
        self.lock().record(epoch, app, items);
        let _ = self.writer.try_send(None);
    }
    /// Wait for the serialized writer without holding the memory/clipboard lock.
    pub fn flush(&self) -> io::Result<()> {
        let (reply, done) = mpsc::sync_channel(1);
        self.writer.send(Some(reply)).map_err(io::Error::other)?;
        done.recv().map_err(io::Error::other)?
    }
    pub fn flush_timeout(&self, timeout: std::time::Duration) -> io::Result<()> {
        let (reply, done) = mpsc::sync_channel(1);
        self.writer.try_send(Some(reply)).map_err(io::Error::other)?;
        done.recv_timeout(timeout).map_err(io::Error::other)?
    }
    pub fn forget(&self) -> io::Result<()> {
        self.lock().forget();
        self.flush()
    }
    pub fn set_enabled(&self, enabled: bool) -> io::Result<()> {
        self.lock().enabled = enabled;
        if enabled { Ok(()) } else { self.forget() }
    }
}

const MAX_BYTES: usize = 256 * 1024;
fn valid(value: &str) -> bool {
    !value.is_empty() && value.len() <= 512 && !value.chars().any(char::is_control)
}

fn save(path: Option<PathBuf>, entries: &mut Vec<(String, Vec<ItemRef>)>) -> io::Result<()> {
    #[cfg(test)]
    tests::before_save(path.as_deref());
    let mut text = serde_json::to_string(entries)?;
    while text.len() > MAX_BYTES {
        entries.pop();
        text = serde_json::to_string(entries)?;
    }
    path.as_ref()
        .map_or(Ok(()), |path| ibis_core::settings::backup::save(path, &text, true))
}

/// Resolve current records, then source-added records, without repeating rows.
pub fn resolve<'a>(snapshot: &'a Snapshot, ids: &[ItemRef], include_added: bool) -> Vec<(&'a SnapshotObject, bool)> {
    let mut picked: Vec<_> = ids
        .iter()
        .filter_map(|id| snapshot.get(id))
        .filter(|object| matches!(object.object, LibraryObject::Record(_)))
        .take(3)
        .map(|object| (object, true))
        .collect();
    if include_added {
        let mut added: Vec<_> = snapshot
            .objects()
            .iter()
            .filter_map(|object| {
                let LibraryObject::Record(record) = &object.object else {
                    return None;
                };
                let date = record.added_at.as_deref().and_then(ibis_core::contract::added_date)?;
                (!picked.iter().any(|(picked, _)| picked.object.id() == &record.id)).then_some((date, object.as_ref()))
            })
            .collect();
        added.sort_by(|(a, ar), (b, br)| b.cmp(a).then_with(|| ar.object.id().cmp(br.object.id())));
        picked.extend(added.into_iter().take(3).map(|(_, object)| (object, false)));
    }
    picked
}
#[cfg(test)]
mod tests;
