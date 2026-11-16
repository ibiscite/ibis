//! Read-only bibliography observations and guarded citation context.
use crate::{KeyCheck, ProjectProblem, ProjectRead, project_name};
use ibis_core::contract::{CitedWarning, Message};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectKeyState {
    In,
    Missing,
    CouldNotCheck,
    Repeated,
    MayNameAnother,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ProjectMark {
    pub generation: u32,
    pub state: ProjectKeyState,
    /// Where to look in the file, when the state asks a person to look.
    pub lines: Vec<u32>,
}

impl ProjectMark {
    pub fn new(generation: u32, check: &crate::KeyCheck) -> Self {
        use crate::KeyCheck;
        let (state, lines) = match check {
            KeyCheck::In { .. } => (ProjectKeyState::In, vec![]),
            KeyCheck::Missing => (ProjectKeyState::Missing, vec![]),
            KeyCheck::CouldNotCheck => (ProjectKeyState::CouldNotCheck, vec![]),
            KeyCheck::Repeated { lines } => (ProjectKeyState::Repeated, lines.clone()),
            KeyCheck::MayNameAnother { line, .. } => (ProjectKeyState::MayNameAnother, vec![*line]),
        };
        Self {
            generation,
            state,
            lines: lines.into_iter().take(5).map(|line| line as u32).collect(),
        }
    }
}

/// State of the current bibliography read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ProjectState {
    /// No answer yet for this choice.
    Reading,
    /// Every entry could be read.
    Ready,
    /// Part of the file could not be read, so nothing can be called missing.
    Damaged,
    /// The newest read failed.
    Failed,
}

/// What the picker may show. A file name, never a folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct ProjectStatus {
    pub generation: u32,
    /// Counts reads that changed an answer, which can happen with the status
    /// otherwise the same, as when an entry is added to a healthy file.
    pub revision: u32,
    pub name: String,
    pub state: ProjectState,
    /// Why the newest read failed, as a sentence ending in a full stop.
    pub reason: Option<String>,
    /// The first lines that could not be read.
    pub damaged_lines: Vec<u32>,
}

/// The newest read of the current choice, for one reply to use throughout.
#[derive(Clone)]
pub struct Current {
    pub generation: u32,
    pub name: String,
    pub read: ProjectRead,
}

struct Ticket {
    generation: u32,
    path: PathBuf,
    previous: Option<ProjectRead>,
}

#[derive(Default)]
struct Slot {
    generation: u32,
    revision: u32,
    path: Option<PathBuf>,
    read: Option<ProjectRead>,
    reading: bool,
    context: Option<String>,
    /// The bound receipt, its project observation and whether its keys are checked.
    prepared: Option<(String, (u32, u32), bool)>,
    output: Option<super::output::OutputRef>,
    /// The choices being read now: the current one, and at most one replaced
    /// choice whose read has not finished yet.
    readers: Vec<u32>,
    pending_read: Option<Ticket>,
}

#[derive(Clone)]
pub struct ProjectBibliography {
    state: std::sync::Arc<Mutex<Slot>>,
    retire: std::sync::Arc<dyn Fn() + Send + Sync>,
    publish: std::sync::Arc<dyn Fn((u32, u32)) + Send + Sync>,
}
impl Default for ProjectBibliography {
    fn default() -> Self {
        Self::new(|| {}, |_| {})
    }
}

impl ProjectBibliography {
    pub fn new(
        retire: impl Fn() + Send + Sync + 'static,
        publish: impl Fn((u32, u32)) + Send + Sync + 'static,
    ) -> Self {
        Self {
            state: Default::default(),
            retire: std::sync::Arc::new(retire),
            publish: std::sync::Arc::new(publish),
        }
    }
    pub fn activate(&self, context: String, path: Option<PathBuf>, output: Option<super::output::OutputRef>) {
        let ticket = {
            let mut slot = self.lock();
            if slot.context.as_ref() == Some(&context) && slot.path == path && slot.output == output {
                return;
            }
            slot.context = Some(context);
            slot.generation = slot.generation.wrapping_add(1);
            slot.path = path;
            slot.output = output;
            slot.read = None;
            slot.reading = false;
            slot.prepared = None;
            slot.pending_read = None;
            (self.retire)();
            Self::begin(&mut slot)
        };
        (self.publish)(self.version());
        self.start_read(ticket);
    }
    pub fn recheck(&self) {
        let ticket = self.check_again();
        if ticket.is_some() {
            (self.publish)(self.version());
        }
        self.start_read(ticket);
    }
    fn start_read(&self, ticket: Option<Ticket>) {
        let Some(mut ticket) = ticket.and_then(|ticket| self.enqueue_read(ticket)) else {
            return;
        };
        let project = self.clone();
        std::thread::spawn(move || {
            loop {
                let read = ProjectRead::read(&ticket.path, ticket.previous.as_ref());
                if project
                    .finish_with(ticket.generation, read, || (project.retire)())
                    .is_some()
                {
                    (project.publish)(project.version());
                }
                match project.next_read(ticket.generation) {
                    Some(next) => ticket = next,
                    None => break,
                }
            }
        });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Slot> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Replace the choice. Nothing from the old file carries over.
    #[cfg(test)]
    fn choose(&self, path: Option<PathBuf>) -> Option<Ticket> {
        let mut slot = self.lock();
        slot.generation = slot.generation.wrapping_add(1);
        slot.path = path;
        slot.read = None;
        slot.reading = false;
        slot.prepared = None;
        slot.pending_read = None;
        Self::begin(&mut slot)
    }

    /// A replaced choice's read never holds up the current one, which starts
    /// beside it. At most two reads run and one latest choice waits, so quick
    /// switching cannot keep more than one discarded bibliography parse alive.
    fn enqueue_read(&self, ticket: Ticket) -> Option<Ticket> {
        let mut slot = self.lock();
        if ticket.generation != slot.generation {
            return None;
        }
        if slot.readers.len() < 2 && !slot.readers.contains(&ticket.generation) {
            slot.readers.push(ticket.generation);
            Some(ticket)
        } else {
            slot.pending_read = Some(ticket);
            None
        }
    }

    /// The reader of `finished` takes the waiting choice, if it may start now.
    fn next_read(&self, finished: u32) -> Option<Ticket> {
        let mut slot = self.lock();
        if let Some(index) = slot.readers.iter().position(|reading| *reading == finished) {
            slot.readers.swap_remove(index);
        }
        let current = slot.generation;
        let next = slot.pending_read.take().filter(|ticket| ticket.generation == current)?;
        if slot.readers.contains(&current) {
            slot.pending_read = Some(next);
            return None;
        }
        slot.readers.push(current);
        Some(next)
    }

    pub fn version(&self) -> (u32, u32) {
        let slot = self.lock();
        (slot.generation, slot.revision)
    }

    pub fn capture(
        &self,
        expected: Option<(u32, u32)>,
        output: Option<super::output::OutputRef>,
    ) -> Result<((u32, u32), Option<Current>), super::Error> {
        let slot = self.lock();
        let version = (slot.generation, slot.revision);
        // An explicit editor owns its output and never borrows this bibliography.
        let Some(output) = output else {
            return Ok((version, None));
        };
        let valid = !slot
            .context
            .as_ref()
            .is_some_and(|context| context.starts_with("unavailable:"))
            && !(matches!(output.spec(), ibis_core::contract::OutputSpec::Key { .. })
                && slot.path.is_some()
                && (slot.read.is_none() || slot.reading))
            && expected == Some(version)
            && slot.output.is_none_or(|current| current == output);
        if !valid {
            return Err(super::Error::Cancelled);
        }
        Ok((version, Self::current_in(&slot)))
    }

    /// A native receipt belongs to one immutable project observation, including
    /// the state with no bibliography. Equal output after A → B → A is new work.
    pub fn bind_preview(
        &self,
        engine: &crate::Engine,
        preview: &ibis_core::contract::Preview,
        version: (u32, u32),
    ) -> Result<(), super::Error> {
        let mut slot = self.lock();
        if (slot.generation, slot.revision) != version {
            return Err(super::Error::SnapshotChanged);
        }
        // The engine validates its own identity while the project binding is locked.
        // A late native completion cannot replace a newer receipt's context.
        let citation = engine.citations().prepared(
            crate::citations::SessionId::PICKER,
            &preview.ticket,
            &preview.content_hash,
        )?;
        slot.prepared = Some((preview.ticket.clone(), version, citation.uses_citation_keys()));
        Ok(())
    }

    /// Project changes and clipboard commits share this lock. Warnings and
    /// content are committed under the same context, before any clipboard I/O.
    /// A key citation waits for a read in progress: the answer it holds may be
    /// the one that read replaces.
    pub fn commit<T, E: From<super::Error>>(
        &self,
        ticket: &str,
        write: impl FnOnce(Option<Current>) -> Result<T, E>,
    ) -> Result<T, E> {
        let slot = self.lock();
        let valid = slot.prepared.as_ref().is_some_and(|(prepared, version, keys)| {
            prepared == ticket && *version == (slot.generation, slot.revision) && !(*keys && slot.reading)
        });
        if !valid {
            return Err(super::Error::SnapshotChanged.into());
        }
        write(Self::current_in(&slot))
    }

    /// Read the current choice again, unless a read of it is running.
    fn check_again(&self) -> Option<Ticket> {
        let mut slot = self.lock();
        if slot.reading { None } else { Self::begin(&mut slot) }
    }

    fn begin(slot: &mut Slot) -> Option<Ticket> {
        let path = slot.path.clone()?;
        slot.reading = true;
        Some(Ticket {
            generation: slot.generation,
            path,
            previous: slot.read.clone(),
        })
    }

    /// Install a finished read, if its choice is still current. True when that
    /// changed an answer; an unchanged file asks nothing again.
    fn finish_with(&self, generation: u32, read: ProjectRead, retire_delivery: impl FnOnce()) -> Option<bool> {
        let mut slot = self.lock();
        if slot.generation != generation {
            return None;
        }
        slot.reading = false;
        let unchanged = slot.read.as_ref().is_some_and(|old| old.same_answers(&read));
        if !unchanged {
            // The same project→delivery boundary guards clipboard commits and
            // final native posting, including workers that already took a proof.
            retire_delivery();
        }
        slot.read = Some(read);
        if !unchanged {
            slot.revision = slot.revision.wrapping_add(1);
        }
        Some(!unchanged)
    }

    #[cfg(test)]
    fn finish(&self, generation: u32, read: ProjectRead) -> bool {
        self.finish_with(generation, read, || {}) == Some(true)
    }

    pub fn status(&self) -> Option<ProjectStatus> {
        let slot = self.lock();
        Self::status_in(&slot)
    }

    pub(super) fn observation(&self) -> ((u32, u32), Option<ProjectStatus>) {
        let slot = self.lock();
        ((slot.generation, slot.revision), Self::status_in(&slot))
    }

    fn status_in(slot: &Slot) -> Option<ProjectStatus> {
        let name = project_name(slot.path.as_deref()?);
        // A check of a file already read also reads: its old answer may not stand.
        let state = match &slot.read {
            _ if slot.reading => ProjectState::Reading,
            None => ProjectState::Reading,
            Some(read) if read.problem().is_some() => ProjectState::Failed,
            Some(read) if read.bibliography().is_some_and(|read| !read.complete()) => ProjectState::Damaged,
            Some(_) => ProjectState::Ready,
        };
        Some(ProjectStatus {
            generation: slot.generation,
            revision: slot.revision,
            name,
            state,
            reason: slot
                .read
                .as_ref()
                .and_then(ProjectRead::problem)
                .map(|problem| reason(problem).to_owned()),
            damaged_lines: slot.read.as_ref().map_or_else(Vec::new, |read| {
                read.damaged_lines().iter().take(5).map(|line| *line as u32).collect()
            }),
        })
    }

    /// The read a reply should use; `None` until the choice has one.
    pub fn current(&self) -> Option<Current> {
        let slot = self.lock();
        Self::current_in(&slot)
    }

    fn current_in(slot: &Slot) -> Option<Current> {
        Some(Current {
            generation: slot.generation,
            name: project_name(slot.path.as_deref()?),
            read: slot.read.clone()?,
        })
    }

    pub fn commit_generation<T>(&self, generation: u32, write: impl FnOnce() -> T) -> Option<T> {
        let slot = self.lock();
        (slot.generation == generation).then(write)
    }
}

fn reason(problem: ProjectProblem) -> &'static str {
    match problem {
        ProjectProblem::Unavailable => "It is not there, or not on this computer yet.",
        ProjectProblem::NotAFile => "It is not a file.",
        ProjectProblem::TooLarge => "It is larger than 64 MiB.",
        ProjectProblem::NotUtf8 => "It is not UTF-8 text.",
        ProjectProblem::Changing => "It is still changing.",
        ProjectProblem::NotBibtex => "It must be a BibTeX or BibLaTeX file.",
    }
}

impl Current {
    /// What a row shows. `None` for a reference with no key to compare.
    pub fn mark(&self, check: Option<KeyCheck>) -> Option<ProjectMark> {
        Some(ProjectMark::new(self.generation, &check?))
    }

    /// The warning for cited item `item`, when its key is not plainly in the
    /// file. These join the citation's own warnings, so the picker copies and
    /// stays open, and each shows with its reference.
    pub fn warning(&self, item: usize, key: &str, check: &KeyCheck) -> Option<CitedWarning> {
        let state = match check {
            KeyCheck::In { .. } => return None,
            KeyCheck::Missing => "missing",
            KeyCheck::CouldNotCheck => "could-not-check",
            KeyCheck::Repeated { .. } => "repeated",
            KeyCheck::MayNameAnother { .. } => "may-name-another",
        };
        let args = [("key", key), ("name", &self.name), ("state", state)];
        Some(CitedWarning {
            item: Some(item),
            message: Message {
                id: "app-project-warning".into(),
                args: args.map(|(name, value)| (name.to_owned(), value.to_owned())).into(),
                context: None,
            },
        })
    }
}

pub fn preview_warnings(
    engine: &crate::Engine,
    id: &str,
    hash: &str,
    project: Option<&Current>,
) -> Result<Vec<CitedWarning>, super::Error> {
    let citation = engine
        .citations()
        .prepared(crate::citations::SessionId::PICKER, id, hash)?;
    Ok(project
        .filter(|_| citation.uses_citation_keys())
        .map(|project| {
            citation
                .project_checks(&project.read)
                .into_iter()
                .filter_map(|(item, key)| project.warning(item, &key.key, &key.check))
                .collect()
        })
        .unwrap_or_default())
}
pub fn preview_evidence(
    engine: &crate::Engine,
    id: &str,
    hash: &str,
    project: Option<&Current>,
) -> Result<Option<crate::ProjectAssessment>, super::Error> {
    let citation = engine
        .citations()
        .prepared(crate::citations::SessionId::PICKER, id, hash)?;
    Ok(project
        .filter(|_| citation.uses_citation_keys())
        .map(|project| citation.project_evidence(&project.read)))
}

#[cfg(test)]
#[path = "project_tests.rs"]
mod tests;
