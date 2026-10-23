//! Local GlobalShortcuts wire client. The desktop owns the trigger, never settings.json.

use std::{
    collections::HashMap,
    future::{Future, poll_fn},
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::Poll,
    time::Duration,
};
use zbus::{
    Connection, MatchRule, Message, MessageStream, Proxy,
    export::futures_core::Stream,
    message::Sequence,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

type Dict = HashMap<String, OwnedValue>;
type Shortcuts = Vec<(String, Dict)>;
const SERVICE: &str = "org.freedesktop.portal.Desktop";
const ROOT: &str = "/org/freedesktop/portal/desktop";
const INTERFACE: &str = "org.freedesktop.portal.GlobalShortcuts";
const ID: &str = "open-ibis";
const CALL: Duration = Duration::from_secs(10);
const RESPONSE: Duration = Duration::from_secs(120);

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Unavailable,
    Denied,
}

impl From<()> for Failure {
    fn from((): ()) -> Self {
        Self::Unavailable
    }
}

fn successful((code, values): (u32, Dict)) -> Result<Dict, Failure> {
    match code {
        0 => Ok(values),
        1 => Err(Failure::Denied),
        _ => Err(Failure::Unavailable),
    }
}

fn session_handle(mut values: Dict) -> Result<OwnedObjectPath, ()> {
    let value = values.remove("session_handle").ok_or(())?;
    OwnedObjectPath::try_from(String::try_from(value).map_err(|_| ())?).map_err(|_| ())
}

fn trigger(shortcuts: Shortcuts) -> Option<String> {
    let (_, mut options) = shortcuts.into_iter().find(|(id, _)| id == ID)?;
    let text = String::try_from(options.remove("trigger_description")?).ok()?;
    (!text.is_empty() && text.len() <= 1024 && !text.chars().any(char::is_control)).then_some(text)
}

fn binding() -> (&'static str, HashMap<&'static str, Value<'static>>) {
    (
        ID,
        HashMap::from([
            ("description", Value::from("Open Ibis")),
            ("preferred_trigger", Value::from("CTRL+SHIFT+y")),
        ]),
    )
}

async fn bounded<T, E>(future: impl std::future::Future<Output = Result<T, E>>) -> Result<T, ()> {
    tokio::time::timeout(CALL, future)
        .await
        .map_err(|_| ())?
        .map_err(|_| ())
}

/// Subscription is installed before sending, including for immediate responses.
async fn request(
    proxy: &Proxy<'_>,
    method: &str,
    session: Option<&OwnedObjectPath>,
) -> Result<(Dict, Sequence), Failure> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let token = format!("ibis_{}", SERIAL.fetch_add(1, Ordering::Relaxed));
    let sender = proxy.connection().unique_name().ok_or(())?.as_str()[1..].replace('.', "_");
    let path = format!("{ROOT}/request/{sender}/{token}");
    let response = bounded(Proxy::new(
        proxy.connection(),
        proxy.destination().clone(),
        path.as_str(),
        "org.freedesktop.portal.Request",
    ))
    .await?;
    let mut signals = bounded(response.receive_signal("Response")).await?;
    let mut options = HashMap::from([("handle_token", Value::from(token.as_str()))]);
    let returned: OwnedObjectPath = match session {
        None => {
            options.insert("session_handle_token", Value::from(token.as_str()));
            bounded(proxy.call(method, &(options,))).await?
        }
        Some(session) if method == "BindShortcuts" => {
            bounded(proxy.call(method, &(session, vec![binding()], "", options))).await?
        }
        Some(session) => bounded(proxy.call(method, &(session, options))).await?,
    };
    if returned.as_str() != path {
        return Err(Failure::Unavailable);
    }
    let result = tokio::time::timeout(RESPONSE, poll_fn(|cx| Pin::new(&mut signals).poll_next(cx))).await;
    match result {
        Ok(Some(message)) => Ok((
            successful(message.body().deserialize().map_err(|_| ())?)?,
            message.recv_position(),
        )),
        _ => {
            let _ = bounded(response.call::<_, _, ()>("Close", &())).await;
            Err(Failure::Unavailable)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Event {
    Trigger(Option<String>),
    Activated,
}

#[derive(Debug, PartialEq, Eq)]
enum Signal {
    Event(Event),
    Closed,
}

fn signal(message: &Message, session: &OwnedObjectPath) -> Option<Signal> {
    let header = message.header();
    if header.interface()?.as_str() == "org.freedesktop.portal.Session"
        && header.path()?.as_str() == session.as_str()
        && header.member()?.as_str() == "Closed"
    {
        let _: (Dict,) = message.body().deserialize().ok()?;
        return Some(Signal::Closed);
    }
    if header.path()?.as_str() != ROOT || header.interface()?.as_str() != INTERFACE {
        return None;
    }
    match header.member()?.as_str() {
        "Activated" => {
            let (handle, id, _, _): (OwnedObjectPath, String, u64, Dict) = message.body().deserialize().ok()?;
            (handle == *session && id == ID).then_some(Signal::Event(Event::Activated))
        }
        "ShortcutsChanged" => {
            let (handle, shortcuts): (OwnedObjectPath, Shortcuts) = message.body().deserialize().ok()?;
            (handle == *session).then(|| Signal::Event(Event::Trigger(trigger(shortcuts))))
        }
        _ => None,
    }
}

/// Session setup is separate from binding so the activation listener can be
/// installed and drained before BindShortcuts presents the consent dialog.
pub(super) struct Portal {
    pub(super) connection: Connection,
    pub(super) owner: String,
    pub(super) session: OwnedObjectPath,
}

impl Portal {
    pub(super) async fn connect() -> Result<Self, Failure> {
        let connection = bounded(Connection::session()).await?;
        let bus = bounded(zbus::fdo::DBusProxy::new(&connection)).await?;
        // Properties.Get activates the service and proves the interface exists.
        let service = bounded(Proxy::new(&connection, SERVICE, ROOT, INTERFACE)).await?;
        let _: u32 = bounded(service.get_property("version")).await?;
        let owner = bounded(bus.get_name_owner(SERVICE.try_into().map_err(|_| ())?))
            .await?
            .to_string();
        let proxy = bounded(Proxy::new(&connection, owner.as_str(), ROOT, INTERFACE)).await?;
        let session = session_handle(request(&proxy, "CreateSession", None).await?.0)?;
        Ok(Self {
            connection: connection.clone(),
            owner,
            session,
        })
    }

    /// Consume the session so BindShortcuts cannot be repeated or run concurrently.
    /// Drain before and during Bind/List, including while consent is pending.
    /// Activation is coalesced until consent succeeds; denial emits no events.
    /// Closed/transport failure ends this session. App wiring and owner restart
    /// recovery remain separate; callbacks must not block this signal loop.
    pub(super) async fn run(self, mut emit: impl FnMut(Event)) -> Result<(), Failure> {
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(self.owner.as_str())
            .map_err(|_| ())?
            .path_namespace(ROOT)
            .map_err(|_| ())?
            .build();
        let mut signals = bounded(MessageStream::for_match_rule(rule, &self.connection, Some(64))).await?;
        let mut binding = std::pin::pin!(self.bind());
        let mut ready = false;
        let mut completed = None;
        let mut remaining = 0usize;
        let mut activation = false;
        let mut changed = None;
        loop {
            enum Next {
                Bound(Result<(Option<String>, Sequence), Failure>),
                Signal(Option<zbus::Result<Message>>),
            }
            let next = poll_fn(|cx| {
                if !ready
                    && completed.is_none()
                    && let Poll::Ready(result) = binding.as_mut().poll(cx)
                {
                    completed = Some(result);
                    remaining = signals.max_queued();
                }
                // All earlier signals are already queued. One queue's capacity
                // drains them (including Closed) without starvation by new signals.
                if (completed.is_none() || remaining > 0)
                    && let Poll::Ready(message) = Pin::new(&mut signals).poll_next(cx)
                {
                    remaining = remaining.saturating_sub(1);
                    return Poll::Ready(Next::Signal(message));
                }
                completed
                    .take()
                    .map_or(Poll::Pending, |result| Poll::Ready(Next::Bound(result)))
            })
            .await;
            match next {
                Next::Bound(result) => {
                    let (initial, position) = result?;
                    ready = true;
                    let newer = changed.take().filter(|(_, sequence)| *sequence > position);
                    emit(Event::Trigger(newer.map(|(label, _)| label).unwrap_or(initial)));
                    if activation {
                        emit(Event::Activated);
                    }
                }
                Next::Signal(Some(Ok(message))) => match signal(&message, &self.session) {
                    Some(Signal::Closed) => return Err(Failure::Unavailable),
                    Some(Signal::Event(event)) if ready => emit(event),
                    Some(Signal::Event(Event::Activated)) => activation = true,
                    Some(Signal::Event(Event::Trigger(label))) => changed = Some((label, message.recv_position())),
                    None => {}
                },
                Next::Signal(_) => return Err(Failure::Unavailable),
            }
            tokio::task::yield_now().await;
        }
    }

    async fn bind(&self) -> Result<(Option<String>, Sequence), Failure> {
        let proxy = bounded(Proxy::new(&self.connection, self.owner.as_str(), ROOT, INTERFACE)).await?;
        let bound = request(&proxy, "BindShortcuts", Some(&self.session)).await?;
        let listed = request(&proxy, "ListShortcuts", Some(&self.session))
            .await
            .unwrap_or(bound);
        Ok((
            listed
                .0
                .get("shortcuts")
                .and_then(|value| value.try_clone().ok())
                .and_then(|value| Shortcuts::try_from(value).ok())
                .and_then(trigger),
            listed.1,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict(key: &str, text: &str) -> Dict {
        HashMap::from([(key.into(), zbus::zvariant::Str::from(text).into())])
    }

    #[test]
    fn session_handle_is_a_string_and_failures_are_not_success() {
        assert!(session_handle(dict("session_handle", "/session/one")).is_ok());
        assert!(session_handle(dict("session_handle", "bad path")).is_err());
        assert!(session_handle(HashMap::from([("session_handle".into(), 4u32.into())])).is_err());
        assert!(session_handle(HashMap::new()).is_err());
        assert_eq!(successful((1, HashMap::new())), Err(Failure::Denied));
        assert_eq!(successful((2, HashMap::new())), Err(Failure::Unavailable));
        assert!(successful((0, HashMap::new())).is_ok());
    }

    #[test]
    fn only_the_desktop_trigger_for_our_action_is_displayed() {
        let entry = |id: &str, label: &str| (id.into(), dict("trigger_description", label));
        assert_eq!(
            trigger(vec![entry("other", "wrong"), entry(ID, "Strg+Umschalt+Y")]),
            Some("Strg+Umschalt+Y".into())
        );
        assert_eq!(trigger(vec![]), None);
        assert_eq!(trigger(vec![entry(ID, "")]), None);
        assert_eq!(trigger(vec![entry(ID, &"x".repeat(1025))]), None);
        assert_eq!(trigger(vec![entry(ID, "bad\nlabel")]), None);
        let options = binding();
        assert_eq!(options.0, ID);
        assert_eq!(
            <&str>::try_from(&options.1["preferred_trigger"]).ok(),
            Some("CTRL+SHIFT+y")
        );
    }

    #[test]
    fn signals_require_our_session_action_interface_and_well_formed_body() -> Result<(), Box<dyn std::error::Error>> {
        let session = OwnedObjectPath::try_from(format!("{ROOT}/session/one"))?;
        let other = OwnedObjectPath::try_from(format!("{ROOT}/session/other")).map_err(zbus::Error::from)?;
        let activated = |path: &str, interface: &str, handle: &OwnedObjectPath, id: &str| -> zbus::Result<Message> {
            Message::signal(path, interface, "Activated")?.build(&(handle, id, 0u64, Dict::new()))
        };
        assert_eq!(
            signal(&activated(ROOT, INTERFACE, &session, ID)?, &session),
            Some(Signal::Event(Event::Activated))
        );
        for message in [
            activated(ROOT, INTERFACE, &other, ID)?,
            activated(ROOT, INTERFACE, &session, "other-action")?,
            activated(other.as_str(), INTERFACE, &session, ID)?,
            activated(ROOT, "org.example.Other", &session, ID)?,
            Message::signal(ROOT, INTERFACE, "Activated")?.build(&("malformed",))?,
        ] {
            assert_eq!(signal(&message, &session), None);
        }
        let changed = |shortcuts: Shortcuts| -> zbus::Result<Message> {
            Message::signal(ROOT, INTERFACE, "ShortcutsChanged")?.build(&(&session, shortcuts))
        };
        assert_eq!(
            signal(&changed(vec![])?, &session),
            Some(Signal::Event(Event::Trigger(None)))
        );
        assert_eq!(
            signal(
                &changed(vec![(ID.into(), dict("trigger_description", "bad\nlabel"))])?,
                &session
            ),
            Some(Signal::Event(Event::Trigger(None)))
        );
        let closed = |path: &str| -> zbus::Result<Message> {
            Message::signal(path, "org.freedesktop.portal.Session", "Closed")?.build(&(Dict::new(),))
        };
        assert_eq!(signal(&closed(session.as_str())?, &session), Some(Signal::Closed));
        assert_eq!(signal(&closed(other.as_str())?, &session), None);
        Ok(())
    }

    #[cfg(target_os = "linux")]
    mod private_bus {
        use super::*;
        use std::sync::{Arc, atomic::AtomicU32};

        struct Fake(Arc<AtomicU32>);

        async fn respond(
            code: u32,
            values: Dict,
            options: Dict,
            connection: &Connection,
            header: zbus::message::Header<'_>,
        ) -> zbus::fdo::Result<OwnedObjectPath> {
            let sender = header
                .sender()
                .ok_or_else(|| zbus::fdo::Error::Failed("sender".into()))?;
            let token = <&str>::try_from(&options["handle_token"]).map_err(zbus::Error::from)?;
            let sender_path = sender.as_str()[1..].replace('.', "_");
            let path = OwnedObjectPath::try_from(format!("{ROOT}/request/{sender_path}/{token}"))
                .map_err(zbus::Error::from)?;
            // Deliberately respond before returning the request handle.
            connection
                .emit_signal(
                    Some(sender.as_str()),
                    &path,
                    "org.freedesktop.portal.Request",
                    "Response",
                    &(code, values),
                )
                .await?;
            Ok(path)
        }

        async fn emit(
            connection: &Connection,
            member: &str,
            body: &(impl zbus::export::serde::Serialize + zbus::zvariant::DynamicType),
        ) -> zbus::Result<()> {
            connection
                .emit_signal(None::<&str>, ROOT, INTERFACE, member, body)
                .await
        }

        #[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
        impl Fake {
            #[zbus(property, name = "version")]
            fn version(&self) -> u32 {
                2
            }

            async fn create_session(
                &self,
                options: Dict,
                #[zbus(connection)] connection: &Connection,
                #[zbus(header)] header: zbus::message::Header<'_>,
            ) -> zbus::fdo::Result<OwnedObjectPath> {
                respond(
                    self.0.load(Ordering::Relaxed),
                    dict("session_handle", &format!("{ROOT}/session/fake")),
                    options,
                    connection,
                    header,
                )
                .await
            }

            async fn bind_shortcuts(
                &self,
                session: OwnedObjectPath,
                _shortcuts: Shortcuts,
                _parent: String,
                options: Dict,
                #[zbus(connection)] connection: &Connection,
                #[zbus(header)] header: zbus::message::Header<'_>,
            ) -> zbus::fdo::Result<OwnedObjectPath> {
                let other = OwnedObjectPath::try_from(format!("{ROOT}/session/other")).map_err(zbus::Error::from)?;
                // Exceed zbus's default signal queue while the method is pending.
                for _ in 0..128 {
                    emit(connection, "Activated", &(&other, ID, 0u64, Dict::new())).await?;
                }
                for id in ["other-action", ID] {
                    emit(connection, "Activated", &(&session, id, 0u64, Dict::new())).await?;
                }
                emit(connection, "Activated", &("malformed",)).await?;
                for (handle, label) in [(&session, "Alt+Y"), (&other, "wrong")] {
                    emit(
                        connection,
                        "ShortcutsChanged",
                        &(handle, vec![(ID.to_string(), dict("trigger_description", label))]),
                    )
                    .await?;
                }
                let code = self.0.load(Ordering::Relaxed);
                respond(
                    if code >= 3 { 0 } else { code },
                    Dict::new(),
                    options,
                    connection,
                    header,
                )
                .await
            }

            async fn list_shortcuts(
                &self,
                session: OwnedObjectPath,
                options: Dict,
                #[zbus(connection)] connection: &Connection,
                #[zbus(header)] header: zbus::message::Header<'_>,
            ) -> zbus::fdo::Result<OwnedObjectPath> {
                if self.0.load(Ordering::Relaxed) == 3 {
                    connection
                        .emit_signal(
                            None::<&str>,
                            &session,
                            "org.freedesktop.portal.Session",
                            "Closed",
                            &(Dict::new(),),
                        )
                        .await?;
                }
                let shortcuts = vec![(ID.to_string(), dict("trigger_description", "Ctrl+Shift+Y"))];
                let values = HashMap::from([(
                    "shortcuts".into(),
                    Value::new(shortcuts).try_to_owned().map_err(zbus::Error::from)?,
                )]);
                let path = respond(0, values, options, connection, header).await?;
                if self.0.load(Ordering::Relaxed) == 4 {
                    emit(
                        connection,
                        "ShortcutsChanged",
                        &(&session, vec![(ID.to_string(), dict("trigger_description", "Alt+Y"))]),
                    )
                    .await?;
                }
                if self.0.load(Ordering::Relaxed) == 5 {
                    let server = connection.clone();
                    let active = self.0.clone();
                    connection
                        .executor()
                        .spawn(
                            async move {
                                while active.load(Ordering::Relaxed) == 5 {
                                    if emit(&server, "Activated", &(&session, "other-action", 0u64, Dict::new()))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            },
                            "portal-test-flood",
                        )
                        .detach();
                }
                Ok(path)
            }
        }

        #[test]
        fn fake_exports_the_portal_property_name() {
            let mut xml = String::new();
            zbus::object_server::Interface::introspect_to_writer(&Fake(Arc::new(AtomicU32::new(0))), &mut xml, 0);
            assert!(xml.contains("property name=\"version\""));
        }

        #[test]
        fn immediate_response_on_a_private_bus() -> Result<(), Box<dyn std::error::Error>> {
            if std::env::var_os("IBIS_PRIVATE_PORTAL_TEST").is_none() {
                // Nix builds have no host /etc/dbus-1/session.conf.
                let config_dir = tempfile::tempdir()?;
                let config = config_dir.path().join("session.conf");
                std::fs::write(
                    &config,
                    r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>
<policy context="default"><allow send_destination="*"/><allow eavesdrop="true"/>
<allow own="*"/></policy></busconfig>"#,
                )?;
                let status = std::process::Command::new("dbus-run-session")
                    .arg("--config-file")
                    .arg(config)
                    .arg("--")
                    .arg(std::env::current_exe()?)
                    .args([
                        "--exact",
                        "picker::portal::tests::private_bus::immediate_response_on_a_private_bus",
                    ])
                    .env("IBIS_PRIVATE_PORTAL_TEST", "1")
                    .status()?;
                assert!(status.success());
                return Ok(());
            }
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async {
                    let code = Arc::new(AtomicU32::new(0));
                    let server = zbus::connection::Builder::session()?
                        .name(SERVICE)?
                        .serve_at(ROOT, Fake(code.clone()))?
                        .build()
                        .await?;
                    let portal = Portal::connect().await.map_err(|_| "session failed")?;
                    assert_eq!(portal.session.as_str(), format!("{ROOT}/session/fake"));
                    for response in [0, 1, 2, 3, 4, 5] {
                        code.store(0, Ordering::Relaxed);
                        let portal = Portal::connect().await.map_err(|_| "session failed")?;
                        code.store(response, Ordering::Relaxed);
                        let mut events = Vec::new();
                        let mut close_task = None;
                        let session = portal.session.clone();
                        let result = tokio::time::timeout(Duration::from_secs(3), async {
                            let result = portal
                                .run(|event| {
                                    if event == Event::Activated {
                                        code.store(0, Ordering::Relaxed);
                                        let server = server.clone();
                                        let session = session.clone();
                                        close_task = Some(tokio::spawn(async move {
                                            server
                                                .emit_signal(
                                                    None::<&str>,
                                                    &session,
                                                    "org.freedesktop.portal.Session",
                                                    "Closed",
                                                    &(Dict::new(),),
                                                )
                                                .await
                                        }));
                                    }
                                    events.push(event);
                                })
                                .await;
                            if let Some(task) = close_task {
                                task.await??;
                            }
                            Ok::<_, Box<dyn std::error::Error>>(result)
                        })
                        .await
                        .map_err(|_| "signal draining must not hang")??;
                        if matches!(response, 0 | 4 | 5) {
                            let label = if response == 4 { "Alt+Y" } else { "Ctrl+Shift+Y" };
                            assert_eq!(events, [Event::Trigger(Some(label.into())), Event::Activated]);
                        } else {
                            assert!(events.is_empty(), "consent failure must not activate");
                        }
                        assert_eq!(
                            result,
                            Err(if response == 1 {
                                Failure::Denied
                            } else {
                                Failure::Unavailable
                            })
                        );
                    }
                    code.store(1, Ordering::Relaxed);
                    assert!(matches!(Portal::connect().await, Err(Failure::Denied)));
                    code.store(2, Ordering::Relaxed);
                    assert!(matches!(Portal::connect().await, Err(Failure::Unavailable)));
                    Ok(())
                })
        }
    }
}
