//! A key chosen after ids were made with this process's own key is refused:
//! the ids already given out would no longer match.
use ibis_sources::identity::{KeyAlreadyChosen, use_key_file};

#[test]
fn a_key_chosen_after_the_first_id_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(&path, "@misc{one,title={Invented}}\n").unwrap();
    ibis_sources::load_file(&path).unwrap();
    assert_eq!(
        use_key_file(dir.path().join("library-ids.key"), true),
        Err(KeyAlreadyChosen)
    );
}
