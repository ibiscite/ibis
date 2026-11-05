#![allow(clippy::unwrap_used)]
use ibis_core::{
    contract::ErrorCode,
    zotero::api::{Response, Transport, TransportError},
};
use ibis_engine::{
    Cancellation,
    discovery::{Discovery, ZoteroPresence},
    health::InspectionPolicy,
};

struct Probe(u16, Option<Cancellation>);
impl Transport for Probe {
    fn get(&mut self, _: &str) -> Result<Response, TransportError> {
        assert_ne!(self.0, u16::MAX, "read-only inspection must never probe Zotero");
        if let Some(cancel) = &self.1 {
            cancel.cancel();
        }
        match self.0 {
            0 => Err(TransportError::Unreachable),
            status => Ok(Response {
                status,
                headers: vec![],
                body: vec![],
            }),
        }
    }
}

fn publish(
    request: &ibis_engine::discovery::Request,
    path: &str,
) -> Result<(u32, Vec<(String, String)>), ibis_core::contract::EngineError> {
    request.publish(vec![path.into()], |path| {
        (
            path.file_name().unwrap().to_string_lossy().into_owned(),
            path.parent()
                .unwrap()
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        )
    })
}

#[test]
fn expired_handles_late_a_after_b_and_reopen_cannot_choose_old_paths() {
    let discovery = Discovery::default();
    let a = discovery.reserve().unwrap();
    let first = publish(&a, "/invented/first/a.bib").unwrap();
    assert_eq!(first.1[0].0, "a.bib");
    assert_eq!(first.1[0].1, "first");
    let b = discovery.reserve().unwrap();
    assert!(discovery.resolve(first.0, 0).is_none());
    let second = publish(&b, "/invented/second/b.bib").unwrap();
    assert_eq!(
        publish(&a, "/invented/late/a.bib").unwrap_err().code,
        ErrorCode::Cancelled
    );
    assert_eq!(discovery.resolve(second.0, 0), Some("/invented/second/b.bib".into()));
    assert!(discovery.resolve(second.0, 1).is_none());
    let reopened = discovery.reserve().unwrap();
    assert!(discovery.resolve(second.0, 0).is_none());
    assert_eq!(publish(&reopened, "/invented/new/c.bib").unwrap().1[0].0, "c.bib");
    assert!(!serde_json::to_string(&second).unwrap().contains("/invented"));
}

#[test]
fn read_only_policy_refuses_discovery_before_any_provider_runs() {
    let result = ibis_engine::discovery::zotero_presence(
        InspectionPolicy::ReadOnly,
        &mut Probe(u16::MAX, None),
        || panic!("must not discover profiles"),
        &Cancellation::default(),
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::BadInput);
    assert_eq!(
        ibis_engine::discovery::find_zotero(InspectionPolicy::ReadOnly, true, &Cancellation::default())
            .unwrap_err()
            .code,
        ErrorCode::BadInput
    );
    let request = Discovery::default().reserve().unwrap();
    assert_eq!(
        request
            .find(InspectionPolicy::ReadOnly, vec!["/invented/never-read".into()])
            .err()
            .unwrap()
            .code,
        ErrorCode::BadInput
    );
}

#[test]
fn another_discovery_owner_cannot_publish_or_revive_a_handle() {
    let first = Discovery::default();
    let second = Discovery::default();
    let request = first.reserve().unwrap();
    let other = second.reserve().unwrap();
    let result = publish(&request, "/invented/right.bib").unwrap();
    assert!(
        second.resolve(result.0, 0).is_none(),
        "a request only publishes to its original owner"
    );
    assert_eq!(
        publish(&request, "/invented/replayed.bib").unwrap_err().code,
        ErrorCode::Cancelled
    );
    assert_eq!(first.resolve(result.0, 0), Some("/invented/right.bib".into()));
    assert!(publish(&other, "/invented/other.bib").is_ok());
}

#[test]
fn zotero_sharing_changes_and_closed_presence_keep_the_same_visible_results() {
    let probe = |status, installed: fn() -> bool| {
        ibis_engine::discovery::zotero_presence(
            InspectionPolicy::Setup,
            &mut Probe(status, None),
            installed,
            &Cancellation::default(),
        )
        .unwrap()
    };
    assert_eq!(probe(0, || true), ZoteroPresence::Closed);
    assert_eq!(probe(0, || false), ZoteroPresence::Absent);
    assert_eq!(
        probe(403, || panic!("running Zotero needs no file discovery")),
        ZoteroPresence::AccessOff
    );
    assert_eq!(
        probe(200, || panic!("running Zotero needs no file discovery")),
        ZoteroPresence::Available
    );
}

#[test]
fn a_cancelled_api_probe_never_starts_profile_discovery() {
    let cancel = Cancellation::default();
    let result = ibis_engine::discovery::zotero_presence(
        InspectionPolicy::Setup,
        &mut Probe(0, Some(cancel.clone())),
        || panic!("cancelled work must not start filesystem discovery"),
        &cancel,
    );
    assert_eq!(result.unwrap_err().code, ErrorCode::Cancelled);
    assert_eq!(
        ibis_engine::discovery::find_zotero(InspectionPolicy::Setup, true, &cancel)
            .unwrap_err()
            .code,
        ErrorCode::Cancelled
    );
}
