use ibis_core::contract::LibraryId;
use ibis_engine::Method;
use serde_json::json;

/// Requests stay strict through dispatch: an argument nothing reads is refused,
/// never ignored, for every method. Each request here is valid without it.
#[test]
fn every_method_refuses_a_field_it_does_not_know() {
    use ibis_core::contract::ErrorCode;
    use ibis_engine::{Cancellation, Engine};
    let engine = Engine::new().unwrap_or_else(|_| panic!("engine"));
    let item = json!({"library": LibraryId::digest(b"synthetic"), "key": "record:one"});
    let valid = |method| match method {
        Method::Libraries => json!({"action":"list"}),
        Method::Search => json!({"query":"lantern"}),
        Method::Show | Method::Entry => json!({"item":{"by":"key","value":"one"}}),
        Method::Cite => json!({"items":[{"item":item}],"output":{"kind":"key","syntax":"pandoc"}}),
        Method::Records => json!({"records":[{"id":"one","type":"book"}],"output":{"kind":"preset","name":"apa"}}),
        Method::Project => json!({"path":"/synthetic/refs.bib","keys":[]}),
        Method::Check => json!({"document":"/synthetic/draft.md","set":"/synthetic/references.json","quotes":true}),
        Method::Outline => json!({"item":{"by":"key","value":"one"}}),
        Method::Read => json!({"attachment":"att:invented"}),
        Method::FindIn => json!({"attachment":"att:invented","query":"lantern"}),
        Method::Share => json!({"items":[{"by":"key","value":"one"}]}),
        Method::Styles | Method::Outputs => json!({}),
        Method::Schema => json!({"method":"cite"}),
        Method::Call => json!({"method":"outputs","params":{}}),
    };
    for method in Method::ALL {
        let mut params = valid(method);
        params["invented"] = json!(true);
        let refused = engine
            .dispatch(method, &params, &Cancellation::default())
            .err()
            .map(|error| error.code);
        assert_eq!(refused, Some(ErrorCode::BadInput), "{}", method.as_str());
    }
    let schema = engine
        .dispatch(Method::Schema, &json!({}), &Cancellation::default())
        .unwrap_or_else(|_| panic!("schema"));
    let listed = schema.data.as_array().cloned().unwrap_or_default();
    assert_eq!(listed.len(), Method::ALL.len());
    assert!(
        listed
            .iter()
            .all(|entry| entry["contract"] == ibis_core::contract::CONTRACT_VERSION)
    );
}
