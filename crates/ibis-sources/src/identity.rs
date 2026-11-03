//! Library ids: stable on this computer and meaningless anywhere else.
//!
//! An id is a digest of the source's identity and a private random key, so a
//! seen id cannot confirm a guessed path. The key is created once per person and
//! kept beside the app's settings, shared by the app and the `ibis` command, so
//! both name a library the same way across runs. Without a key file (a test, a
//! read-only home) ids hold for this process only; they are never guessable, and
//! an old id then finds nothing rather than something else.
use crate::LibraryKind;
use ibis_core::contract::LibraryId;
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
};

const KEY_BYTES: usize = 32;
/// Kept in the stable app's folder whichever build runs, so Nightly and the
/// command agree with it.
const KEY_FILE: &str = "com.ibiscite.ibis/library-ids.key";

type Key = [u8; KEY_BYTES];

static FILE: OnceLock<(PathBuf, bool)> = OnceLock::new();
static KEY: OnceLock<(Key, bool)> = OnceLock::new();

/// The folder apps keep their settings in: the same place as Tauri's
/// `app_config_dir`, from the environment so a test or a person can move it.
pub fn config_dir() -> Option<PathBuf> {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        var("HOME").map(|home| home.join("Library/Application Support"))
    } else if cfg!(windows) {
        var("APPDATA")
    } else {
        var("XDG_CONFIG_HOME")
            .filter(|path| path.is_absolute())
            .or_else(|| var("HOME").map(|home| home.join(".config")))
    }
}

/// Use the person's saved key from the first library on, writing one if
/// `create` allows it. Nothing is read or written until a library needs an id.
pub fn use_saved_key(create: bool) {
    if let Some(directory) = config_dir() {
        use_key_file(directory.join(KEY_FILE), create);
    }
}

/// The first call decides, and only before the first id is made.
pub fn use_key_file(path: PathBuf, create: bool) {
    let _ = FILE.set((path, create));
}

/// Whether ids made in this process last beyond it.
pub fn ids_last() -> bool {
    key().1
}

/// The id of the source that `parts` identify. Parts are length-framed, so two
/// different identities can never concatenate to the same input.
pub(crate) fn library_id(kind: LibraryKind, parts: &[&[u8]]) -> LibraryId {
    let mut identity = Vec::with_capacity(KEY_BYTES + 64);
    identity.extend_from_slice(&key().0);
    identity.push(kind as u8);
    for part in parts {
        identity.extend_from_slice(&(part.len() as u64).to_le_bytes());
        identity.extend_from_slice(part);
    }
    LibraryId::digest(&identity)
}

fn key() -> &'static (Key, bool) {
    KEY.get_or_init(|| {
        FILE.get()
            .and_then(|(path, create)| saved(path, *create).ok())
            .map_or_else(|| (random(), false), |key| (key, true))
    })
}

fn random() -> Key {
    let mut key = [0; KEY_BYTES];
    if getrandom::fill(&mut key).is_err() {
        // Nothing else here can fail this way; still never a known key.
        use std::hash::{BuildHasher, Hasher};
        for chunk in key.chunks_mut(8) {
            let word = std::collections::hash_map::RandomState::new().build_hasher().finish();
            chunk.copy_from_slice(&word.to_le_bytes());
        }
    }
    key
}

fn saved(path: &Path, create: bool) -> io::Result<Key> {
    let found = read(path);
    match found {
        Ok(Some(key)) => return Ok(key),
        Ok(None) if create => std::fs::remove_file(path)?,
        Err(ref error) if create && error.kind() == io::ErrorKind::NotFound => {}
        Ok(None) => return Err(io::ErrorKind::InvalidData.into()),
        Err(error) => return Err(error),
    }
    let directory = path.parent().ok_or(io::ErrorKind::InvalidInput)?;
    std::fs::create_dir_all(directory)?;
    let temporary = directory.join(format!(".library-ids.{}.tmp", std::process::id()));
    write_private(&temporary, hex(&random()).as_bytes())?;
    // A hard link cannot replace a key another process saved a moment ago.
    let linked = std::fs::hard_link(&temporary, path);
    let _ = std::fs::remove_file(&temporary);
    match linked {
        Err(error) if error.kind() != io::ErrorKind::AlreadyExists => Err(error),
        _ => read(path)?.ok_or(io::ErrorKind::InvalidData.into()),
    }
}

/// `None` when the file holds no usable key.
fn read(path: &Path) -> io::Result<Option<Key>> {
    if !std::fs::symlink_metadata(path)?.is_file() {
        return Err(io::ErrorKind::InvalidInput.into());
    }
    let mut text = String::new();
    let limit = KEY_BYTES as u64 * 2 + 1;
    if std::fs::File::open(path)?
        .take(limit)
        .read_to_string(&mut text)
        .is_err()
    {
        return Ok(None);
    }
    Ok(parse(&text))
}

fn parse(text: &str) -> Option<Key> {
    let mut key = [0; KEY_BYTES];
    if text.len() != KEY_BYTES * 2 || !text.is_ascii() {
        return None;
    }
    for (byte, pair) in key.iter_mut().zip(text.as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(key)
}

fn hex(key: &Key) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_key_is_made_once_kept_and_replaced_only_when_unusable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/library-ids.key");
        assert_eq!(saved(&path, false).unwrap_err().kind(), io::ErrorKind::NotFound);
        assert!(!path.exists(), "a read-only caller writes nothing");
        let first = saved(&path, true).unwrap();
        assert_eq!(saved(&path, true).unwrap(), first);
        assert_eq!(saved(&path, false).unwrap(), first);
        assert_eq!(std::fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        for damage in ["", "short", &"z".repeat(64), &"a".repeat(200)] {
            std::fs::write(&path, damage).unwrap();
            assert_eq!(saved(&path, false).unwrap_err().kind(), io::ErrorKind::InvalidData);
            assert_ne!(saved(&path, true).unwrap(), first);
        }
    }

    #[test]
    fn an_unusable_location_fails_instead_of_blocking_or_replacing_it() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("library-ids.key");
        std::fs::create_dir(&folder).unwrap();
        assert!(saved(&folder, true).is_err());
        assert!(folder.is_dir());
        let blocked = dir.path().join("file");
        std::fs::write(&blocked, "").unwrap();
        assert!(saved(&blocked.join("library-ids.key"), true).is_err());
    }

    #[test]
    fn two_processes_saving_at_once_agree_on_one_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("library-ids.key");
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let path = path.clone();
                std::thread::spawn(move || {
                    // The same pid can race in a test; keep their temporaries apart.
                    let own = path.with_extension(format!("{:?}", std::thread::current().id()));
                    write_private(&own, hex(&random()).as_bytes()).unwrap();
                    let _ = std::fs::hard_link(&own, &path);
                    std::fs::remove_file(&own).unwrap();
                    saved(&path, true).unwrap()
                })
            })
            .collect();
        let keys: Vec<_> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
        assert!(keys.iter().all(|key| *key == keys[0]));
    }

    #[test]
    fn identities_are_framed_kind_separated_and_never_the_bare_digest() {
        let one = library_id(LibraryKind::File, &[b"ab", b"c"]);
        assert_eq!(one, library_id(LibraryKind::File, &[b"ab", b"c"]));
        assert_ne!(one, library_id(LibraryKind::File, &[b"a", b"bc"]));
        assert_ne!(one, library_id(LibraryKind::Zotero, &[b"ab", b"c"]));
        assert_ne!(library_id(LibraryKind::File, &[b"abc"]), LibraryId::digest(b"abc"));
    }
}
