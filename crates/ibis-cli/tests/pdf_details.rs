//! What a PDF says about itself, read through the real `ibis` executable as
//! the extractor's child (decision 0060).
#![cfg(test)]

use ibis_engine::pdf::{PdfReader, Refusal};
use std::path::PathBuf;
use std::sync::Arc;

const XMP: &str = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:prism="http://prismstandard.org/namespaces/basic/2.0/" prism:doi="10.5555/Invented.Atlas">
<dc:title><rdf:Alt><rdf:li xml:lang="x-default">An Invented Atlas of Tides</rdf:li></rdf:Alt></dc:title>
<dc:creator><rdf:Seq><rdf:li>Ada Lee</rdf:li></rdf:Seq></dc:creator>
<prism:publicationDate>2026-05</prism:publicationDate>
</rdf:Description></rdf:RDF></x:xmpmeta>"#;

fn folder(name: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("pdf-details-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn a_pdfs_own_details_come_back_checked() {
    let root = folder("read");
    let pdf = root.join("atlas.pdf");
    std::fs::write(
        &pdf,
        ibis::pdf_details::invented_pdf_with_details(
            &["An Invented Atlas of Tides\nAda Lee", "Invented second page."],
            &[("Title", "Microsoft Word - atlas_final.docx"), ("Author", "alee")],
            Some(XMP),
        ),
    )
    .unwrap();
    let reader = PdfReader::new(PathBuf::from(env!("CARGO_BIN_EXE_ibis")));
    let details = reader.details(&pdf, Arc::new(|| false)).unwrap();
    assert_eq!(details.pages, 2);
    assert_eq!(details.title.as_deref(), Some("An Invented Atlas of Tides"));
    assert_eq!(details.authors, ["Ada Lee"]);
    assert_eq!(details.issued.as_deref(), Some("2026-05"));
    assert_eq!(details.doi.as_deref(), Some("10.5555/invented.atlas"));
    assert_eq!(details.page_doi, None);

    std::fs::write(root.join("notes.pdf"), "Invented notes saved as text.").unwrap();
    assert_eq!(reader.details(&root.join("notes.pdf"), Arc::new(|| false)), Err(Refusal::NotPdf));
    assert_eq!(reader.details(&root.join("gone.pdf"), Arc::new(|| false)), Err(Refusal::Missing));
    let _ = std::fs::remove_dir_all(&root);
}
