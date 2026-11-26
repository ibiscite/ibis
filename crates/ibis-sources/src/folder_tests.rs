use super::*;

#[test]
fn a_kept_folder_returns_its_original_entries_without_offering_proposals() {
    use crate::{SourceCapability, load_file, original_entry, reload_file};
    use ibis_core::contract::LibraryObject;

    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(LIBRARY_FILE);
    let text = "@book{atlas,\n  title = {An Invented Atlas},\n  file = {:./atlas.pdf:application/pdf},\n}\n";
    std::fs::write(&path, text).unwrap();
    for loaded in [load_file(&path).unwrap(), reload_file(&path).unwrap()] {
        let item = loaded
            .snapshot
            .objects()
            .iter()
            .find(|item| matches!(item.object, LibraryObject::Record(_)))
            .unwrap();
        assert_eq!(
            original_entry(&path, item.object.id(), "atlas", &item.fingerprints.cite),
            Ok(text.to_owned())
        );
        assert!(loaded.capabilities.contains(&SourceCapability::OriginalEntries));
        assert!(!loaded.capabilities.contains(&SourceCapability::Proposals));
    }
}

fn folder(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("ibis-folder-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn pdfs_are_listed_to_the_depth_limit_without_links_or_hidden_entries() {
    let root = folder("list");
    let mut deep = root.clone();
    for part in ["a", "b", "c", "d", "e"] {
        deep = deep.join(part);
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join(format!("{part}.PDF")), b"%PDF-1.7").unwrap();
    }
    std::fs::write(root.join("tides.pdf"), b"%PDF-1.7").unwrap();
    std::fs::write(root.join("notes.txt"), b"invented").unwrap();
    std::fs::write(root.join(".hidden.pdf"), b"%PDF-1.7").unwrap();
    std::fs::write(root.join("brace{d}.pdf"), b"%PDF-1.7").unwrap();
    std::fs::create_dir_all(root.join(".trash")).unwrap();
    std::fs::write(root.join(".trash/gone.pdf"), b"%PDF-1.7").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("tides.pdf"), root.join("link.pdf")).unwrap();
    let listing = list(&root, &|| false).unwrap();
    assert_eq!(
        listing.pdfs,
        ["a/a.PDF", "a/b/b.PDF", "a/b/c/c.PDF", "a/b/c/d/d.PDF", "tides.pdf"]
    );
    assert!(!listing.cut_short);
    // The fifth level, the hidden file and folder, the braced name and the link.
    assert_eq!(listing.skipped, if cfg!(unix) { 5 } else { 4 });
    assert_eq!(list(&root.join("missing"), &|| false), Err(SourceError::Missing));
    assert_eq!(list(&root, &|| true), Err(SourceError::Cancelled));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_folder_names_its_library_file() {
    let root = folder("names");
    assert_eq!(library_file(&root), root.join(LIBRARY_FILE));
    assert_eq!(library_file(&root.join("refs.bib")), root.join("refs.bib"));
    assert!(is_folder_library(&root.join(LIBRARY_FILE)));
    assert!(!is_folder_library(&root.join("refs.bib")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unkept_pdfs_are_those_no_file_field_names_inside_the_folder() {
    let listing = Listing {
        pdfs: vec!["a/b.pdf".into(), "c.pdf".into(), "d:e.pdf".into()],
        ..Listing::default()
    };
    let named = named([
        "./c.pdf".to_owned(),
        "../outside.pdf".to_owned(),
        "/abs/a/b.pdf".to_owned(),
    ]);
    assert_eq!(unkept(&listing, &named), ["a/b.pdf", "d:e.pdf"]);
    assert_eq!(file_value("d:e;f.pdf"), r":./d\:e\;f.pdf:application/pdf");
}
