//! The owner as a front end sees it: intents in, published statuses out.
//! Libraries are invented files and an invented local Zotero.

use super::*;
use std::sync::mpsc;
use std::sync::mpsc::{Receiver, channel};

type Error = Box<dyn std::error::Error>;

struct Fixture {
    owner: Owner,
    statuses: Receiver<LibraryStatus>,
    settings: Arc<crate::settings::Owner>,
    directory: tempfile::TempDir,
}

fn fixture_with(engine: impl Fn() -> Result<Engine, EngineError> + Send + Sync + 'static) -> Result<Fixture, Error> {
    let directory = tempfile::tempdir()?;
    let settings = Arc::new(crate::settings::Owner::open(directory.path().join("settings.json")));
    let (send, statuses) = channel();
    let owner = Owner::new(settings.clone(), engine, move |status| {
        let _ = send.send(status);
    });
    Ok(Fixture {
        owner,
        statuses,
        settings,
        directory,
    })
}

fn fixture() -> Result<Fixture, Error> {
    fixture_with(|| Ok(Engine::new()?.without_file_watches()))
}

impl Fixture {
    fn bib(&self, name: &str, text: &str) -> Result<PathBuf, Error> {
        let path = self.directory.path().join(name);
        std::fs::write(&path, text)?;
        Ok(path)
    }

    fn open(&self, source: impl Into<Source>, save: Save) {
        self.owner.submit(Intent::Open {
            source: source.into(),
            save,
            profile: None,
        });
    }

    /// The first published status that `done` accepts.
    fn until(&self, done: impl Fn(&LibraryStatus) -> bool) -> Result<LibraryStatus, Error> {
        loop {
            let status = self.statuses.recv_timeout(Duration::from_secs(10))?;
            if done(&status) {
                return Ok(status);
            }
        }
    }

    fn settled(&self) -> Result<LibraryStatus, Error> {
        self.until(|status| !matches!(status.state, LibraryState::Loading | LibraryState::Reloading))
    }

    fn saved(&self) -> Option<LibrarySetting> {
        self.settings.read().cites().ok()?.portable.library.clone()
    }
}

fn file_source(fixture: &Fixture) -> Option<PathBuf> {
    match fixture.owner.view().source()? {
        Source::File(path) => Some(path.clone()),
        _ => None,
    }
}

#[test]
fn a_chosen_file_opens_searchable_and_is_saved_for_next_time() -> Result<(), Error> {
    let fixture = fixture()?;
    let path = fixture.bib("invented.bib", "@book{first,title={Synthetic stars}}")?;
    fixture.open(path.clone(), Save::Always);
    let loading = fixture.until(|status| status.state == LibraryState::Loading)?;
    assert_eq!(loading.name.as_deref(), Some("invented.bib"));
    assert!(!loading.has_snapshot);
    let loaded = fixture.settled()?;
    assert_eq!(loaded.state, LibraryState::Loaded);
    assert_eq!(loaded.references, 1);
    assert!(loaded.checked_at.is_some() && loaded.updated_at.is_none() && loaded.error.is_none());
    let current = fixture.owner.current().ok_or("an open library")?;
    assert_eq!(current.engine.snapshot().map_err(|_| "snapshot")?.objects().len(), 1);
    assert_eq!(loaded.revision, current.revision());
    assert_eq!(fixture.saved(), Some(LibrarySetting::Bibtex(path.canonicalize()?)));
    Ok(())
}

#[test]
fn the_last_of_several_rapid_choices_wins() -> Result<(), Error> {
    let fixture = fixture()?;
    let paths = ["first", "second", "third"]
        .map(|name| fixture.bib(&format!("{name}.bib"), &format!("@misc{{{name},title={{{name}}}}}")));
    for path in &paths {
        fixture.open(path.as_ref().map_err(|e| e.to_string())?.clone(), Save::Always);
    }
    let last =
        fixture.until(|status| status.state == LibraryState::Loaded && status.name.as_deref() == Some("third.bib"))?;
    assert_eq!(last.references, 1);
    assert_eq!(
        file_source(&fixture).and_then(|path| path.file_name().map(|name| name.to_owned())),
        Some("third.bib".into())
    );
    let third = paths[2].as_ref().map_err(|e| e.to_string())?;
    assert_eq!(fixture.saved(), Some(LibrarySetting::Bibtex(third.canonicalize()?)));
    Ok(())
}

#[test]
fn a_failed_switch_keeps_the_open_library_searchable_and_names_the_attempt() -> Result<(), Error> {
    let fixture = fixture()?;
    let kept = fixture.bib("kept.bib", "@misc{kept,title={Kept}}")?;
    fixture.open(kept.clone(), Save::Always);
    fixture.settled()?;
    fixture.open(fixture.directory.path().join("missing.bib"), Save::Always);
    let failed = fixture.until(|status| status.state == LibraryState::Error)?;
    assert_eq!(failed.name.as_deref(), Some("missing.bib"));
    assert_eq!(failed.snapshot_name.as_deref(), Some("kept.bib"));
    assert!(failed.has_snapshot && !failed.source_is_requested && !failed.stale);
    assert_eq!(failed.problem, Some(LibraryProblem::Missing));
    assert_eq!(failed.references, 1);
    assert_eq!(file_source(&fixture).as_deref(), Some(kept.as_path()));
    assert_eq!(fixture.saved(), Some(LibrarySetting::Bibtex(kept.canonicalize()?)));
    Ok(())
}

#[test]
fn closing_leaves_no_library_and_a_late_read_cannot_restore_it() -> Result<(), Error> {
    let fixture = fixture()?;
    let path = fixture.bib("late.bib", "@misc{late,title={Late}}")?;
    fixture.open(path, Save::Always);
    fixture.owner.submit(Intent::Close);
    let closed = fixture.until(|status| status.state == LibraryState::None)?;
    assert!(!closed.has_snapshot && closed.references == 0);
    // Whatever the read finished with, nothing replaces the closed state.
    std::thread::sleep(Duration::from_millis(300));
    assert!(fixture.owner.current().is_none());
    assert_eq!(fixture.owner.status().state, LibraryState::None);
    Ok(())
}

#[test]
fn a_file_not_on_this_computer_is_refused_and_the_open_library_stays() -> Result<(), Error> {
    let fixture = fixture()?;
    let path = fixture.bib("open.bib", "@misc{open,title={Open}}")?;
    fixture.open(path, Save::Always);
    fixture.settled()?;
    fixture.owner.submit(Intent::RefuseNotLocal);
    let refused = fixture.until(|status| status.state == LibraryState::Error)?;
    assert_eq!(refused.problem, Some(LibraryProblem::Other));
    assert_eq!(refused.references, 1);
    assert!(refused.has_snapshot);
    Ok(())
}

struct Unsaved;
impl ChoiceStore for Unsaved {
    fn zotero_profile(&self) -> Result<Option<String>, EngineError> {
        Ok(None)
    }
    fn save(&self, _: &mut dyn FnMut(&mut Settings)) -> Result<(), String> {
        Err("invented disk failure".into())
    }
}

#[test]
fn a_choice_that_cannot_be_saved_opens_and_says_so_once() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("unsaved.bib");
    std::fs::write(&path, "@misc{unsaved,title={Unsaved}}")?;
    let (send, statuses) = channel();
    let owner = Owner::new(
        Arc::new(Unsaved),
        || Ok(Engine::new()?.without_file_watches()),
        move |status| {
            let _ = send.send(status);
        },
    );
    owner.submit(Intent::Open {
        source: path.into(),
        save: Save::Always,
        profile: None,
    });
    let shown = loop {
        let status = statuses.recv_timeout(Duration::from_secs(10))?;
        if status.reveal {
            break status;
        }
    };
    assert_eq!(shown.state, LibraryState::Loaded);
    assert!(
        shown
            .note
            .as_deref()
            .is_some_and(|note| note.contains("could not be saved for next time"))
    );
    assert!(
        !owner.status().reveal,
        "only the publication right after the failure asks to be seen"
    );
    Ok(())
}

#[test]
fn a_stuck_read_gives_way_to_cancellation_and_to_its_deadline() {
    let reads = Pool::new(4, 0);
    let cancel = OpenCancellation::default();
    let (release, held) = mpsc::channel::<()>();
    let waiting = {
        let cancel = cancel.clone();
        let reads = reads.clone();
        std::thread::spawn(move || {
            supervise_with(
                &cancel,
                Duration::from_secs(30),
                &reads,
                LoadError::FILE_UNAVAILABLE,
                move || {
                    let _ = held.recv();
                    Ok(())
                },
            )
        })
    };
    std::thread::sleep(Duration::from_millis(50));
    cancel.cancel();
    assert_eq!(waiting.join().ok(), Some(Err(LoadError::Stopped)));
    let deadline = supervise_with(
        &OpenCancellation::default(),
        Duration::from_millis(50),
        &reads,
        LoadError::FILE_UNAVAILABLE,
        || {
            std::thread::sleep(Duration::from_secs(1));
            Ok(())
        },
    );
    assert_eq!(deadline, Err(LoadError::FILE_UNAVAILABLE));
    drop(release);
}
