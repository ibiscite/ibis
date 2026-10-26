use super::*;

#[test]
fn temporary_query_storage_stays_in_memory_under_the_sqlite_heap_cap() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("synthetic.sqlite");
    drop(Connection::open(&path).unwrap_or_else(|e| panic!("{e}")));
    let control = Control {
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
        cancelled: std::sync::Arc::new(|| false),
    };
    let db = open(&path, control).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(
        db.query_row("PRAGMA temp_store", [], |row| row.get::<_, i64>(0)).ok(),
        Some(2)
    );
    let limit: i64 = db
        .query_row("PRAGMA hard_heap_limit", [], |row| row.get(0))
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(limit > 0 && limit <= 64 * 1024 * 1024);
}
