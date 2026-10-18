//! Preserve the documented size limit using only a synthetic library.
use std::io::{Read, Write};

use ibis_core::source::{MAX_LIBRARY_BYTES, SourceError, load_library_file};

#[test]
fn exactly_sixty_four_mib_loads_and_one_more_byte_is_rejected() {
    let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join("参考文献 synthetic.bib");
    let first = b"@misc{first,title={Synthetic first}}\n";
    let last = b"@misc{last,title={Synthetic last}}\n";
    let padding = MAX_LIBRARY_BYTES - (first.len() + last.len()) as u64;
    {
        let mut file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("{e}"));
        file.write_all(first).unwrap_or_else(|e| panic!("{e}"));
        std::io::copy(&mut std::io::repeat(b' ').take(padding), &mut file).unwrap_or_else(|e| panic!("{e}"));
        file.write_all(last).unwrap_or_else(|e| panic!("{e}"));
    }
    assert_eq!(
        std::fs::metadata(&path).unwrap_or_else(|e| panic!("{e}")).len(),
        64 * 1024 * 1024
    );
    for reload in [false, true] {
        let loaded = load_library_file(&path, reload).unwrap_or_else(|e| panic!("{e}"));
        let keys: Vec<_> = loaded.references.iter().map(|r| r.id.native_key.as_str()).collect();
        assert_eq!(keys, ["first", "last"]);
        assert!(loaded.warnings.is_empty());
    }
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap_or_else(|e| panic!("{e}"))
        .write_all(b" ")
        .unwrap_or_else(|e| panic!("{e}"));
    for reload in [false, true] {
        assert_eq!(load_library_file(&path, reload).err(), Some(SourceError::TooLarge));
    }
}
