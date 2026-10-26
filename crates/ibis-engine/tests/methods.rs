use ibis_core::contract::ErrorCode;
use ibis_engine::Method;

#[test]
fn one_method_list_drives_all_front_ends() {
    let names = ["libraries", "search", "show", "cite", "styles", "schema", "call"];
    assert_eq!(Method::ALL.map(Method::as_str), names);
    for (method, name) in Method::ALL.into_iter().zip(names) {
        assert_eq!(Method::parse(name), Ok(method));
    }
    assert_eq!(Method::parse("unknown"), Err(ErrorCode::Usage));
    assert_eq!(Method::parse(""), Err(ErrorCode::Usage));
}
