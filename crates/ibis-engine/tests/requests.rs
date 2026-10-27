use ibis_core::contract::LibraryId;
use ibis_engine::{
    Method,
    request::{ItemSelector, LibrariesRequest, SearchRequest, ShowRequest, ZoteroLibrary, request_schema},
};
use serde_json::json;

#[test]
fn requests_are_typed_and_unknown_fields_are_refused() -> Result<(), Box<dyn std::error::Error>> {
    let library = LibraryId::file(b"synthetic-schema");
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
    assert!(matches!(open, LibrariesRequest::OpenZotero { libraries: None }));
    assert!(serde_json::from_value::<LibrariesRequest>(serde_json::to_value(&open)?)? == open);
    let mixed: LibrariesRequest =
        serde_json::from_value(json!({"action":"open-zotero","libraries":["personal",{"group":7}]}))?;
    assert!(
        mixed
            == LibrariesRequest::OpenZotero {
                libraries: Some(vec![ZoteroLibrary::Personal, ZoteroLibrary::Group(7)])
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
