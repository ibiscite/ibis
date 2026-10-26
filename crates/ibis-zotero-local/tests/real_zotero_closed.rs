//! Reads a real Zotero's library twice: through its local API while it
//! runs, then from a copy of its database after it has been stopped, and
//! requires both to give the same references.
//!
//! Ignored by default: `scripts/real-zotero.sh` runs the first test while
//! its throwaway Zotero runs (after `real_zotero.rs` has added synthetic
//! items), stops Zotero without a clean shutdown, then runs the second.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

use ibis_core::model::LoadResult;
use ibis_core::zotero::Library;
use ibis_core::zotero::api;
use ibis_zotero_local::snapshot::{self, Refresh};
use ibis_zotero_local::{DEFAULT_PORT, LocalZotero};

fn var(name: &str) -> Result<String, Box<dyn Error>> {
    std::env::var(name).map_err(|_| format!("run this through scripts/real-zotero.sh ({name} is not set)").into())
}

/// The references only, without the verbatim source each path keeps.
fn comparable(mut result: LoadResult) -> LoadResult {
    for reference in &mut result.references {
        reference.raw = None;
    }
    result
}

fn contents(dir: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>, Box<dyn Error>> {
    let mut files = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() {
            files.insert(path.clone(), std::fs::read(&path)?);
        }
    }
    Ok(files)
}

#[test]
#[ignore = "needs a running Zotero; see scripts/real-zotero.sh"]
fn the_live_answer_is_saved_for_comparison() -> Result<(), Box<dyn Error>> {
    let port: u16 = var("IBIS_ZOTERO_PORT")?.parse()?;
    if port == DEFAULT_PORT {
        return Err("refusing to read a Zotero someone is using".into());
    }
    let (result, _) = api::scan(&mut LocalZotero::on_port(port), Library::User)?;
    if result.references.len() < 4 {
        return Err("run real_zotero.rs first; it adds the synthetic items".into());
    }
    std::fs::write(var("IBIS_PARITY_FILE")?, serde_json::to_vec(&comparable(result))?)?;
    Ok(())
}

#[test]
#[ignore = "needs a stopped Zotero's data folder; see scripts/real-zotero.sh"]
fn a_closed_zotero_reads_the_same_as_its_local_api() -> Result<(), Box<dyn Error>> {
    let data = PathBuf::from(var("IBIS_ZOTERO_DATA")?);
    let live: LoadResult = serde_json::from_slice(&std::fs::read(var("IBIS_PARITY_FILE")?)?)?;
    let scratch = tempfile::tempdir()?;
    let before = contents(&data)?;
    let outcome = snapshot::read(&data, scratch.path(), &[Library::User], None)?;
    assert_eq!(contents(&data)?, before, "Zotero's files must not change");
    let Refresh::Loaded { result, .. } = outcome else {
        return Err("a first read is never unchanged".into());
    };
    let closed = comparable(result);
    assert_eq!(closed.warnings, live.warnings);
    assert_eq!(closed.references.len(), live.references.len());
    for (closed, live) in closed.references.iter().zip(&live.references) {
        assert_eq!(closed, live, "item {}", live.id.native_key);
    }
    Ok(())
}
