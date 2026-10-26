use ibis_core::contract::LibraryId;
use ibis_engine::{
    Method,
    request::{ItemSelector, LibrariesRequest, SearchRequest, ShowRequest, request_schema},
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
    for method in Method::ALL {
        assert!(request_schema(method).is_object());
    }
    Ok(())
}
