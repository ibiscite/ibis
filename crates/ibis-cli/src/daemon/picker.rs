//! A native picker's ordered RPC turns, over the daemon's socket or, with no
//! daemon, over the standard streams of an engine the shell started for
//! itself. A delivery turn ends with the shell's acknowledgment, or with its
//! connection closing.
use super::{Endpoint, Framed, Stream, WRITE_TIMEOUT, decode as decode_frame};
use crate::rpc;
use ibis_core::contract::{EngineError, ErrorCode, ItemRef};
use ibis_core::destination::{AppId, PASTE_DEADLINE_MS};
use ibis_engine::citations::SessionId;
use ibis_engine::picker::{
    self, Drafts, PickerChangeFormatRequest, PickerDraft, PickerLibraryCondition, PickerOpenRequest, PickerSession,
};
use ibis_engine::recents::{List, SessionReferences};
use ibis_engine::settings::native::SettingsRequest;
use ibis_engine::{Cancellation, Engine, PickerMethod, protocol, settings};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::{Duration, Instant};

// Allow for the native post and the acknowledgement after the last possible paste.
const ACK_GRACE: Duration = Duration::from_millis(500);

/// How picker turns travel: frames in and out, and the shell's word, within
/// the paste deadline, that its native write ended.
pub(crate) trait Wire {
    fn request(&mut self) -> io::Result<Option<Value>>;
    fn send(&mut self, message: &Value) -> io::Result<()>;
    fn acknowledgement(&mut self) -> io::Result<bool>;
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_millis(PASTE_DEADLINE_MS) + ACK_GRACE
}

impl Wire for Framed {
    fn request(&mut self) -> io::Result<Option<Value>> {
        Framed::request(self)
    }

    fn send(&mut self, message: &Value) -> io::Result<()> {
        Framed::send(self, message)
    }

    fn acknowledgement(&mut self) -> io::Result<bool> {
        let framed = self;
        // A stalled or partial frame closes the turn. A deadline bounds the
        // entire frame even if the peer makes intermittent progress.
        struct Until<'a> {
            reader: &'a mut BufReader<Stream>,
            deadline: Instant,
        }
        impl Read for Until<'_> {
            fn read(&mut self, destination: &mut [u8]) -> io::Result<usize> {
                let n = self.fill_buf()?.read(destination)?;
                self.consume(n);
                Ok(n)
            }
        }
        impl BufRead for Until<'_> {
            fn fill_buf(&mut self) -> io::Result<&[u8]> {
                let left = self.deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::ErrorKind::TimedOut.into());
                }
                self.reader.get_mut().set_timeouts(Some(left), WRITE_TIMEOUT)?;
                self.reader.fill_buf()
            }
            fn consume(&mut self, n: usize) {
                self.reader.consume(n);
            }
        }
        let mut reader = Until {
            reader: &mut framed.reader,
            deadline: deadline(),
        };
        let copied = decode_frame(protocol::read_frame(&mut reader))?.ok_or(io::ErrorKind::UnexpectedEof)?;
        framed.session()?;
        Ok(copied)
    }
}

/// A private engine's standard streams. One thread reads frames so that the
/// acknowledgement can wait with a deadline, as the socket does.
pub(crate) struct Pipes {
    frames: mpsc::Receiver<io::Result<Option<Value>>>,
    out: io::Stdout,
}

impl Pipes {
    pub(crate) fn standard() -> Self {
        let (sender, frames) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let mut input = BufReader::new(io::stdin());
            loop {
                let frame = decode_frame(protocol::read_frame(&mut input));
                let end = !matches!(frame, Ok(Some(_)));
                if sender.send(frame).is_err() || end {
                    break;
                }
            }
        });
        Self {
            frames,
            out: io::stdout(),
        }
    }
}

impl Wire for Pipes {
    fn request(&mut self) -> io::Result<Option<Value>> {
        self.frames.recv().unwrap_or(Ok(None))
    }

    fn send(&mut self, message: &Value) -> io::Result<()> {
        protocol::write_frame(&mut self.out.lock(), message).map_err(super::frame_error)
    }

    fn acknowledgement(&mut self) -> io::Result<bool> {
        let left = deadline().saturating_duration_since(Instant::now());
        match self.frames.recv_timeout(left) {
            Ok(frame) => serde_json::from_value(frame?.ok_or(io::ErrorKind::UnexpectedEof)?).map_err(io::Error::other),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(io::ErrorKind::TimedOut.into()),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(io::ErrorKind::UnexpectedEof.into()),
        }
    }
}

pub(crate) struct History {
    refs: SessionReferences,
    enabled: Mutex<bool>,
}

impl History {
    fn open(path: &Path) -> Self {
        let enabled = Self::wanted(path);
        let refs = SessionReferences::default();
        *refs.lock() = List::open(path.with_file_name("recents.json"), enabled);
        if !enabled {
            let _ = ibis_engine::recents::clear_disabled(path);
        }
        Self {
            refs,
            enabled: Mutex::new(enabled),
        }
    }

    fn wanted(path: &Path) -> bool {
        settings::load(path)
            .cites()
            .is_ok_and(|settings| settings.portable.recent_citations())
    }

    fn enabled(&self, path: &Path) -> bool {
        let Ok(mut previous) = self.enabled.lock() else {
            return false;
        };
        let enabled = Self::wanted(path);
        if *previous != enabled {
            let _ = self.refs.set_enabled(enabled);
            *previous = enabled;
        }
        enabled
    }

    fn recent(&self, path: &Path, app: Option<&AppId>) -> Vec<ItemRef> {
        if self.enabled(path) {
            self.refs.lock().ids(app.map(AppId::normalized).as_deref()).to_vec()
        } else {
            Vec::new()
        }
    }

    fn record(&self, path: &Path, app: Option<&AppId>, items: &[ItemRef]) {
        if self.enabled(path) {
            let epoch = self.refs.lock().epoch();
            self.refs.record(&epoch, app.map(AppId::normalized).as_deref(), items);
        }
    }
}

struct Picker<'a> {
    host: &'a crate::headless::Host,
    settings: PathBuf,
    engine: Engine,
    source: Option<settings::LibrarySetting>,
    target: Option<AppId>,
    site: Option<AppId>,
    session: PickerSession,
    paste_at_opening: bool,
    /// The citation last saved to `picker-draft.json`, and when, so typing
    /// alone writes nothing until the saved copy nears its expiry. None
    /// writes at the next turn.
    saved: Option<(PickerDraft, Instant)>,
}

impl Picker<'_> {
    fn open(&mut self, request: PickerOpenRequest) -> Result<picker::PickerOpenResult, EngineError> {
        self.engine.citations().invalidate(SessionId::PICKER);
        self.paste_at_opening = false;
        self.site = request.site();
        self.target = request.destination();
        let source = request.library(&self.settings)?;
        let mut error = None;
        if request.reload || source != self.source {
            // Each picker connection has its own library selection; file reads
            // and formatting still use the daemon's shared caches.
            self.engine = self.host.cli_engine()?;
            self.source = None;
            let cancel = Cancellation::default();
            let opened = match &source {
                Some(settings::LibrarySetting::Bibtex(path)) => self.engine.open_file_id(path, &cancel).map(|_| ()),
                Some(settings::LibrarySetting::Zotero(libraries)) => self
                    .engine
                    .prepare_zotero_reload(request.reload, Some(libraries.libraries()), None, &cancel)
                    .and_then(|prepared| self.engine.accept_zotero_reload(prepared)),
                None => Ok(()),
            };
            match opened {
                Ok(()) => self.source = source.clone(),
                Err(failure) => error = Some(failure),
            }
        }
        let recent = self.history().recent(&self.settings, self.target.as_ref());
        // Reopening soon after, or after the engine changed, takes back the
        // citation an earlier engine saved; any other opening starts afresh.
        let drafts = Drafts::beside(&self.settings);
        let draft = request.keep_query.then(|| drafts.load()).flatten();
        if draft.is_none() {
            // Nothing saved to take back, or a new moment of writing.
            let _ = drafts.save(&PickerDraft::default());
        }
        let draft = draft.filter(|_| self.history().enabled(&self.settings));
        let (session, engine) = (&mut self.session, &self.engine);
        let result = picker::open_result(
            &self.settings,
            self.target.as_ref(),
            self.site.as_ref(),
            source.as_ref(),
            error,
            request.language,
            |scope, ready| {
                let state = session.open(
                    engine,
                    ready,
                    scope.clone(),
                    recent,
                    request.language,
                    request.keep_query,
                );
                match draft.filter(|_| ready) {
                    Some(draft) => session.restore(engine, draft),
                    None => state,
                }
            },
        );
        let ready = result
            .library
            .as_ref()
            .is_some_and(|library| library.condition == PickerLibraryCondition::Ready);
        // A ready opening writes what it holds, so a restored citation is as
        // fresh as its opening; one still loading leaves the file alone.
        self.saved = (!ready).then(|| (PickerDraft::default(), Instant::now()));
        if ready {
            self.keep();
        }
        self.paste_at_opening = result.paste_enabled && ready;
        Ok(result)
    }

    /// Save the citation when it changed, for an engine that takes over
    /// (decision 0065). Nothing is kept while recent citations are off, and a
    /// file that cannot be written costs only that.
    fn keep(&mut self) {
        let draft = if self.history().enabled(&self.settings) {
            self.session.draft()
        } else {
            PickerDraft::default()
        };
        let current = self.saved.as_ref().is_some_and(|(saved, at)| {
            *saved == draft && (draft.is_empty() || at.elapsed() < picker::DRAFT_KEPT_FOR / 2)
        });
        if !current {
            let _ = Drafts::beside(&self.settings).save(&draft);
            self.saved = Some((draft, Instant::now()));
        }
    }

    fn history(&self) -> &History {
        self.host.picker_history.get_or_init(|| History::open(&self.settings))
    }
}

/// The daemon's restart fence: a delivery turn holds its connection's lock,
/// and a held endpoint `delivery` lock means a replacement is starting.
pub(crate) struct Fence<'a> {
    pub(crate) turn: &'a Mutex<()>,
    pub(crate) endpoint: &'a Endpoint,
}

pub(crate) fn serve(
    wire: &mut impl Wire,
    host: &crate::headless::Host,
    draining: &AtomicBool,
    fence: Option<Fence<'_>>,
) -> io::Result<()> {
    let config = ibis_engine::identity::config_dir().ok_or(io::ErrorKind::NotFound)?;
    let mut picker = Picker {
        host,
        settings: config.join("com.ibiscite.ibis.native/settings.json"),
        engine: host.cli_engine().map_err(|_| io::Error::other("engine unavailable"))?,
        source: None,
        target: None,
        site: None,
        session: PickerSession::default(),
        paste_at_opening: false,
        saved: None,
    };
    while !draining.load(Ordering::Acquire) {
        let Some(frame) = wire.request()? else {
            break;
        };
        let request = match rpc::parse_message_for(&frame, PickerMethod::parse) {
            rpc::Message::Request(request) => request,
            rpc::Message::Reply(reply) => {
                wire.send(&reply)?;
                continue;
            }
            _ => continue,
        };
        let Some(id) = request.id else { continue };
        let params = request.params;
        let result = match request.method {
            PickerMethod::Open => decode(params).and_then(|params| encode(picker.open(params)?)),
            PickerMethod::ChangeFormat => decode::<PickerChangeFormatRequest>(params).and_then(|request| {
                let owner = host.settings_for(picker.settings.clone(), |_| {});
                let scope = request.answer(&owner, picker.target.as_ref(), picker.site.as_ref())?;
                if picker.session.scope() != Some(&scope) {
                    picker.engine.citations().invalidate(SessionId::PICKER);
                }
                let state = picker.session.set_scope(&picker.engine, scope);
                picker.keep();
                encode(state)
            }),
            PickerMethod::Act => decode(params).and_then(|intent| {
                let state = picker.session.act(&picker.engine, intent)?;
                picker.keep();
                encode(state)
            }),
            PickerMethod::Settings => decode::<Option<SettingsRequest>>(params).and_then(|params| {
                let owner = host.settings_for(picker.settings.clone(), |_| {});
                let saved = params.unwrap_or_default().answer(&owner, |path| {
                    // Validate a replacement off to the side so the current picker ticket stays usable.
                    host.cli_engine()?
                        .open_file_id(path, &Cancellation::default())
                        .map(|_| ())
                })?;
                encode(saved)
            }),
            PickerMethod::Forms => decode(params).and_then(|params| {
                encode(
                    picker::forms(&picker.engine, params, Cancellation::default()).map_err(|_| ErrorCode::Cancelled)?,
                )
            }),
            PickerMethod::Deliver => {
                let _turn = match &fence {
                    Some(fence) => Some(
                        fence
                            .turn
                            .lock()
                            .map_err(|_| io::Error::other("delivery unavailable"))?,
                    ),
                    None => None,
                };
                if draining.load(Ordering::Acquire) {
                    return Ok(());
                }
                // Over the socket the shell holds the fence for the whole turn.
                let unfenced = match &fence {
                    Some(fence) => fence.endpoint.0.lock("delivery")?.is_some(),
                    None => false,
                };
                if unfenced {
                    Err(ErrorCode::BadInput.into())
                } else if let Ok(params) = decode(params) {
                    let mut pasting = false;
                    match picker::deliver(
                        &picker.engine,
                        &picker.settings,
                        params,
                        picker.paste_at_opening,
                        |saved| {
                            picker.session.scope().is_some_and(|scope| {
                                scope.is_current(saved, picker.target.as_ref(), picker.site.as_ref())
                            })
                        },
                        |grant| {
                            pasting = grant.paste;
                            wire.send(&rpc::success(&id, serde_json::to_value(grant)?))?;
                            // The client keeps the restart fence until its last native effect.
                            wire.acknowledgement()
                        },
                    ) {
                        Ok((copied, items)) => {
                            // A paste granted but never acknowledged may have
                            // happened: it is not offered again after a restart.
                            if pasting && copied.as_ref().is_err() {
                                picker.session.delivered();
                                picker.keep();
                            }
                            if copied? {
                                picker
                                    .history()
                                    .record(&picker.settings, picker.target.as_ref(), &items);
                                picker.session.delivered();
                                picker.keep();
                            } else {
                                // The write failed after its ticket was spent: render again.
                                picker.session.forget_preview();
                            }
                            continue;
                        }
                        Err(_) => Err(ErrorCode::Cancelled.into()),
                    }
                } else {
                    Err(ErrorCode::BadInput.into())
                }
            }
        };
        wire.send(&match result {
            Ok(value) => rpc::success(&id, value),
            Err(error) => rpc::failure(Some(&id), error),
        })?;
    }
    Ok(())
}

fn decode<T: DeserializeOwned>(params: &Value) -> Result<T, EngineError> {
    serde_json::from_value(params.clone()).map_err(|_| ErrorCode::BadInput.into())
}

fn encode(value: impl Serialize) -> Result<Value, EngineError> {
    serde_json::to_value(value).map_err(|_| ErrorCode::Internal.into())
}
