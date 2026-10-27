//! Better BibTeX keys from the private copy (see [`crate::bbt`]). A store that
//! is present but cannot be read marks the snapshot's keys unknown; it never
//! fails the library read.
use super::private_copy::Copy;
use super::{ClosedError, ClosedSnapshot, Control, database, database_bounds};
use crate::bbt::{self, StoredKey};
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeMap;
use std::io::Read;

const STORE: &str = "better-bibtex.sqlite";
const JOURNAL: &str = "better-bibtex.sqlite-journal";
const READ_ONLY: &str = "better-bibtex/read-only.json";
/// `read-only.json` is parsed whole, which can take many times its size in
/// memory; real files hold one short record per read-only item. A larger file
/// leaves keys unknown.
const MAX_READ_ONLY: u64 = 16 * 1024 * 1024;

/// Adds Better BibTeX's keys to the snapshot's items. `zotero` is the major
/// version of the Zotero that last opened the database; `skipped` says a
/// Better BibTeX file was there but could not be copied.
pub(super) fn attach(
    copy: &Copy,
    zotero: Option<u32>,
    skipped: bool,
    snapshot: &mut ClosedSnapshot,
    control: &Control,
) -> Result<(), ClosedError> {
    if skipped {
        snapshot.better_bibtex_unknown = true;
        return Ok(());
    }
    let mut unknown = false;
    let mut found = Vec::new();
    if copy.file(STORE).exists() {
        match zotero {
            // Better BibTeX 7 keeps every key here.
            Some(7) => match soft(store(copy, control))? {
                Some(keys) => found.extend(keys.into_iter().map(|key| (key, false))),
                None => unknown = true,
            },
            // Better BibTeX 8 serves Zotero's own field and renames this
            // store once it has migrated it. A store still here means the
            // migration is pending or failed, so Zotero's field may lack keys
            // Better BibTeX will add later.
            _ => unknown = true,
        }
    }
    if copy.file(READ_ONLY).exists() {
        match soft(read_only(copy))? {
            // Better BibTeX serves Zotero's own key over these.
            Some(keys) => found.extend(keys.into_iter().map(|key| (key, true))),
            None => unknown = true,
        }
    }
    control.check()?;
    let mut items = BTreeMap::new();
    for (l, library) in snapshot.libraries.iter().enumerate() {
        for (i, item) in library.items.iter().enumerate() {
            items.insert((library.library_id, item.data.key.clone()), (l, i));
        }
    }
    for (key, only_without_own) in found {
        let Some(&(l, i)) = items.get(&(key.library_id, key.item_key)) else {
            continue;
        };
        let item = &mut snapshot.libraries[l].items[i].data;
        if only_without_own && item.fields.get("citationKey").is_some_and(|own| !own.trim().is_empty()) {
            continue;
        }
        // Two different keys already make the item's key ambiguous.
        if !item.better_bibtex_keys.contains(&key.citation_key) && item.better_bibtex_keys.len() < 2 {
            item.better_bibtex_keys.push(key.citation_key);
        }
    }
    snapshot.better_bibtex_unknown = unknown;
    Ok(())
}

/// Cancellation and the deadline stop the whole read; any other failure only
/// makes the keys unknown.
fn soft<T>(result: Result<T, ClosedError>) -> Result<Option<T>, ClosedError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error @ (ClosedError::Cancelled | ClosedError::TimedOut)) => Err(error),
        Err(_) => Ok(None),
    }
}

fn store(copy: &Copy, control: &Control) -> Result<Vec<StoredKey>, ClosedError> {
    // As for Zotero's database: refuse a hot journal, discard a cold one.
    match std::fs::File::open(copy.file(JOURNAL)) {
        Ok(mut journal) => {
            let mut first = [0];
            if journal.read(&mut first).map_err(|_| ClosedError::PrivateStorage)? != 0 && first[0] != 0 {
                return Err(ClosedError::NeedsRecovery);
            }
            drop(journal);
            std::fs::remove_file(copy.file(JOURNAL)).map_err(|_| ClosedError::PrivateStorage)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(ClosedError::PrivateStorage),
    }
    let path = copy.file(STORE);
    database_bounds::check(&path, control)?;
    let mut db = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(database::failure)?;
    database::configure(&mut db, control)?;
    let check: String = db
        .query_row("PRAGMA quick_check(1)", [], |r| r.get(0))
        .map_err(database::failure)?;
    if check != "ok" {
        return Err(ClosedError::Damaged);
    }
    let keys = bbt::read_store(&db, 7).map_err(|_| ClosedError::Damaged);
    control.check()?;
    keys
}

fn read_only(copy: &Copy) -> Result<Vec<StoredKey>, ClosedError> {
    let file = std::fs::File::open(copy.file(READ_ONLY)).map_err(|_| ClosedError::PrivateStorage)?;
    let mut bytes = Vec::new();
    file.take(MAX_READ_ONLY + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ClosedError::PrivateStorage)?;
    if bytes.len() as u64 > MAX_READ_ONLY {
        return Err(ClosedError::TooLarge);
    }
    bbt::read_only_keys(&bytes).map_err(|_| ClosedError::Damaged)
}

#[cfg(test)]
#[path = "keys_tests.rs"]
mod tests;
