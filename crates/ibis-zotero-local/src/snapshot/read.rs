//! Reading the private copy of Zotero's database with SQLite.
//!
//! Only tables and columns that Zotero 7 to 10 share are read, by name, and
//! only from database layouts in [`SUPPORTED`]; anything else is refused
//! rather than guessed at. The copy is opened with triggers, views and
//! untrusted schema functions off, so a damaged or hostile file can only
//! fail to read, and every query stops at the read's deadline.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Instant;

use ibis_core::model::{LoadResult, LoadWarning, WarningKind};
use ibis_core::zotero::Library;
use ibis_core::zotero::database::{self, Creator, Item, Schema};
use rusqlite::config::DbConfig;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, Row};

use super::SnapshotError;

/// Zotero's `userdata` schema versions known to have the tables and columns
/// read here: 123 is Zotero 7.0, 130 the newest Zotero 10 release checked.
const SUPPORTED: std::ops::RangeInclusive<i64> = 123..=130;
/// Items read before giving up, as for the local API.
const MAX_ITEMS: usize = 1_000_000;
/// Largest inflated global schema: Zotero's is under a megabyte.
const MAX_SCHEMA_BYTES: usize = 64 * 1024 * 1024;
/// Attachment link mode for a link to a web page, which is no best attachment.
const LINKED_URL: i64 = 3;

/// The tables and columns read, checked before anything else is.
const COLUMNS: [(&str, &[&str]); 16] = [
    ("version", &["schema", "version"]),
    ("settings", &["setting", "key", "value"]),
    ("libraries", &["libraryID", "type"]),
    ("groups", &["groupID", "libraryID"]),
    (
        "items",
        &[
            "itemID",
            "itemTypeID",
            "libraryID",
            "key",
            "version",
            "dateAdded",
            "dateModified",
        ],
    ),
    ("itemTypesCombined", &["itemTypeID", "typeName"]),
    ("fieldsCombined", &["fieldID", "fieldName"]),
    ("baseFieldMappingsCombined", &["itemTypeID", "baseFieldID", "fieldID"]),
    ("itemTypeCreatorTypes", &["itemTypeID", "creatorTypeID", "primaryField"]),
    ("itemData", &["itemID", "fieldID", "valueID"]),
    ("itemDataValues", &["valueID", "value"]),
    ("creators", &["creatorID", "firstName", "lastName", "fieldMode"]),
    ("itemCreators", &["itemID", "creatorID", "creatorTypeID", "orderIndex"]),
    ("creatorTypes", &["creatorTypeID", "creatorType"]),
    ("itemTags", &["itemID", "tagID", "type"]),
    (
        "itemAttachments",
        &["itemID", "parentItemID", "linkMode", "contentType"],
    ),
];
const MORE_COLUMNS: [(&str, &[&str]); 2] = [("tags", &["tagID", "name"]), ("deletedItems", &["itemID"])];

/// Read the chosen libraries from the copy in `dir`.
pub(super) fn libraries(dir: &Path, wanted: &[Library], deadline: Instant) -> Result<LoadResult, SnapshotError> {
    let db = open(&dir.join("zotero.sqlite"), deadline)?;
    let version: i64 = db
        .query_row("SELECT version FROM version WHERE schema = 'userdata'", [], |row| {
            row.get(0)
        })
        .map_err(failure)?;
    if version < *SUPPORTED.start() {
        return Err(SnapshotError::TooOld);
    }
    if version > *SUPPORTED.end() {
        return Err(SnapshotError::TooNew);
    }
    for (table, columns) in COLUMNS.iter().chain(&MORE_COLUMNS) {
        let mut statement = db.prepare("SELECT name FROM pragma_table_info(?1)").map_err(failure)?;
        let found: Vec<String> = statement
            .query_map([table], |row| row.get(0))
            .and_then(Iterator::collect)
            .map_err(failure)?;
        if columns.iter().any(|c| !found.iter().any(|f| f == c)) {
            return Err(SnapshotError::TooNew);
        }
    }
    let schema = schema(&db)?;

    let mut result = LoadResult::default();
    let ids = library_ids(&db)?;
    let mut chosen = Vec::new();
    for &library in wanted {
        if chosen.iter().any(|(seen, _)| *seen == library) {
            continue;
        }
        match ids.get(&library) {
            Some(&id) => chosen.push((library, id)),
            None => result.warnings.push(LoadWarning {
                kind: WarningKind::Syntax,
                line: None,
                key: None,
                message: format!(
                    "{} was skipped: Zotero has no such group on this computer",
                    name(library)
                ),
            }),
        }
    }
    let mut items = items(&db, &chosen.iter().map(|(_, id)| *id).collect::<Vec<_>>())?;
    let keys = citation_keys(dir, &items, deadline);
    if keys.is_err() {
        result.warnings.push(LoadWarning {
            kind: WarningKind::Syntax,
            line: None,
            key: None,
            message: "Better BibTeX's citation keys could not be read; Zotero's own keys are used".to_string(),
        });
    }
    for (item_id, key) in keys.unwrap_or_default() {
        if let Some((_, item)) = items.get_mut(&item_id) {
            item.better_bibtex_key = Some(key);
        }
    }
    let mut by_library: BTreeMap<i64, Vec<(String, i64, Item)>> = BTreeMap::new();
    for (item_id, (library_id, item)) in items {
        by_library
            .entry(library_id)
            .or_default()
            .push((item.date_added.clone(), item_id, item));
    }
    for (library, id) in chosen {
        let mut list = by_library.remove(&id).unwrap_or_default();
        // Oldest first, as the local API lists them.
        list.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        let list: Vec<Item> = list.into_iter().map(|(_, _, item)| item).collect();
        let loaded = database::load(library, &schema, &list);
        result.references.extend(loaded.references);
        result.warnings.extend(loaded.warnings);
    }
    Ok(result)
}

/// Open a private copy, recover its WAL or journal, and check it.
fn open(path: &Path, deadline: Instant) -> Result<Connection, SnapshotError> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let mut db = Connection::open_with_flags(path, flags).map_err(|_| SnapshotError::Unreadable)?;
    for (config, on) in [
        (DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false),
        (DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false),
        (DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false),
    ] {
        db.set_db_config(config, on).map_err(|_| SnapshotError::Unreadable)?;
    }
    db.progress_handler(10_000, Some(move || Instant::now() > deadline))
        .map_err(|_| SnapshotError::Unreadable)?;
    let check: String = db
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(failure)?;
    if check != "ok" {
        return Err(SnapshotError::Damaged);
    }
    Ok(db)
}

fn failure(error: rusqlite::Error) -> SnapshotError {
    match error.sqlite_error_code() {
        Some(rusqlite::ErrorCode::OperationInterrupted) => SnapshotError::TimedOut,
        Some(rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase) => SnapshotError::Damaged,
        _ => SnapshotError::Unreadable,
    }
}

/// Any stored value as text, as Zotero reads it.
fn text(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    Ok(match row.get_ref(index)? {
        ValueRef::Null => String::new(),
        ValueRef::Integer(n) => n.to_string(),
        ValueRef::Real(n) => n.to_string(),
        ValueRef::Text(t) | ValueRef::Blob(t) => String::from_utf8_lossy(t).into_owned(),
    })
}

fn schema(db: &Connection) -> Result<Schema, SnapshotError> {
    let compressed: Vec<u8> = db
        .query_row(
            "SELECT value FROM settings WHERE setting = 'globalSchema' AND key = 'data'",
            [],
            |row| Ok(row.get_ref(0)?.as_bytes_or_null()?.unwrap_or_default().to_vec()),
        )
        .map_err(failure)?;
    let json = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&compressed, MAX_SCHEMA_BYTES)
        .map_err(|_| SnapshotError::Unreadable)?;
    let rows = |sql: &str, columns: usize| -> Result<Vec<Vec<String>>, SnapshotError> {
        let mut statement = db.prepare(sql).map_err(failure)?;
        statement
            .query_map([], |row| (0..columns).map(|i| text(row, i)).collect())
            .and_then(Iterator::collect)
            .map_err(failure)
    };
    let base = rows(
        "SELECT T.typeName, B.fieldName, F.fieldName FROM baseFieldMappingsCombined M \
         JOIN itemTypesCombined T USING (itemTypeID) \
         JOIN fieldsCombined B ON B.fieldID = M.baseFieldID JOIN fieldsCombined F ON F.fieldID = M.fieldID",
        3,
    )?;
    let primary = rows(
        "SELECT T.typeName, C.creatorType FROM itemTypeCreatorTypes X \
         JOIN itemTypesCombined T USING (itemTypeID) JOIN creatorTypes C USING (creatorTypeID) \
         WHERE X.primaryField = 1",
        2,
    )?;
    Schema::new(
        &json,
        base.into_iter().map(|r| (r[0].clone(), r[1].clone(), r[2].clone())),
        primary.into_iter().map(|r| (r[0].clone(), r[1].clone())),
    )
    .ok_or(SnapshotError::Unreadable)
}

/// Zotero's own library IDs for the personal library and each group.
fn library_ids(db: &Connection) -> Result<HashMap<Library, i64>, SnapshotError> {
    let mut statement = db
        .prepare(
            "SELECT L.libraryID, L.type, G.groupID FROM libraries L LEFT JOIN groups G USING (libraryID) \
             WHERE L.type IN ('user', 'group')",
        )
        .map_err(failure)?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, text(row, 1)?, row.get::<_, Option<i64>>(2)?))
        })
        .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
        .map_err(failure)?;
    let mut ids = HashMap::new();
    for (id, kind, group) in rows {
        let library = match (kind.as_str(), group.and_then(|g| u64::try_from(g).ok())) {
            ("user", _) => Library::User,
            ("group", Some(group)) => Library::Group(group),
            _ => continue,
        };
        ids.entry(library).or_insert(id);
    }
    Ok(ids)
}

/// How an attachment ranks as its parent's best: not a PDF, URL not the
/// parent's, date added, item ID; the smallest is best.
type Rank = (bool, bool, String, i64);

/// Every regular item outside the trash in the given libraries, by item ID,
/// with its library ID.
fn items(db: &Connection, libraries: &[i64]) -> Result<HashMap<i64, (i64, Item)>, SnapshotError> {
    let mut items = HashMap::new();
    if libraries.is_empty() {
        return Ok(items);
    }
    let list = libraries.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
    let regular = format!(
        "SELECT I.itemID FROM items I JOIN itemTypesCombined T USING (itemTypeID) \
         WHERE I.libraryID IN ({list}) AND T.typeName NOT IN ('note', 'attachment', 'annotation') \
         AND I.itemID NOT IN (SELECT itemID FROM deletedItems)"
    );
    let mut statement = db
        .prepare(&format!(
            "SELECT I.itemID, I.libraryID, I.key, I.version, T.typeName, I.dateAdded, I.dateModified \
             FROM items I JOIN itemTypesCombined T USING (itemTypeID) WHERE I.itemID IN ({regular})"
        ))
        .map_err(failure)?;
    let mut rows = statement.query([]).map_err(failure)?;
    while let Some(row) = rows.next().map_err(failure)? {
        if items.len() >= MAX_ITEMS {
            return Err(SnapshotError::TooLarge);
        }
        let item = Item {
            key: text(row, 2).map_err(failure)?,
            version: row.get::<_, i64>(3).map_err(failure)?.max(0).unsigned_abs(),
            item_type: text(row, 4).map_err(failure)?,
            date_added: text(row, 5).map_err(failure)?,
            date_modified: text(row, 6).map_err(failure)?,
            ..Item::default()
        };
        let id: i64 = row.get(0).map_err(failure)?;
        items.insert(id, (row.get(1).map_err(failure)?, item));
    }
    let mut each = |sql: String, add: &mut dyn FnMut(&mut Item, &Row<'_>) -> rusqlite::Result<()>| {
        let mut statement = db.prepare(&sql).map_err(failure)?;
        let mut rows = statement.query([]).map_err(failure)?;
        while let Some(row) = rows.next().map_err(failure)? {
            let id: i64 = row.get(0).map_err(failure)?;
            if let Some((_, item)) = items.get_mut(&id) {
                add(item, row).map_err(failure)?;
            }
        }
        Ok::<_, SnapshotError>(())
    };
    each(
        format!(
            "SELECT D.itemID, F.fieldName, V.value FROM itemData D JOIN fieldsCombined F USING (fieldID) \
             JOIN itemDataValues V USING (valueID) WHERE D.itemID IN ({regular})"
        ),
        &mut |item, row| {
            item.fields.insert(text(row, 1)?, text(row, 2)?);
            Ok(())
        },
    )?;
    each(
        format!(
            "SELECT IC.itemID, CT.creatorType, C.firstName, C.lastName, C.fieldMode FROM itemCreators IC \
             JOIN creators C USING (creatorID) JOIN creatorTypes CT USING (creatorTypeID) \
             WHERE IC.itemID IN ({regular}) ORDER BY IC.itemID, IC.orderIndex"
        ),
        &mut |item, row| {
            item.creators.push(Creator {
                creator_type: text(row, 1)?,
                first_name: text(row, 2)?,
                last_name: text(row, 3)?,
                single_field: row.get::<_, Option<i64>>(4)? == Some(1),
            });
            Ok(())
        },
    )?;
    each(
        format!(
            "SELECT IT.itemID, T.name, IT.type FROM itemTags IT JOIN tags T USING (tagID) \
             WHERE IT.itemID IN ({regular}) ORDER BY IT.itemID, IT.tagID"
        ),
        &mut |item, row| {
            let kind = u8::try_from(row.get::<_, Option<i64>>(2)?.unwrap_or(0)).unwrap_or(0);
            item.tags.push((text(row, 1)?, kind));
            Ok(())
        },
    )?;
    // Zotero's best attachment: a PDF first, then one whose URL is the
    // item's, then the oldest. Links to web pages and the trash are left out.
    let mut best: HashMap<i64, (Rank, Option<String>)> = HashMap::new();
    each(
        format!(
            "SELECT A.parentItemID, A.itemID, A.contentType, V.value, I.dateAdded FROM itemAttachments A \
             JOIN items I USING (itemID) \
             LEFT JOIN itemData D ON D.itemID = A.itemID AND D.fieldID = (SELECT fieldID FROM fieldsCombined WHERE fieldName = 'url') \
             LEFT JOIN itemDataValues V ON V.valueID = D.valueID \
             WHERE A.parentItemID IN ({regular}) AND A.linkMode IS NOT {LINKED_URL} \
             AND A.itemID NOT IN (SELECT itemID FROM deletedItems)"
        ),
        &mut |item, row| {
            let content: Option<String> = row.get_ref(2)?.as_str_or_null()?.map(str::to_string);
            let url: Option<String> = row.get_ref(3)?.as_str_or_null()?.map(str::to_string);
            let matches = url.is_some() && url.as_ref() == item.fields.get("url");
            let rank = (
                content.as_deref() != Some("application/pdf"),
                !matches,
                text(row, 4)?,
                row.get::<_, i64>(1)?,
            );
            let key: i64 = row.get(0)?;
            match best.get(&key) {
                Some((seen, _)) if *seen <= rank => {}
                _ => {
                    best.insert(key, (rank, content));
                }
            }
            let _ = item;
            Ok(())
        },
    )?;
    for (id, (_, content)) in best {
        if let Some((_, item)) = items.get_mut(&id) {
            item.attachment_type = content;
        }
    }
    Ok(items)
}

/// Better BibTeX's citation keys for Zotero 7, which keeps them in its own
/// database; from Zotero 8 on it writes Zotero's citation key field. A row
/// counts only if its item key and library match the item's.
fn citation_keys(
    dir: &Path,
    items: &HashMap<i64, (i64, Item)>,
    deadline: Instant,
) -> Result<Vec<(i64, String)>, SnapshotError> {
    let path = dir.join("better-bibtex.sqlite");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let db = open(&path, deadline)?;
    let mut statement = db
        .prepare("SELECT itemID, itemKey, libraryID, citationKey FROM citationkey")
        .map_err(failure)?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                text(row, 1)?,
                row.get::<_, i64>(2)?,
                text(row, 3)?,
            ))
        })
        .and_then(Iterator::collect::<rusqlite::Result<Vec<_>>>)
        .map_err(failure)?;
    Ok(rows
        .into_iter()
        .filter(|(id, key, library, _)| {
            items
                .get(id)
                .is_some_and(|(own, item)| own == library && &item.key == key)
        })
        .map(|(id, _, _, citation_key)| (id, citation_key))
        .collect())
}

fn name(library: Library) -> String {
    match library {
        Library::User => "Your Zotero library".to_string(),
        Library::Group(id) => format!("Zotero group {id}"),
    }
}
