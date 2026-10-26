//! Read Zotero only while it is closed, opening SQLite only on a private copy.

#[cfg(test)]
mod tests {
    #[test]
    fn closed_reader_copies_and_reads_synthetic_database() {
        let source = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic source"));
        let db = rusqlite::Connection::open(source.path().join("zotero.sqlite"))
            .unwrap_or_else(|_| panic!("synthetic database"));
        db.execute_batch("CREATE TABLE version(schema TEXT, version INTEGER); INSERT INTO version VALUES('userdata',123);")
            .unwrap_or_else(|_| panic!("synthetic schema"));
        drop(db);
        let scratch = tempfile::tempdir().unwrap_or_else(|_| panic!("synthetic scratch"));
        assert_eq!(super::read_schema(source.path(), scratch.path()), Ok(123));
    }
}

#[cfg(test)]
fn read_schema(_source: &std::path::Path, _scratch: &std::path::Path) -> Result<i64, &'static str> {
    Err("closed reader not yet implemented")
}
