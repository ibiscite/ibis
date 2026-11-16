//! The owner as a front end sees it: intents in, published statuses out.
//! Libraries are invented files and an invented local Zotero.

use super::*;
use std::sync::mpsc::{Receiver, channel};

use crate::service::sources::zotero_fixture;

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
fn a_choice_from_the_settings_file_opens_without_saving_again() -> Result<(), Error> {
    let fixture = fixture()?;
    let path = fixture.bib("invented.bib", "@book{first,title={Synthetic stars}}")?;
    fixture.open(path, Save::Never);
    assert_eq!(fixture.settled()?.state, LibraryState::Loaded);
    assert_eq!(fixture.saved(), None);
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
fn an_initial_missing_file_keeps_its_name_without_claiming_references() -> Result<(), Error> {
    let fixture = fixture()?;
    fixture.open(fixture.directory.path().join("missing.bib"), Save::Always);
    let failed = fixture.settled()?;
    assert_eq!(failed.state, LibraryState::Error);
    assert_eq!(failed.name.as_deref(), Some("missing.bib"));
    assert!(!failed.has_snapshot && !failed.kept_references && failed.references == 0);
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
fn rereading_a_changed_file_is_an_update_and_an_unchanged_one_is_not() -> Result<(), Error> {
    let fixture = fixture()?;
    let path = fixture.bib("edited.bib", "@misc{one,title={One}}")?;
    fixture.open(path.clone(), Save::Always);
    fixture.settled()?;
    fixture.owner.submit(Intent::Reload);
    fixture.until(|status| status.state == LibraryState::Reloading)?;
    let unchanged = fixture.settled()?;
    assert!(unchanged.updated_at.is_none(), "an unchanged reread is not an update");
    std::fs::write(&path, "@misc{one,title={One}}\n@misc{two,title={Two}}")?;
    fixture.owner.submit(Intent::Reload);
    let reloading = fixture.until(|status| status.state == LibraryState::Reloading)?;
    assert!(reloading.has_snapshot, "the references stay searchable while rereading");
    let updated = fixture.settled()?;
    assert_eq!(updated.references, 2);
    assert!(updated.updated_at.is_some());
    Ok(())
}

#[test]
fn a_failed_reread_keeps_the_references_and_the_next_opening_check_recovers() -> Result<(), Error> {
    let fixture = fixture()?;
    let path = fixture.bib("flaky.bib", "@misc{kept,title={Kept}}")?;
    fixture.open(path.clone(), Save::Always);
    fixture.settled()?;
    // A watched file that is healthy needs no check when the picker opens.
    assert!(fixture.owner.submit(Intent::Check));
    std::fs::remove_file(&path)?;
    fixture.owner.submit(Intent::Reload);
    let failed = fixture.until(|status| status.stale)?;
    assert!(
        failed.stale && failed.kept_references && failed.references == 1,
        "{failed:?}"
    );
    std::fs::write(&path, "@misc{kept,title={Kept}}\n@misc{fixed,title={Fixed}}")?;
    fixture.owner.submit(Intent::Check);
    let recovered = fixture.until(|status| status.references == 2)?;
    assert!(recovered.error.is_none() && !recovered.stale);
    Ok(())
}

#[test]
fn samples_are_left_without_touching_the_saved_choice() -> Result<(), Error> {
    let fixture = fixture()?;
    let saved = fixture.bib("saved.bib", "@misc{saved,title={Saved}}")?;
    fixture.open(saved.clone(), Save::Always);
    fixture.settled()?;
    let samples = fixture.bib("samples.bib", "@misc{sample,title={Invented sample}}")?;
    fixture.open(Source::Samples(samples), Save::IfChanged);
    let shown = fixture.until(|status| status.state == LibraryState::Loaded && status.sample)?;
    assert_eq!(shown.references, 1);
    assert_eq!(fixture.saved(), Some(LibrarySetting::Bibtex(saved.canonicalize()?)));
    fixture.owner.submit(Intent::LeaveSamples);
    assert_eq!(fixture.until(|status| !status.sample)?.state, LibraryState::None);
    assert_eq!(fixture.saved(), Some(LibrarySetting::Bibtex(saved.canonicalize()?)));
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

#[test]
fn reload_with_nothing_open_says_so() -> Result<(), Error> {
    let fixture = fixture()?;
    assert!(!fixture.owner.submit(Intent::Reload));
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
fn a_zotero_choice_opens_and_showing_the_picker_checks_it_quietly() -> Result<(), Error> {
    let server = zotero_fixture::Server::new();
    let port = server.port;
    let fixture = fixture_with(move || Engine::with_zotero_port(port))?;
    let groups = ZoteroLibraries::new(vec![ZoteroLibrary::User]).ok_or("a choice")?;
    fixture.open(Source::Zotero(groups.clone()), Save::Always);
    let loaded = fixture.settled()?;
    assert_eq!(loaded.state, LibraryState::Loaded, "{loaded:?}");
    assert!(loaded.zotero && loaded.snapshot_zotero && loaded.references > 0);
    assert_eq!(fixture.saved(), Some(LibrarySetting::Zotero(groups)));
    // Showing the picker checks Zotero quietly: no busy state.
    fixture.owner.submit(Intent::Check);
    let checked = fixture.settled()?;
    assert_eq!(checked.state, LibraryState::Loaded);
    assert_eq!(checked.references, loaded.references);
    Ok(())
}

#[test]
fn a_failed_zotero_choice_is_retried_when_the_person_comes_back() -> Result<(), Error> {
    let port = {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
        listener.local_addr()?.port()
    };
    let fixture = fixture_with(move || Engine::with_zotero_port(port))?;
    fixture.open(Source::Zotero(ZoteroLibrary::User.into()), Save::Always);
    let failed = fixture.settled()?;
    assert_eq!(failed.state, LibraryState::Error);
    assert!(failed.zotero && !failed.has_snapshot);
    assert_eq!(fixture.saved(), None, "a choice that never opened is not saved");
    fixture.owner.submit(Intent::RetryFailed);
    let retried = fixture.until(|status| status.state == LibraryState::Loading)?;
    assert!(retried.zotero);
    assert_eq!(fixture.settled()?.state, LibraryState::Error);
    Ok(())
}

#[test]
fn a_stuck_read_gives_way_to_cancellation_and_to_its_deadline() {
    static READS: AtomicUsize = AtomicUsize::new(0);
    let cancel = OpenCancellation::default();
    let (release, held) = mpsc::channel::<()>();
    let waiting = {
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            supervise_with(
                &cancel,
                Duration::from_secs(30),
                &READS,
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
        &READS,
        LoadError::FILE_UNAVAILABLE,
        || {
            std::thread::sleep(Duration::from_secs(1));
            Ok(())
        },
    );
    assert_eq!(deadline, Err(LoadError::FILE_UNAVAILABLE));
    drop(release);
}

#[test]
fn a_panicking_publication_does_not_disable_the_library_owner() -> Result<(), Error> {
    let directory = tempfile::tempdir()?;
    let settings = Arc::new(crate::settings::Owner::open(directory.path().join("settings.json")));
    let panic_once = std::sync::atomic::AtomicBool::new(true);
    let owner = Owner::new(
        settings,
        || Ok(Engine::new()?.without_file_watches()),
        move |_| {
            if panic_once.swap(false, Ordering::SeqCst) {
                panic!("invented publication failure");
            }
        },
    );
    assert!(crate::sync::attempt(|| owner.submit(Intent::RefuseNotLocal)).is_err());
    assert!(owner.submit(Intent::Close));
    assert_eq!(owner.status().state, LibraryState::None);
    assert!(owner.view().error.is_none());
    Ok(())
}
