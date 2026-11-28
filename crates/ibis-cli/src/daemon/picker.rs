//! A native picker's ordered RPC turns. A delivery turn ends with the
//! shell's acknowledgment, or with its connection closing.
use super::{Connections, Endpoint, Framed, Stream, WRITE_TIMEOUT, decode as decode_frame};
use crate::rpc;
use ibis_core::contract::{EngineError, ErrorCode, ItemRef};
use ibis_core::destination::{AppId, PASTE_DEADLINE_MS};
use ibis_engine::citations::SessionId;
use ibis_engine::picker::{self, PickerChangeFormatRequest, PickerLibraryCondition, PickerOpenRequest, PickerSession};
use ibis_engine::recents::{List, SessionReferences};
use ibis_engine::settings::native::SettingsRequest;
use ibis_engine::{Cancellation, Engine, PickerMethod, protocol, settings};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, atomic::Ordering};
use std::time::{Duration, Instant};

// Allow for the native post and the acknowledgement after the last possible paste.
const ACK_GRACE: Duration = Duration::from_millis(500);

fn acknowledgement(framed: &mut Framed) -> io::Result<bool> {
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
        deadline: Instant::now() + Duration::from_millis(PASTE_DEADLINE_MS) + ACK_GRACE,
    };
    let copied = decode_frame(protocol::read_frame(&mut reader))?.ok_or(io::ErrorKind::UnexpectedEof)?;
    framed.session()?;
    Ok(copied)
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
        let (session, engine) = (&mut self.session, &self.engine);
        let result = picker::open_result(
            &self.settings,
            self.target.as_ref(),
            self.site.as_ref(),
            source.as_ref(),
            error,
            request.language,
            |scope, ready| {
                session.open(
                    engine,
                    ready,
                    scope.clone(),
                    recent,
                    request.language,
                    request.keep_query,
                )
            },
        );
        self.paste_at_opening = result.paste_enabled
            && result
                .library
                .as_ref()
                .is_some_and(|library| library.condition == PickerLibraryCondition::Ready);
        Ok(result)
    }

    fn history(&self) -> &History {
        self.host.picker_history.get_or_init(|| History::open(&self.settings))
    }
}

pub(super) fn serve(
    mut framed: Framed,
    host: &crate::headless::Host,
    connections: &Connections,
    delivery: &Mutex<()>,
) -> io::Result<()> {
    let endpoint = Endpoint::current()?;
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
    };
    while !connections.draining.load(Ordering::Acquire) {
        let Some(frame): Option<Value> = framed.request()? else {
            break;
        };
        let request = match rpc::parse_message_for(&frame, PickerMethod::parse) {
            rpc::Message::Request(request) => request,
            rpc::Message::Reply(reply) => {
                framed.send(&reply)?;
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
                encode(picker.session.set_scope(&picker.engine, scope))
            }),
            PickerMethod::Act => decode(params).and_then(|intent| encode(picker.session.act(&picker.engine, intent)?)),
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
                let _turn = delivery.lock().map_err(|_| io::Error::other("delivery unavailable"))?;
                if connections.draining.load(Ordering::Acquire) {
                    return Ok(());
                }
                let held = endpoint.0.lock("delivery")?;
                if held.is_some() {
                    Err(ErrorCode::BadInput.into())
                } else if let Ok(params) = decode(params) {
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
                            framed.send(&rpc::success(&id, serde_json::to_value(grant)?))?;
                            // The client keeps the restart fence until its last native effect.
                            acknowledgement(&mut framed)
                        },
                    ) {
                        Ok((copied, items)) => {
                            if copied? {
                                picker
                                    .history()
                                    .record(&picker.settings, picker.target.as_ref(), &items);
                                picker.session.delivered();
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
        framed.send(&match result {
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
