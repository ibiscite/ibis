//! Query only the disposable copy. No CSL/legacy model adapters live here.
use super::{
    ClosedError, ClosedItem, ClosedLibrary, ClosedSnapshot, ClosedWarning, Control, RawCreator, RawItem,
    SourceMetadata, StoredValue,
};
use rusqlite::config::DbConfig;
use rusqlite::limits::Limit;
use rusqlite::{Connection, OpenFlags, Row};
use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

const MAX_ITEMS: usize = 1_000_000;
const MAX_VALUE: usize = 256 * 1024;
const MAX_FIELDS: usize = 256;
const MAX_CREATORS: usize = 4096;
// Retained strings plus conservative container overhead. Separate from the
// source-file cap: allow room for a 512MiB live payload's allocation overhead.
const MAX_RETAINED_BYTES: usize = 1024 * 1024 * 1024;
// Independent work ceiling; only retained bibliographic rows consume it.
const MAX_ROWS: usize = MAX_ITEMS * 64;

pub(super) fn read(copy: &super::private_copy::Copy, control: &Control) -> Result<ClosedSnapshot, ClosedError> {
    control.check()?;
    super::database_bounds::check(copy, control)?;
    let path = copy.database_path();
    let dir = path.parent().ok_or(ClosedError::PrivateStorage)?;
    // A hot rollback journal may name an external super-journal. A zeroed
    // PERSIST journal is not hot (SQLite pager.c hasHotJournal); discard only
    // that private journal so SQLite never interprets its path instructions.
    match std::fs::File::open(dir.join("zotero.sqlite-journal")) {
        Ok(mut journal) => {
            use std::io::Read;
            let mut first = [0];
            let count = journal.read(&mut first).map_err(|_| ClosedError::PrivateStorage)?;
            if count != 0 && first[0] != 0 {
                return Err(ClosedError::NeedsRecovery);
            }
            drop(journal);
            std::fs::remove_file(dir.join("zotero.sqlite-journal")).map_err(|_| ClosedError::PrivateStorage)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(ClosedError::PrivateStorage),
    }
    let mut db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(failure)?;
    configure(&mut db, control)?;
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
        text: Cell::new(MAX_RETAINED_BYTES),
        rows: Cell::new(MAX_ROWS),
        exceeded: Cell::new(false),
    };
    let mut libraries = BTreeMap::new();
    each(&db, READ_SQL[0], &budget, |r| {
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
    })?;
    let mut items = BTreeMap::<i64, ClosedItem>::new();
    let mut warnings = Vec::new();
    let mut identities = BTreeSet::new();
    each(&db, READ_SQL[1], &budget, |r| {
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
                stored_values: BTreeMap::new(),
            },
        };
        if items.insert(id, item).is_some() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    })?;
    each(&db, READ_SQL[2], &budget, |r| {
        let id = r.get(0)?;
        if !items.contains_key(&id) || byte_len(r, 4)? > MAX_VALUE {
            budget.discard(r, &[1], &[4])?;
            skip_item(&mut items, id, &mut warnings, &budget)?;
            return Ok(());
        }
        let Some(item) = items.get_mut(&id) else {
            return Ok(());
        };
        if item.data.fields.len()
            + item
                .metadata
                .stored_values
                .values()
                .filter(|v| matches!(v, StoredValue::Null))
                .count()
            >= MAX_FIELDS
        {
            budget.discard(r, &[1], &[4])?;
            skip_item(&mut items, id, &mut warnings, &budget)?;
            return Ok(());
        }
        let field = budget.text(r, 1)?;
        if item.data.fields.contains_key(&field) || item.metadata.stored_values.contains_key(&field) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        if r.get::<_, Option<i64>>(3)?.is_none() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let (stored, original) = budget.value(r, 2)?;
        if let Some(original) = original {
            budget.charge(field.len().saturating_add(128))?;
            item.metadata.stored_values.insert(field.clone(), original);
        }
        let Some(stored) = stored else {
            return Ok(());
        };
        // Metadata cannot overwrite mapper envelope fields through flattening.
        if field.is_empty() || ["key", "itemType", "creators", "better_bibtex_keys"].contains(&field.as_str()) {
            return Err(rusqlite::Error::InvalidQuery);
        }
        let decoded = if date_field(&item.data.item_type, &field) {
            multipart(&stored)
        } else {
            &stored
        };
        let typed = if decoded != stored {
            // Only changed text needs a second retained string. Unchanged
            // source text is already preserved by the neutral mapper input.
            budget.charge(decoded.len().saturating_add(field.len()).saturating_add(128))?;
            let typed = decoded.to_owned();
            item.metadata.stored_fields.insert(field.clone(), stored);
            typed
        } else {
            stored
        };
        item.data.fields.insert(field, typed);
        Ok(())
    })?;
    let mut creators = BTreeMap::<i64, BTreeMap<i64, RawCreator>>::new();
    each(&db, READ_SQL[3], &budget, |r| {
        let id = r.get(0)?;
        if !items.contains_key(&id) {
            budget.discard(r, &[1], &[6, 7])?;
            return Ok(());
        }
        let index: i64 = r.get(5)?;
        if creators.get(&id).is_some_and(|ordered| ordered.len() >= MAX_CREATORS)
            || index >= MAX_CREATORS as i64
            || byte_len(r, 6)? > MAX_VALUE
            || byte_len(r, 7)? > MAX_VALUE
        {
            budget.discard(r, &[1], &[6, 7])?;
            skip_item(&mut items, id, &mut warnings, &budget)?;
            creators.remove(&id);
            return Ok(());
        }
        let creator_type = budget.text(r, 1)?;
        let first = budget.text(r, 2)?;
        let last = budget.text(r, 3)?;
        let mode: i64 = r.get(4)?;
        if ![0, 1].contains(&mode) || index < 0 || creator_type.is_empty() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        budget.charge(256)?;
        let creator = if mode == 1 {
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
        };
        if creators.entry(id).or_default().insert(index, creator).is_some() {
            return Err(rusqlite::Error::InvalidQuery);
        }
        Ok(())
    })?;
    for (id, mut item) in items {
        if let Some(ordered) = creators.remove(&id) {
            if ordered
                .keys()
                .enumerate()
                .any(|(expected, actual)| expected as i64 != *actual)
            {
                return Err(ClosedError::Damaged);
            }
            item.data.creators = ordered.into_values().collect();
        }
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
        warnings,
    })
}

fn skip_item(
    items: &mut BTreeMap<i64, ClosedItem>,
    id: i64,
    warnings: &mut Vec<ClosedWarning>,
    budget: &Budget,
) -> rusqlite::Result<()> {
    if let Some(item) = items.remove(&id) {
        budget.charge(128)?;
        warnings.push(ClosedWarning::ItemTooLarge {
            library_id: item.metadata.library_id,
            key: item.data.key,
        });
    }
    Ok(())
}

fn byte_len(row: &Row<'_>, column: usize) -> rusqlite::Result<usize> {
    usize::try_from(row.get::<_, Option<i64>>(column)?.unwrap_or_default()).map_err(|_| rusqlite::Error::InvalidQuery)
}

fn configure(db: &mut Connection, control: &Control) -> Result<(), ClosedError> {
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
        (Limit::SQLITE_LIMIT_SCHEMA, 1024),
        (Limit::SQLITE_LIMIT_PARSER_DEPTH, 100),
        (Limit::SQLITE_LIMIT_TRIGGER_STEPS, 256),
    ] {
        db.set_limit(limit, value).map_err(failure)?;
    }
    let progress = control.clone();
    db.progress_handler(10000, Some(move || progress.check().is_err()))
        .map_err(failure)?;
    db.execute_batch(
        "PRAGMA temp_store=MEMORY; PRAGMA cache_size=-2048; PRAGMA query_only=ON; PRAGMA automatic_index=OFF;",
    )
    .map_err(failure)?;
    Ok(())
}

const READ_SQL: [&str; 4] = [
    "SELECT L.libraryID,L.type,G.groupID FROM libraries L LEFT JOIN groups G USING(libraryID) WHERE L.type IN ('user','group')",
    "SELECT I.itemID,I.libraryID,I.key,I.version,T.typeName,I.dateAdded,I.dateModified FROM items I LEFT JOIN itemTypesCombined T USING(itemTypeID) WHERE (T.typeName IS NULL OR T.typeName NOT IN ('note','attachment','annotation')) AND NOT EXISTS(SELECT 1 FROM deletedItems D WHERE D.itemID=I.itemID)",
    "SELECT D.itemID,F.fieldName,CASE WHEN octet_length(V.value)>:max_value THEN NULL ELSE V.value END,V.valueID,octet_length(V.value) FROM itemData D LEFT JOIN fieldsCombined F USING(fieldID) LEFT JOIN itemDataValues V USING(valueID) WHERE EXISTS(SELECT 1 FROM items I JOIN libraries L USING(libraryID) JOIN itemTypesCombined T USING(itemTypeID) WHERE I.itemID=D.itemID AND L.type IN ('user','group') AND T.typeName NOT IN ('note','attachment','annotation') AND NOT EXISTS(SELECT 1 FROM deletedItems X WHERE X.itemID=I.itemID))",
    "SELECT IC.itemID,CT.creatorType,CASE WHEN octet_length(C.firstName)>:max_value THEN NULL ELSE C.firstName END,CASE WHEN octet_length(C.lastName)>:max_value THEN NULL ELSE C.lastName END,C.fieldMode,IC.orderIndex,octet_length(C.firstName),octet_length(C.lastName) FROM itemCreators IC LEFT JOIN creators C USING(creatorID) LEFT JOIN creatorTypes CT USING(creatorTypeID) WHERE EXISTS(SELECT 1 FROM items I JOIN libraries L USING(libraryID) JOIN itemTypesCombined T USING(itemTypeID) WHERE I.itemID=IC.itemID AND L.type IN ('user','group') AND T.typeName NOT IN ('note','attachment','annotation') AND NOT EXISTS(SELECT 1 FROM deletedItems X WHERE X.itemID=I.itemID))",
];

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
    let count = statement.parameter_count();
    let mut rows = statement
        .query(rusqlite::params_from_iter([MAX_VALUE as i64].into_iter().take(count)))
        .map_err(failure)?;
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
    // Omitted rows still consume the global work/text allowance. Read their
    // byte lengths without allocating the oversized field or creator strings.
    fn discard(&self, row: &Row<'_>, text: &[usize], lengths: &[usize]) -> rusqlite::Result<()> {
        for &column in text {
            self.charge(row.get_ref(column)?.as_str()?.len().saturating_add(128))?;
        }
        for &column in lengths {
            self.charge(byte_len(row, column)?.saturating_add(128))?;
        }
        Ok(())
    }
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
    fn value(&self, r: &Row<'_>, i: usize) -> rusqlite::Result<(Option<String>, Option<StoredValue>)> {
        use rusqlite::types::ValueRef;
        let (number, original) = match r.get_ref(i)? {
            ValueRef::Text(_) => return self.text(r, i).map(|s| (Some(s), None)),
            ValueRef::Integer(n) => (n as f64, StoredValue::Integer(n)),
            ValueRef::Real(n) => (n, StoredValue::RealBits(n.to_bits())),
            ValueRef::Null => {
                self.charge(128)?;
                return Ok((None, Some(StoredValue::Null)));
            }
            _ => return Err(rusqlite::Error::InvalidQuery),
        };
        let mut formatter = ryu_js::Buffer::new();
        let value = formatter.format(number);
        self.charge(value.len().saturating_add(128))?;
        Ok((Some(value.to_owned()), Some(original)))
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
