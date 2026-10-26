//! Query only the disposable copy. No CSL/legacy model adapters live here.
use super::{ClosedError, ClosedItem, ClosedLibrary, ClosedSnapshot, Control, RawCreator, RawItem, SourceMetadata};
use rusqlite::config::DbConfig;
use rusqlite::limits::Limit;
use rusqlite::{Connection, OpenFlags, Row};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

const MAX_ITEMS: usize = 1_000_000;
const MAX_VALUE: usize = 256 * 1024;
const MAX_TEXT: usize = 256 * 1024 * 1024;
const MAX_ROWS: usize = 4_000_000;

pub(super) fn read(copy: &super::private_copy::Copy, control: &Control) -> Result<ClosedSnapshot, ClosedError> {
    control.check()?;
    let path = copy.database_path();
    let dir = path.parent().ok_or(ClosedError::PrivateStorage)?;
    // A rollback journal may name an external super-journal. Never let SQLite
    // recovery interpret a copied journal's path instructions. Zotero repairs
    // the original on next launch; WAL-only recovery is safe in private storage.
    if std::fs::metadata(dir.join("zotero.sqlite-journal")).is_ok_and(|m| m.len() != 0) {
        return Err(ClosedError::NeedsRecovery);
    }
    let mut db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(failure)?;
    for (config, enabled) in [
        (DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false),
        (DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false),
    ] {
        db.set_db_config(config, enabled).map_err(failure)?;
    }
    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, MAX_VALUE as i32),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, 65536),
        (Limit::SQLITE_LIMIT_COLUMN, 256),
        (Limit::SQLITE_LIMIT_EXPR_DEPTH, 64),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_VDBE_OP, 100000),
    ] {
        db.set_limit(limit, value).map_err(failure)?;
    }
    let progress = control.clone();
    db.progress_handler(10000, Some(move || progress.check().is_err()))
        .map_err(failure)?;
    db.execute_batch("PRAGMA temp_store=MEMORY; PRAGMA cache_size=-2048; PRAGMA query_only=ON;")
        .map_err(failure)?;
    let check: String = db
        .query_row("PRAGMA quick_check(1)", [], |r| r.get(0))
        .map_err(failure)?;
    if check != "ok" {
        return Err(ClosedError::Damaged);
    }
    validate_table(&db, "version", &["schema", "version"])?;
    let version: i64 = db
        .query_row("SELECT version FROM version WHERE schema='userdata'", [], |r| r.get(0))
        .map_err(failure)?;
    if version < 123 {
        return Err(ClosedError::TooOld);
    }
    if version > 130 {
        return Err(ClosedError::TooNew);
    }
    for (table, fields) in TABLES {
        validate_table(&db, table, fields)?;
    }
    let budget = Budget {
        text: Cell::new(MAX_TEXT),
        rows: Cell::new(MAX_ROWS),
        exceeded: Cell::new(false),
    };
    let mut libraries = BTreeMap::new();
    each(
        &db,
        "SELECT L.libraryID,L.type,G.groupID FROM libraries L LEFT JOIN groups G USING(libraryID) WHERE L.type IN ('user','group') ORDER BY L.libraryID",
        &budget,
        |r| {
            let id: i64 = r.get(0)?;
            let kind = budget.text(r, 1)?;
            let group_id: Option<i64> = r.get(2)?;
            if id < 1
                || (kind == "group" && group_id.is_none_or(|id| id < 1))
                || (kind == "user" && group_id.is_some())
                || libraries.len() >= 4096
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            if libraries
                .insert(
                    id,
                    ClosedLibrary {
                        library_id: id,
                        group_id,
                        items: Vec::new(),
                    },
                )
                .is_some()
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok(())
        },
    )?;
    let mut items = BTreeMap::<i64, ClosedItem>::new();
    let mut identities = BTreeSet::new();
    each(
        &db,
        "SELECT I.itemID,I.libraryID,I.key,I.version,T.typeName,I.dateAdded,I.dateModified FROM items I JOIN itemTypesCombined T USING(itemTypeID) WHERE T.typeName NOT IN ('note','attachment','annotation') AND NOT EXISTS(SELECT 1 FROM deletedItems D WHERE D.itemID=I.itemID) ORDER BY I.itemID",
        &budget,
        |r| {
            let id: i64 = r.get(0)?;
            let library: i64 = r.get(1)?;
            if !libraries.contains_key(&library) {
                return Ok(());
            }
            if items.len() >= MAX_ITEMS {
                return budget.limit();
            }
            let key = budget.text(r, 2)?;
            let item_type = budget.text(r, 4)?;
            if id < 1
                || key.is_empty()
                || key.len() > 128
                || key.chars().any(|c| c.is_whitespace() || c.is_control())
                || item_type.is_empty()
                || !identities.insert((library, key.clone()))
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let item = ClosedItem {
                data: RawItem {
                    key,
                    item_type,
                    ..RawItem::default()
                },
                metadata: SourceMetadata {
                    item_id: id,
                    library_id: library,
                    version: r.get(3)?,
                    date_added: budget.text(r, 5)?,
                    date_modified: budget.text(r, 6)?,
                    stored_fields: BTreeMap::new(),
                },
            };
            if items.insert(id, item).is_some() {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok(())
        },
    )?;
    each(
        &db,
        "SELECT D.itemID,F.fieldName,V.value FROM itemData D JOIN fieldsCombined F USING(fieldID) JOIN itemDataValues V USING(valueID) ORDER BY D.itemID,D.fieldID",
        &budget,
        |r| {
            let id = r.get(0)?;
            let Some(item) = items.get_mut(&id) else {
                return Ok(());
            };
            if item.data.fields.len() >= 256 {
                return budget.limit();
            }
            let field = budget.text(r, 1)?;
            let stored = budget.text(r, 2)?;
            // Metadata cannot overwrite mapper envelope fields through flattening.
            if field.is_empty() || ["key", "itemType", "creators", "better_bibtex_keys"].contains(&field.as_str()) {
                return Err(rusqlite::Error::InvalidQuery);
            }
            let typed = if date_field(&item.data.item_type, &field) {
                multipart(&stored).to_owned()
            } else {
                stored.clone()
            };
            budget.charge(typed.len().saturating_add(stored.len()).saturating_add(field.len()))?;
            if item.metadata.stored_fields.insert(field.clone(), stored).is_some()
                || item.data.fields.insert(field, typed).is_some()
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            Ok(())
        },
    )?;
    each(
        &db,
        "SELECT IC.itemID,CT.creatorType,C.firstName,C.lastName,C.fieldMode,IC.orderIndex FROM itemCreators IC JOIN creators C USING(creatorID) JOIN creatorTypes CT USING(creatorTypeID) ORDER BY IC.itemID,IC.orderIndex",
        &budget,
        |r| {
            let id = r.get(0)?;
            let Some(item) = items.get_mut(&id) else {
                return Ok(());
            };
            if item.data.creators.len() >= 4096 {
                return budget.limit();
            }
            let creator_type = budget.text(r, 1)?;
            let first = budget.text(r, 2)?;
            let last = budget.text(r, 3)?;
            let mode: i64 = r.get(4)?;
            let index: i64 = r.get(5)?;
            if ![0, 1].contains(&mode)
                || index < 0
                || index as usize != item.data.creators.len()
                || creator_type.is_empty()
            {
                return Err(rusqlite::Error::InvalidQuery);
            }
            item.data.creators.push(if mode == 1 {
                RawCreator {
                    creator_type,
                    name: last,
                    field_mode: 1,
                    ..RawCreator::default()
                }
            } else {
                RawCreator {
                    creator_type,
                    first_name: first,
                    last_name: last,
                    ..RawCreator::default()
                }
            });
            Ok(())
        },
    )?;
    for (_, item) in items {
        libraries
            .get_mut(&item.metadata.library_id)
            .ok_or(ClosedError::Damaged)?
            .items
            .push(item);
    }
    control.check()?;
    Ok(ClosedSnapshot {
        schema_version: version,
        libraries: libraries.into_values().collect(),
        warnings: Vec::new(),
    })
}

const TABLES: &[(&str, &[&str])] = &[
    ("libraries", &["libraryID", "type"]),
    ("groups", &["groupID", "libraryID"]),
    (
        "items",
        &[
            "itemID",
            "libraryID",
            "key",
            "version",
            "itemTypeID",
            "dateAdded",
            "dateModified",
        ],
    ),
    ("itemTypesCombined", &["itemTypeID", "typeName"]),
    ("fieldsCombined", &["fieldID", "fieldName"]),
    ("itemData", &["itemID", "fieldID", "valueID"]),
    ("itemDataValues", &["valueID", "value"]),
    ("creators", &["creatorID", "firstName", "lastName", "fieldMode"]),
    ("creatorTypes", &["creatorTypeID", "creatorType"]),
    ("itemCreators", &["itemID", "creatorID", "creatorTypeID", "orderIndex"]),
    ("deletedItems", &["itemID"]),
];
fn validate_table(db: &Connection, name: &str, columns: &[&str]) -> Result<(), ClosedError> {
    let sql: String = db
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='table' AND name=?1 AND rootpage>0",
            [name],
            |r| r.get(0),
        )
        .map_err(|_| ClosedError::Damaged)?;
    if sql.len() > 65536 || !sql.trim_start().to_ascii_uppercase().starts_with("CREATE TABLE") {
        return Err(ClosedError::Damaged);
    }
    let mut statement = db
        .prepare("SELECT name,hidden FROM pragma_table_xinfo(?1)")
        .map_err(failure)?;
    let names = statement
        .query_map([name], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
        .map_err(failure)?;
    if names.iter().any(|(_, hidden)| *hidden != 0) || columns.iter().any(|c| !names.iter().any(|(n, _)| n == c)) {
        return Err(ClosedError::Damaged);
    }
    Ok(())
}
fn each(
    db: &Connection,
    sql: &str,
    budget: &Budget,
    mut action: impl FnMut(&Row<'_>) -> rusqlite::Result<()>,
) -> Result<(), ClosedError> {
    let mut statement = db.prepare(sql).map_err(failure)?;
    let mut rows = statement.query([]).map_err(failure)?;
    while let Some(row) = rows.next().map_err(failure)? {
        budget.row().and_then(|_| action(row)).map_err(|e| {
            if budget.exceeded.get() {
                ClosedError::TooLarge
            } else {
                failure(e)
            }
        })?;
    }
    Ok(())
}
struct Budget {
    text: Cell<usize>,
    rows: Cell<usize>,
    exceeded: Cell<bool>,
}
impl Budget {
    fn limit<T>(&self) -> rusqlite::Result<T> {
        self.exceeded.set(true);
        Err(rusqlite::Error::InvalidQuery)
    }
    fn charge(&self, n: usize) -> rusqlite::Result<()> {
        if let Some(left) = self.text.get().checked_sub(n) {
            self.text.set(left);
            Ok(())
        } else {
            self.limit()
        }
    }
    fn row(&self) -> rusqlite::Result<()> {
        if let Some(left) = self.rows.get().checked_sub(1) {
            self.rows.set(left);
            Ok(())
        } else {
            self.limit()
        }
    }
    fn text(&self, r: &Row<'_>, i: usize) -> rusqlite::Result<String> {
        let value = r.get_ref(i)?.as_str()?;
        if value.len() > MAX_VALUE {
            return self.limit();
        }
        self.charge(value.len().saturating_add(128))?;
        Ok(value.to_owned())
    }
}
fn failure(error: rusqlite::Error) -> ClosedError {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::TooBig | rusqlite::ErrorCode::OutOfMemory) => ClosedError::TooLarge,
        Some(rusqlite::ErrorCode::OperationInterrupted) => ClosedError::TimedOut,
        _ => ClosedError::Damaged,
    }
}

fn date_field(kind: &str, field: &str) -> bool {
    matches!(field, "date" | "filingDate" | "originalDate")
        || matches!(
            (kind, field),
            ("case", "dateDecided") | ("patent", "issueDate") | ("statute", "dateEnacted")
        )
}
/// Zotero Date.multipartToStr: leave SQL timestamps intact; remove exactly one
/// valid sortable date prefix, retaining the typed text even when empty.
fn multipart(value: &str) -> &str {
    let b = value.as_bytes();
    let digits = |range: std::ops::Range<usize>| b.get(range).is_some_and(|v| v.iter().all(u8::is_ascii_digit));
    if b.len() < 11 || !digits(0..4) || b[4] != b'-' || !digits(5..7) || b[7] != b'-' || !digits(8..10) || b[10] != b' '
    {
        return value;
    }
    let number = |a: usize| (b[a] - b'0') * 10 + (b[a + 1] - b'0');
    if number(5) > 12 || number(8) > 31 {
        return value;
    }
    let timestamp = (b.len() == 16 || b.len() == 19)
        && digits(11..13)
        && b.get(13) == Some(&b':')
        && digits(14..16)
        && number(5) > 0
        && number(8) > 0
        && number(11) <= 23
        && number(14) <= 59
        && (b.len() == 16 || (b.get(16) == Some(&b':') && digits(17..19) && number(17) <= 59));
    if timestamp { value } else { &value[11..] }
}

#[cfg(test)]
#[path = "database_tests.rs"]
mod tests;
