//! A native picker's ordered RPC turns. A delivery turn ends with the
//! shell's acknowledgment, or with its connection closing.
use super::{Connections, Endpoint, Framed};
use ibis_core::contract::{EngineError, ErrorCode, ItemRef};
use ibis_core::destination::AppId;
use ibis_engine::citations::SessionId;
use ibis_engine::picker::{self, PickerOpenRequest, PickerPreviewResult};
use ibis_engine::recents::{List, SessionReferences};
use ibis_engine::{Cancellation, Engine, PickerMethod, rpc, settings};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, atomic::Ordering};

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
    file: Option<PathBuf>,
    target: Option<AppId>,
    paste_at_opening: bool,
}

impl Picker<'_> {
    fn open(&mut self, request: PickerOpenRequest) -> Result<picker::PickerOpenResult, EngineError> {
        self.engine.citations().invalidate(SessionId::PICKER);
        self.paste_at_opening = false;
        self.target = request.destination();
        let file = request.library(&self.settings)?;
        if file != self.file {
            // Each picker connection has its own library selection; file reads
            // and formatting still use the daemon's shared caches.
            self.engine = self.host.cli_engine()?;
            self.file = None;
            if let Some(path) = &file {
                self.engine.open_file_id(path, &Cancellation::default())?;
            }
            self.file = file;
        }
        let result = picker::open_result(&self.settings, self.target.as_ref(), self.file.is_some());
        self.paste_at_opening = result.paste_enabled;
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
        file: None,
        target: None,
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
            PickerMethod::Search => decode(params).and_then(|params| {
                let recent = picker.history().recent(&picker.settings, picker.target.as_ref());
                encode(picker::search(
                    &picker.engine,
                    params,
                    &recent,
                    &Cancellation::default(),
                )?)
            }),
            PickerMethod::Preview => decode(params).and_then(|params| {
                let preview =
                    picker::preview(&picker.engine, params, Cancellation::default()).unwrap_or_else(|error| {
                        PickerPreviewResult {
                            ticket: String::new(),
                            content_hash: String::new(),
                            text: String::new(),
                            problem: Some(error.to_string()),
                        }
                    });
                encode(preview)
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
                        |grant| {
                            framed.send(&rpc::success(&id, serde_json::to_value(grant)?))?;
                            // The client keeps the restart fence until its last native
                            // effect. EOF means it can no longer perform that effect.
                            framed
                                .request::<bool>()?
                                .ok_or_else(|| io::Error::from(io::ErrorKind::UnexpectedEof))
                        },
                    ) {
                        Ok((copied, items)) => {
                            if copied? {
                                picker
                                    .history()
                                    .record(&picker.settings, picker.target.as_ref(), &items);
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
