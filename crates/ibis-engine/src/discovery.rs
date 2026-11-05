//! Setup discovery owns native paths and one-use, expiring choices.
use crate::{
    Cancellation,
    health::{InspectionPolicy, bounded},
};
use ibis_core::{
    contract::{EngineError, ErrorCode},
    zotero::api::{self, Presence},
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::SystemTime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoteroPresence {
    Available,
    AccessOff,
    Closed,
    Absent,
}

pub fn zotero_presence(
    policy: InspectionPolicy,
    transport: &mut impl api::Transport,
    installed: impl FnOnce() -> bool,
    cancel: &Cancellation,
) -> Result<ZoteroPresence, EngineError> {
    if policy != InspectionPolicy::Setup {
        return Err(ErrorCode::BadInput.into());
    }
    cancel.check()?;
    let found = api::presence(transport);
    cancel.check()?;
    let found = match found {
        Presence::Available => ZoteroPresence::Available,
        Presence::AccessOff => ZoteroPresence::AccessOff,
        Presence::Absent if installed() => ZoteroPresence::Closed,
        Presence::Absent => ZoteroPresence::Absent,
    };
    cancel.check()?;
    Ok(found)
}

pub fn find_zotero(
    policy: InspectionPolicy,
    include_closed: bool,
    cancel: &Cancellation,
) -> Result<ZoteroPresence, EngineError> {
    if policy != InspectionPolicy::Setup {
        return Err(ErrorCode::BadInput.into());
    }
    let result = bounded(cancel, move |token| {
        zotero_presence(
            policy,
            &mut ibis_sources::zotero_source::LocalZotero::default(),
            || include_closed && ibis_sources::zotero_source::installed(),
            &token,
        )
    });
    cancel.check()?;
    result.ok_or(EngineError::new(ErrorCode::SourceUnavailable))?
}

#[derive(Default)]
struct Offered {
    list: u32,
    paths: Vec<PathBuf>,
    cancel: Cancellation,
}
#[derive(Clone, Default)]
pub struct Discovery(Arc<Mutex<Offered>>);
pub struct Request {
    owner: Discovery,
    list: u32,
    cancel: Cancellation,
}
impl Discovery {
    pub fn reserve(&self) -> Result<Request, EngineError> {
        let mut offered = self.0.lock().map_err(|_| EngineError::new(ErrorCode::Internal))?;
        let list = offered
            .list
            .checked_add(1)
            .ok_or(EngineError::new(ErrorCode::ResourceLimit))?;
        offered.cancel.cancel();
        *offered = Offered {
            list,
            ..Offered::default()
        };
        Ok(Request {
            owner: self.clone(),
            list,
            cancel: offered.cancel.clone(),
        })
    }
    pub fn resolve(&self, list: u32, index: u32) -> Option<PathBuf> {
        let offered = self.0.lock().ok()?;
        (offered.list == list)
            .then(|| offered.paths.get(index as usize).cloned())
            .flatten()
    }
}
impl Request {
    pub fn find(
        &self,
        policy: InspectionPolicy,
        roots: Vec<PathBuf>,
    ) -> Result<(ZoteroPresence, Vec<PathBuf>, bool), EngineError> {
        if policy != InspectionPolicy::Setup {
            return Err(ErrorCode::BadInput.into());
        }
        let zotero = find_zotero(policy, true, &self.cancel)?;
        let found = bounded(&self.cancel, move |token| search(&roots, 20_000, &token));
        self.cancel.check()?;
        let (files, cut_short) = found.ok_or(EngineError::new(ErrorCode::SourceUnavailable))?;
        Ok((zotero, files, cut_short))
    }
    /// The caller formats names natively; the paths stay in this request's owner.
    pub fn publish<T>(&self, paths: Vec<PathBuf>, shown: impl Fn(&Path) -> T) -> Result<(u32, Vec<T>), EngineError> {
        let mut offered = self.owner.0.lock().map_err(|_| EngineError::new(ErrorCode::Internal))?;
        self.cancel.check()?;
        if offered.list != self.list {
            return Err(ErrorCode::Cancelled.into());
        }
        let files = paths.iter().map(|path| shown(path)).collect();
        offered.paths = paths;
        self.cancel.cancel();
        Ok((self.list, files))
    }
}

/// Newest first, at most 20 files; each root has its own entry budget. No links
/// are followed and no file contents are read. Cancellation guards each OS call.
fn search(roots: &[PathBuf], budget: usize, cancel: &Cancellation) -> (Vec<PathBuf>, bool) {
    let mut files: Vec<(Option<SystemTime>, PathBuf)> = Vec::new();
    let mut cut_short = false;
    for root in roots {
        let mut left = budget;
        let mut folders = vec![(root.clone(), 0)];
        while let Some((folder, depth)) = folders.pop() {
            if left == 0 || cancel.is_cancelled() {
                break;
            }
            let Ok(mut entries) = std::fs::read_dir(folder) else {
                continue;
            };
            let mut children = Vec::new();
            while left > 0 && !cancel.is_cancelled() {
                let Some(entry) = entries.next() else { break };
                let Ok(entry) = entry else { continue };
                if cancel.is_cancelled() {
                    break;
                }
                left -= 1;
                let name = entry.file_name();
                let Some(name) = name.to_str().filter(|name| !name.starts_with('.')) else {
                    continue;
                };
                let Ok(kind) = entry.file_type() else { continue };
                if cancel.is_cancelled() {
                    break;
                }
                if kind.is_dir()
                    && depth < 3
                    && !matches!(
                        name,
                        "node_modules"
                            | "Library"
                            | "AppData"
                            | "Applications"
                            | "target"
                            | "venv"
                            | "__pycache__"
                            | "site-packages"
                    )
                {
                    children.push((entry.path(), depth + 1));
                } else if kind.is_file()
                    && Path::new(name)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("bib"))
                {
                    files.push((entry.metadata().ok().and_then(|m| m.modified().ok()), entry.path()));
                }
            }
            children.sort();
            folders.extend(children.into_iter().rev());
        }
        cut_short |= left == 0;
    }
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    files.dedup_by(|a, b| a.1 == b.1);
    cut_short |= files.len() > 20;
    files.truncate(20);
    (files.into_iter().map(|(_, path)| path).collect(), cut_short)
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
