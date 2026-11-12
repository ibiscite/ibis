#![cfg(not(unix))]
#![allow(clippy::unwrap_used)]

#[test]
fn unsupported_safe_writes_refuse_without_creating_client_files() {
    let home = tempfile::tempdir().unwrap();
    let context = ibis::agents::Context::new(
        home.path().into(),
        home.path().join("ibis-data"),
        std::env::current_exe().unwrap(),
    )
    .unwrap();
    assert!(context.setup(&[ibis::agents::Client::Cursor]).is_err());
    assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
}
