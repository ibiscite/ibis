//! The key file is chosen once, before the first id: the same choice again is
//! fine, and any other is refused rather than silently ignored.
use ibis_sources::identity::{KeyAlreadyChosen, use_key_file};

#[test]
fn a_second_different_key_choice_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let chosen = dir.path().join("library-ids.key");
    assert_eq!(use_key_file(chosen.clone(), true), Ok(()));
    assert_eq!(use_key_file(chosen.clone(), true), Ok(()));
    assert_eq!(use_key_file(chosen.clone(), false), Err(KeyAlreadyChosen));
    assert_eq!(use_key_file(dir.path().join("other.key"), true), Err(KeyAlreadyChosen));
}
