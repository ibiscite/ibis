//! Local GlobalShortcuts wire client. The desktop owns the trigger, never settings.json.

use std::{
    collections::HashMap,
    future::poll_fn,
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use zbus::{
    Connection, Proxy,
    export::futures_core::Stream,
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
async fn request(proxy: &Proxy<'_>, method: &str, session: Option<&OwnedObjectPath>) -> Result<Dict, Failure> {
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
        Ok(Some(message)) => successful(message.body().deserialize().map_err(|_| ())?),
        _ => {
            let _ = bounded(response.call::<_, _, ()>("Close", &())).await;
            Err(Failure::Unavailable)
        }
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
        let session = session_handle(request(&proxy, "CreateSession", None).await?)?;
        Ok(Self {
            connection: connection.clone(),
            owner,
            session,
        })
    }

    pub(super) async fn bind(&self) -> Result<Option<String>, Failure> {
        let proxy = bounded(Proxy::new(&self.connection, self.owner.as_str(), ROOT, INTERFACE)).await?;
        let bound = request(&proxy, "BindShortcuts", Some(&self.session)).await?;
        let listed = request(&proxy, "ListShortcuts", Some(&self.session))
            .await
            .unwrap_or(bound);
        Ok(listed
            .get("shortcuts")
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| Shortcuts::try_from(value).ok())
            .and_then(trigger))
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

    #[cfg(target_os = "linux")]
    mod private_bus {
        use super::*;
        use std::sync::{Arc, atomic::AtomicU32};

        struct Fake(Arc<AtomicU32>);

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
                        &(self.0.load(Ordering::Relaxed), dict("session_handle", "/session/fake")),
                    )
                    .await?;
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
                    let _server = zbus::connection::Builder::session()?
                        .name(SERVICE)?
                        .serve_at(ROOT, Fake(code.clone()))?
                        .build()
                        .await?;
                    let portal = Portal::connect().await.map_err(|_| "session failed")?;
                    assert_eq!(portal.session.as_str(), "/session/fake");
                    code.store(1, Ordering::Relaxed);
                    assert!(matches!(Portal::connect().await, Err(Failure::Denied)));
                    code.store(2, Ordering::Relaxed);
                    assert!(matches!(Portal::connect().await, Err(Failure::Unavailable)));
                    Ok(())
                })
        }
    }
}
