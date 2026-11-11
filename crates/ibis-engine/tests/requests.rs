use ibis_core::contract::LibraryId;
use ibis_engine::{
    Method,
    request::{ItemSelector, LibrariesRequest, SearchRequest, ShowRequest, ZoteroLibrary, request_schema},
};
use serde_json::json;

#[test]
fn requests_are_typed_and_unknown_fields_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let library = LibraryId::digest(b"synthetic-schema");
    let search: SearchRequest = serde_json::from_value(json!({"query":"文字","libraries":[library]}))?;
    assert_eq!(search.limit, 10);
    assert!(search.libraries.is_some());
    assert!(serde_json::from_value::<SearchRequest>(json!({"query":"x","unknown":"text"})).is_err());
    let show: ShowRequest = serde_json::from_value(json!({"item":{"by":"key","value":"Example2026"}}))?;
    assert!(matches!(show.item, ItemSelector::Key(_)));
    let request = LibrariesRequest::OpenFile {
        path: "/synthetic/refs.bib".into(),
    };
    assert!(serde_json::from_value::<LibrariesRequest>(serde_json::to_value(&request)?)? == request);
    let open: LibrariesRequest = serde_json::from_value(json!({"action":"open-zotero"}))?;
    assert!(matches!(
        open,
        LibrariesRequest::OpenZotero {
            libraries: None,
            profile: None,
            reload: false,
        }
    ));
    assert!(serde_json::from_value::<LibrariesRequest>(serde_json::to_value(&open)?)? == open);
    let mixed: LibrariesRequest =
        serde_json::from_value(json!({"action":"open-zotero","libraries":["personal",{"group":7}]}))?;
    assert!(
        mixed
            == LibrariesRequest::OpenZotero {
                libraries: Some(vec![ZoteroLibrary::Personal, ZoteroLibrary::Group(7)]),
                profile: None,
                reload: false,
            }
    );
    assert!(serde_json::from_value::<LibrariesRequest>(serde_json::to_value(&mixed)?)? == mixed);
    for invalid in [json!(["user"]), json!([{"group":-1}]), json!([{"group":"7"}])] {
        assert!(
            serde_json::from_value::<LibrariesRequest>(json!({"action":"open-zotero","libraries":invalid})).is_err()
        );
    }
    assert!(serde_json::from_value::<LibrariesRequest>(json!({"action":"open-zotero","unknown":true})).is_err());
    assert!(request_schema(Method::Libraries).to_string().contains("open-zotero"));
    for method in Method::ALL {
        assert!(request_schema(method).is_object());
    }
    Ok(())
}

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
        Method::Project => json!({"path":"/synthetic/refs.bib","keys":[]}),
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
