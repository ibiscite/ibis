//! Synthetic files only: public records and final citations must agree on URL privacy.
use ibis_core::contract::{CitationForm, GoldenCase, LibraryObject, OutputSpec};
use ibis_engine::{Cancellation, Engine};
use serde_json::json;
use std::sync::Arc;

#[test]
fn private_urls_in_file_fields_and_note_overrides_never_reach_formatted_output()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let engine = Engine::new().map_err(|error| format!("{error:?}"))?;
    let mut case: GoldenCase = serde_json::from_str(include_str!("../../../fixtures/cases/02-pandoc.json"))?;
    case.request.output = OutputSpec::Csl { style: "apa".into() };
    case.request.form = CitationForm::BibliographyOnly;
    for url in [
        "smb://invented/share/private.pdf",
        "private.pdf",
        "file:///invented/private.pdf",
        "\\\\invented\\share\\private.pdf",
    ] {
        for placement in ["URL", "note", "inline", "wrapped", "wrapped-inline"] {
            for extension in ["bib", "json"] {
                let path = dir.path().join(format!("synthetic.{extension}"));
                let field = if placement == "URL" { "URL" } else { "note" };
                let value = match placement {
                    "note" => format!("URL: {url}"),
                    "inline" => format!("{{:URL: {url}}}"),
                    "wrapped" => format!("URL:\n{url}"),
                    "wrapped-inline" => format!("{{:URL:\n{url}}}"),
                    _ => url.into(),
                };
                let text = if extension == "json" {
                    serde_json::to_string(
                        &json!([{"id":"invented","type":"book","title":"Invented title",(field):value}]),
                    )?
                } else {
                    // Escaped braces produce literal citeproc override braces;
                    // ordinary BibTeX grouping braces are removed by its parser.
                    let value = if placement.ends_with("inline") {
                        value.replace('{', "\\{").replace('}', "\\}")
                    } else {
                        value
                    };
                    format!("@book{{invented,title={{Invented title}},{field}={{{value}}}}}")
                };
                std::fs::write(&path, text)?;
                let loaded = ibis_sources::load_file(&path)?;
                let LibraryObject::Record(record) = &loaded.snapshot.objects()[0].object else {
                    return Err("expected invented record".into());
                };
                assert!(
                    !serde_json::to_string(record)?.contains("private.pdf"),
                    "{extension} {placement}: {:?}",
                    record.csl
                );
                assert!(!loaded.warnings.is_empty());
                assert!(
                    serde_json::to_string(record.raw.as_ref().ok_or("private provenance missing")?)?
                        .contains("private.pdf")
                );
                case.request.items[0].item = record.id.clone();
                engine
                    .publish_library(loaded.library.clone(), Arc::new(loaded.snapshot))
                    .map_err(|error| format!("{error:?}"))?;
                let prepared = engine
                    .prepare_citation(&case.request, &Cancellation::default())
                    .map_err(|error| format!("{error:?}"))?;
                let result = prepared.result();
                assert!(result.copy_content(1).is_some());
                assert!(result.text.contains("Invented title"));
                for output in [
                    result.text.as_str(),
                    result.html.as_deref().ok_or("missing HTML")?,
                    result.rtf.as_deref().ok_or("missing RTF")?,
                ] {
                    assert!(
                        !output.contains("private.pdf"),
                        "private URL reached formatted {extension} output"
                    );
                }
            }
        }
    }
    Ok(())
}
