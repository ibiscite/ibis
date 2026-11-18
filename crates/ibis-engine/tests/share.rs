//! A pack of references (#1953): what the receiver gets, and what never
//! leaves. Every library, note and file here is invented.
#![cfg(test)]
use ibis_core::contract::{ErrorCode, ErrorReason};
use ibis_engine::{Cancellation, Engine, Method, ShareResult};
use serde_json::{Value, json};
use std::io::Read;
use std::path::Path;

const LIBRARY: &str = r"@article{aster2026lanterns,
  author = {Aster, Mira and Vale, Tomas},
  title = {Brass lanterns at dusk},
  journal = {Journal of Invented Light},
  volume = {4},
  year = {2026},
  doi = {10.5555/lantern.1},
  url = {http://192.168.1.7/private/aster},
  note = {SECRET-NOTE-TEXT},
  annote = {SECRET-ANNOTATION-TEXT},
  file = {papers/aster.pdf}
}
@book{aster2026guide,
  author = {Aster, Mira},
  title = {Brass lanterns at dusk},
  publisher = {Invented Press},
  year = {2026},
  file = {papers/ASTER-guide.pdf}
}
@misc{fen2025clouds,
  author = {Fen, Ola},
  title = {Counting clouds},
  year = {2025},
  url = {https://example.org/clouds},
  file = {../outside.pdf}
}
";

struct Fixture {
    _directory: tempfile::TempDir,
    root: std::path::PathBuf,
    engine: Engine,
}

fn fixture() -> Fixture {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("library/papers")).unwrap();
    std::fs::write(root.join("library/papers/aster.pdf"), b"%PDF-1.7\ninvented article\n").unwrap();
    std::fs::write(
        root.join("library/papers/ASTER-guide.pdf"),
        b"%PDF-1.7\ninvented guide, longer\n",
    )
    .unwrap();
    std::fs::write(root.join("outside.pdf"), b"%PDF-1.7\noutside the library\n").unwrap();
    let library = root.join("library/refs.bib");
    std::fs::write(&library, LIBRARY).unwrap();
    let engine = Engine::new().unwrap().without_file_watches();
    engine
        .dispatch(
            Method::Libraries,
            &json!({"action":"open-file","path":library}),
            &Cancellation::default(),
        )
        .unwrap();
    Fixture {
        _directory: directory,
        root,
        engine,
    }
}

fn share(engine: &Engine, params: Value) -> Result<(ShareResult, u8), ibis_core::contract::EngineError> {
    let mut params = params;
    if params.get("items").is_none() {
        params["items"] = json!([
            {"by":"key","value":"aster2026lanterns"},
            {"by":"key","value":"aster2026guide"},
            {"by":"key","value":"fen2025clouds"},
            {"by":"key","value":"aster2026lanterns"},
        ]);
    }
    let reply = engine.dispatch(Method::Share, &params, &Cancellation::default())?;
    let result = serde_json::from_value(reply.data.clone()).unwrap();
    Ok((result, reply.exit_code.value()))
}

fn paths(result: &ShareResult) -> Vec<(String, u64)> {
    result
        .files
        .iter()
        .map(|file| (file.path.clone(), file.bytes))
        .collect()
}

/// Each file of a written zip, by name, read by the zip crate's own reader.
fn unzip(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (file.name().to_owned(), bytes)
        })
        .filter(|(name, _)| !name.ends_with('/'))
        .collect()
}

#[test]
fn the_preview_is_the_pack_and_nothing_private_leaves() {
    let fixture = fixture();
    let (preview, exit) = share(&fixture.engine, json!({"pdfs": true})).unwrap();
    assert!(!preview.written);
    // The outside file is not packed, so the clouds reference has no PDF.
    assert_eq!(preview.without_pdf, [2]);
    assert_eq!(exit, 1, "a reference without its PDF is a finding");
    let destination = fixture.root.join("Reading pack.zip");
    let (written, _) = share(&fixture.engine, json!({"pdfs": true, "destination": destination})).unwrap();
    assert!(written.written);
    assert_eq!(paths(&written), paths(&preview));
    assert_eq!(written.total_bytes, preview.total_bytes);
    let files = unzip(&destination);
    assert_eq!(
        files
            .iter()
            .map(|(name, bytes)| (name.clone(), bytes.len() as u64))
            .collect::<Vec<_>>(),
        paths(&written)
    );
    let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Reading list.html",
            "References.bib",
            "PDFs/Aster 2026 - Brass lanterns at dusk.pdf",
            // The same name to a system that ignores case, numbered in order.
            "PDFs/Aster 2026 - Brass lanterns at dusk (2).pdf",
        ]
    );
    assert_eq!(files[2].1, b"%PDF-1.7\ninvented article\n");
    let private = [
        "SECRET-NOTE-TEXT",
        "SECRET-ANNOTATION-TEXT",
        "192.168.1.7",
        "papers/",
        "outside.pdf",
        fixture.root.to_str().unwrap(),
    ];
    for (name, bytes) in &files {
        let text = String::from_utf8_lossy(bytes);
        for secret in private {
            assert!(!text.contains(secret) && !name.contains(secret), "{secret} in {name}");
        }
    }
    let page = String::from_utf8_lossy(&files[0].1);
    assert!(page.starts_with("<!doctype html>"), "{page}");
    assert!(page.contains("<i>Journal of Invented Light</i>"), "{page}");
    assert!(
        page.contains("<a href=\"https://doi.org/10.5555/lantern.1\">"),
        "{page}"
    );
    assert!(page.contains("<a href=\"https://example.org/clouds\">"), "{page}");
    // Each reference once, though it was picked twice.
    assert_eq!(page.matches("<li>").count(), 3, "{page}");
    let bib = String::from_utf8_lossy(&files[1].1);
    assert_eq!(bib.matches('@').count(), 3, "{bib}");
    assert!(bib.contains("@article{aster2026lanterns,"), "{bib}");
}

#[test]
fn pdfs_stay_out_until_asked_and_a_folder_holds_the_same_files() {
    let fixture = fixture();
    let (without, exit) = share(&fixture.engine, json!({})).unwrap();
    assert_eq!(
        without.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(),
        ["Reading list.html", "References.bib"]
    );
    assert!(without.without_pdf.is_empty());
    assert_eq!(exit, 0);
    let destination = fixture.root.join("pack");
    let (written, _) = share(
        &fixture.engine,
        json!({"pdfs": true, "folder": true, "destination": destination}),
    )
    .unwrap();
    for file in &written.files {
        let bytes = std::fs::read(destination.join(&file.path)).unwrap();
        assert_eq!(bytes.len() as u64, file.bytes, "{}", file.path);
    }
    assert_eq!(std::fs::read_dir(destination.join("PDFs")).unwrap().count(), 2);
}

#[test]
fn nothing_already_there_is_replaced() {
    let fixture = fixture();
    let file = fixture.root.join("taken.zip");
    std::fs::write(&file, b"someone else's file").unwrap();
    let folder = fixture.root.join("taken");
    std::fs::create_dir(&folder).unwrap();
    for (destination, folder_mode) in [(&file, false), (&folder, true), (&file, true), (&folder, false)] {
        let refused = share(
            &fixture.engine,
            json!({"destination": destination, "folder": folder_mode}),
        )
        .unwrap_err();
        assert_eq!(refused.code, ErrorCode::BadInput);
        assert_eq!(refused.reason, Some(ErrorReason::DestinationExists));
    }
    assert_eq!(std::fs::read(&file).unwrap(), b"someone else's file");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
    let relative = share(&fixture.engine, json!({"destination": "pack.zip"})).unwrap_err();
    assert_eq!(relative.reason, Some(ErrorReason::DestinationUnwritable));
    let missing = share(
        &fixture.engine,
        json!({"destination": fixture.root.join("no such folder/pack.zip")}),
    )
    .unwrap_err();
    assert_eq!(missing.reason, Some(ErrorReason::DestinationUnwritable));
}

#[cfg(unix)]
#[test]
fn a_linked_pdf_is_not_packed() {
    let fixture = fixture();
    let library = fixture.root.join("library");
    std::os::unix::fs::symlink(library.join("papers/aster.pdf"), library.join("papers/linked.pdf")).unwrap();
    std::fs::write(
        library.join("refs.bib"),
        "@book{vale2024links, author={Vale, Tomas}, title={Linked}, year={2024}, file={papers/linked.pdf}}",
    )
    .unwrap();
    fixture
        .engine
        .dispatch(Method::Libraries, &json!({"action":"reload"}), &Cancellation::default())
        .unwrap();
    let (result, _) = share(
        &fixture.engine,
        json!({"items":[{"by":"key","value":"vale2024links"}], "pdfs": true}),
    )
    .unwrap();
    assert_eq!(result.without_pdf, [0]);
    assert!(result.files.iter().all(|file| !file.path.starts_with("PDFs/")));
}
