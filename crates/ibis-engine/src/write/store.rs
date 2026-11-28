//! Proposals on disk: one private JSON file each, `proposals/ID.json` beside
//! the app's settings, as recent citations are. Each file is written whole
//! and atomically with Unix mode 0600 in a folder with mode 0700. At most
//! [`MAX_PENDING`] wait and [`MAX_KEPT`] are kept; the oldest decided ones go
//! first. A file that cannot be read is left alone and listed as nothing.
use super::{Change, FORMAT, Proposal, Source, Status, Target, WriteError, WriteProblem};
use ibis_core::contract::Fingerprint;
use ibis_os::atomic_file::AtomicFile;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Proposals waiting for the person at most.
pub const MAX_PENDING: usize = 100;
/// Proposals kept at most, decided ones included.
pub const MAX_KEPT: usize = 200;
/// Bytes one proposal file holds at most.
pub const MAX_PROPOSAL_BYTES: usize = 64 * 1024;
/// Folder entries looked at, so a folder of other files stays cheap.
const MAX_ENTRIES: usize = 4 * MAX_KEPT;
const ID_DIGITS: usize = 16;

/// The folder proposals are kept in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposals {
    dir: PathBuf,
}

impl Proposals {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The folder beside the app's `settings.json`.
    pub fn beside(settings: &Path) -> Self {
        Self::new(settings.with_file_name("proposals"))
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every readable proposal, newest first.
    pub fn list(&self) -> Result<Vec<Proposal>, WriteError> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(_) => return Err(WriteProblem::StoreUnreadable.into()),
        };
        let mut proposals: Vec<Proposal> = entries
            .take(MAX_ENTRIES)
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name();
                let id = name.to_str()?.strip_suffix(".json")?;
                valid_id(id).then(|| self.get(id).ok()).flatten()
            })
            .collect();
        proposals.sort_by(|a, b| b.created_ms.cmp(&a.created_ms).then_with(|| a.id.cmp(&b.id)));
        Ok(proposals)
    }

    /// One proposal by id.
    pub fn get(&self, id: &str) -> Result<Proposal, WriteError> {
        if !valid_id(id) {
            return Err(WriteProblem::UnknownProposal.into());
        }
        let path = self.path(id);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(WriteProblem::UnknownProposal.into());
            }
            Err(_) => return Err(WriteProblem::ProposalUnreadable.into()),
        };
        if !metadata.is_file() || metadata.len() > MAX_PROPOSAL_BYTES as u64 {
            return Err(WriteProblem::ProposalUnreadable.into());
        }
        let mut bytes = Vec::new();
        fs::File::open(&path)
            .and_then(|file| file.take(MAX_PROPOSAL_BYTES as u64 + 1).read_to_end(&mut bytes))
            .map_err(|_| WriteProblem::ProposalUnreadable)?;
        let proposal: Proposal = serde_json::from_slice(&bytes).map_err(|_| WriteProblem::ProposalUnreadable)?;
        if proposal.ibis_proposal != FORMAT || proposal.id != id || bytes.len() > MAX_PROPOSAL_BYTES {
            return Err(WriteProblem::ProposalUnreadable.into());
        }
        Ok(proposal)
    }

    /// Hold this until the returned file drops: one change to proposals or a
    /// library at a time, across Ibis processes. The operating system lets go
    /// of it when the holder ends, so a crash never leaves it held.
    pub(super) fn lock(&self) -> Result<fs::File, WriteError> {
        self.folder()?;
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.dir.join(".lock"))
            .map_err(|_| WriteProblem::NotSaved)?;
        file.lock().map_err(|_| WriteProblem::NotSaved)?;
        Ok(file)
    }

    /// Commit one library replacement and retain its newest backups together.
    /// The apply transaction already holds this store's writer lock.
    pub(super) fn replace_library(
        &self,
        target: &Target,
        before: &[u8],
        after: &[u8],
        check: impl FnOnce() -> Result<(), WriteProblem>,
    ) -> Result<PathBuf, WriteError> {
        let staged = AtomicFile::new(&target.path, after).map_err(|_| WriteProblem::NotWritten)?;
        let backup = self.backup_path(target)?;
        AtomicFile::new(&backup, before)
            .and_then(AtomicFile::create)
            .map_err(|_| WriteProblem::BackupFailed)?;
        if let Err(problem) = check() {
            let _ = fs::remove_file(&backup);
            return Err(problem.into());
        }
        staged.replace().map_err(|_| {
            let _ = fs::remove_file(&backup);
            WriteProblem::NotWritten
        })?;
        self.prune_backups(&backup);
        Ok(backup)
    }

    /// Backups live in Ibis's data folder, isolated by resolved library identity.
    fn backup_path(&self, target: &Target) -> Result<PathBuf, WriteError> {
        let identity = Fingerprint::of_bytes(target.resolved.as_os_str().as_encoded_bytes());
        let dir = self.dir.with_file_name("library-backups").join(identity.as_str());
        private_folder(&dir).map_err(|_| WriteProblem::BackupFailed)?;
        let next = backups(&dir)?
            .last()
            .map_or(Some(1), |(number, _)| number.checked_add(1))
            .ok_or(WriteProblem::BackupFailed)?;
        let mut name = target
            .resolved
            .file_name()
            .ok_or(WriteProblem::BackupFailed)?
            .to_os_string();
        name.push(format!(".ibis-backup-{next}"));
        Ok(dir.join(name))
    }

    /// Only called after replacement, while holding the proposal writer lock.
    fn prune_backups(&self, backup: &Path) {
        if let Some(dir) = backup.parent() {
            match backups(dir) {
                Ok(backups) => {
                    let excess = backups.len().saturating_sub(MAX_BACKUPS);
                    for (_, path) in backups.into_iter().take(excess) {
                        if fs::remove_file(path).is_err() {
                            ibis_os::sync::log(format_args!("Ibis could not remove an old library backup"));
                        }
                    }
                }
                Err(_) => ibis_os::sync::log(format_args!("Ibis could not list old library backups")),
            }
        }
    }

    /// Save a new pending proposal, within the bounds.
    pub(super) fn create(
        &self,
        source: Source,
        target: Target,
        change: Change,
        fingerprint: Option<Fingerprint>,
    ) -> Result<Proposal, WriteError> {
        let _lock = self.lock()?;
        let kept = self.list()?;
        if kept
            .iter()
            .filter(|proposal| proposal.status == Status::Pending)
            .count()
            >= MAX_PENDING
        {
            return Err(WriteProblem::TooMany.into());
        }
        let mut decided: Vec<&Proposal> = kept
            .iter()
            .filter(|proposal| proposal.status != Status::Pending)
            .collect();
        decided.sort_by_key(|proposal| proposal.decided_ms.unwrap_or(proposal.created_ms));
        let mut count = kept.len();
        for old in decided {
            if count < MAX_KEPT {
                break;
            }
            if fs::remove_file(self.path(&old.id)).is_ok() {
                count -= 1;
            }
        }
        if count >= MAX_KEPT {
            return Err(WriteProblem::TooMany.into());
        }
        let created_ms = now_ms();
        let mut proposal = Proposal {
            ibis_proposal: FORMAT,
            id: String::new(),
            created_ms,
            source,
            target,
            change,
            fingerprint,
            status: Status::Pending,
            decided_ms: None,
        };
        let bytes = text(&proposal)?;
        for attempt in 0..8u64 {
            proposal.id = new_id(&bytes, attempt);
            let saved = AtomicFile::new(&self.path(&proposal.id), &text(&proposal)?).and_then(AtomicFile::create);
            match saved {
                Ok(()) => return Ok(proposal),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err(WriteProblem::NotSaved.into()),
            }
        }
        Err(WriteProblem::NotSaved.into())
    }

    /// Save a decision on a proposal.
    pub(super) fn decide(&self, id: &str, status: Status) -> Result<Proposal, WriteError> {
        let mut proposal = self.get(id)?;
        proposal.status = status;
        proposal.decided_ms = Some(now_ms());
        AtomicFile::new(&self.path(id), &text(&proposal)?)
            .and_then(AtomicFile::replace)
            .map_err(|_| WriteProblem::NotSaved)?;
        Ok(proposal)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    /// The folder, private to this person when Ibis makes it.
    fn folder(&self) -> Result<(), WriteError> {
        private_folder(&self.dir).map_err(|_| WriteProblem::NotSaved.into())
    }
}

fn text(proposal: &Proposal) -> Result<Vec<u8>, WriteError> {
    let mut bytes = serde_json::to_vec_pretty(proposal).map_err(|_| WriteProblem::NotSaved)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_PROPOSAL_BYTES {
        return Err(WriteProblem::TooLarge.into());
    }
    Ok(bytes)
}

/// Unique on this computer, and saying nothing about what was proposed.
fn new_id(content: &[u8], attempt: u64) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_nanos())
        .unwrap_or_default();
    let mut seed = Vec::with_capacity(content.len() + 48);
    seed.extend_from_slice(&nanos.to_le_bytes());
    seed.extend_from_slice(&std::process::id().to_le_bytes());
    seed.extend_from_slice(&COUNTER.fetch_add(1, Ordering::Relaxed).to_le_bytes());
    seed.extend_from_slice(&attempt.to_le_bytes());
    seed.extend_from_slice(content);
    Fingerprint::of_bytes(&seed).as_str()[..ID_DIGITS].to_owned()
}

pub(super) fn valid_id(id: &str) -> bool {
    id.len() == ID_DIGITS && id.bytes().all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| u64::try_from(time.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Applied library versions retained per library.
pub const MAX_BACKUPS: usize = 5;

fn private_folder(dir: &Path) -> std::io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    if fs::symlink_metadata(dir)?.is_dir() {
        Ok(())
    } else {
        Err(std::io::ErrorKind::InvalidInput.into())
    }
}

fn backups(dir: &Path) -> Result<Vec<(u64, PathBuf)>, WriteError> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).map_err(|_| WriteProblem::BackupFailed)? {
        let entry = entry.map_err(|_| WriteProblem::BackupFailed)?;
        if let Some(number) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.rsplit_once(".ibis-backup-").map(|(_, number)| number))
            .and_then(|number| number.parse::<u64>().ok())
        {
            found.push((number, entry.path()));
        }
    }
    found.sort_by_key(|(number, _)| *number);
    Ok(found)
}
