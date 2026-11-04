//! `--format` takes exactly the engine's output ids and `ibis outputs` lists
//! them by their one name (#1260); nothing in the command line names an
//! output its own way.
use ibis_engine::{Engine, outputs};
use serde_json::{Value, json};
use std::{io::Cursor, path::PathBuf};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn library() -> Result<PathBuf> {
    static WRITTEN: std::sync::OnceLock<std::io::Result<PathBuf>> = std::sync::OnceLock::new();
    let written = WRITTEN.get_or_init(|| {
        let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("outputs.bib");
        std::fs::write(
            &path,
            "@article{key,title={Synthetic sleep study},author={García, Ana},year={2021},journal={Journal of Tests},volume={4}}\n",
        )
        .map(|()| path)
    });
    Ok(written.as_ref().map_err(|error| error.to_string())?.clone())
}

fn run(args: &[&str]) -> Result<(u8, String, String)> {
    let path = library()?;
    let mut all = vec!["ibis", "--file", path.to_str().ok_or("path")?];
    all.extend_from_slice(args);
    let (mut output, mut errors) = (vec![], vec![]);
    let code = ibis::run_with(
        Engine::new().map_err(|_| "engine")?,
        all.into_iter().map(Into::into).collect(),
        Cursor::new(vec![]),
        &mut output,
        &mut errors,
        ibis::OutputMode::Plain,
    );
    Ok((code, String::from_utf8(output)?, String::from_utf8(errors)?))
}

#[test]
fn format_takes_every_listed_id_and_writes_its_output() -> Result {
    for output in outputs::outputs() {
        let (code, text, errors) = run(&["cite", "key", "--format", &output.id, "--json"])?;
        assert_eq!(code, 0, "{}: {errors}", output.id);
        let envelope: Value = serde_json::from_str(&text)?;
        if let Some(example) = &output.example {
            assert_eq!(&envelope["data"]["text"], example, "{}", output.id);
        }
        // A style named by --format is the same style --style names.
        if let ibis_core::contract::OutputSpec::Csl { style } = &output.output {
            let (_, styled, _) = run(&["cite", "key", "--style", style, "--json"])?;
            assert_eq!(styled, text, "{}", output.id);
        }
    }
    // Without --format, the listed default.
    let (_, default, _) = run(&["cite", "key", "--json"])?;
    let (_, named, _) = run(&["cite", "key", "--format", outputs::DEFAULT, "--json"])?;
    assert_eq!(default, named);
    Ok(())
}

#[test]
fn format_refuses_any_other_name_with_the_listed_ones() -> Result {
    // The old command-line spellings of a syntax, a style's title and a near miss.
    for wrong in ["biblatex", "latex", "APA (7th edition)", "Pandoc", "style:apa", ""] {
        let (code, text, errors) = run(&["cite", "key", "--format", wrong, "--json"])?;
        assert_eq!(code, 2, "{wrong}");
        assert_eq!(
            serde_json::from_str::<Value>(&text)?,
            json!({"ibis":1,"ok":false,"error":{"code":"usage"}})
        );
        assert!(
            errors.contains("pandoc") && errors.contains("textcite") && errors.contains("apa"),
            "{errors}"
        );
    }
    Ok(())
}

#[test]
fn a_bibliography_needs_a_style_by_either_flag() -> Result {
    let (code, apa, errors) = run(&["cite", "key", "--format", "apa", "--bibliography"])?;
    assert_eq!(code, 0, "{errors}");
    let (_, styled, _) = run(&["cite", "key", "--style", "apa", "--bibliography"])?;
    assert_eq!(apa, styled);
    let (code, text, errors) = run(&["cite", "key", "--format", "typst", "--bibliography"])?;
    assert_eq!((code, text.as_str()), (2, ""));
    assert!(errors.contains("--bibliography needs a style"), "{errors}");
    Ok(())
}

#[test]
fn outputs_lists_each_id_with_its_one_name() -> Result {
    let listed = outputs::outputs();
    let (code, text, errors) = run(&["outputs"])?;
    assert_eq!(code, 0, "{errors}");
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), listed.len());
    for (line, output) in lines.iter().zip(listed) {
        let mut fields = line.split('\t');
        assert_eq!(fields.next(), Some(output.id.as_str()));
        assert_eq!(fields.next(), Some(output.name.as_str()));
    }
    assert_eq!(lines[0], "pandoc\tPandoc\t[@key]");
    let (code, json, _) = run(&["outputs", "--json"])?;
    assert_eq!(code, 0);
    let envelope: Value = serde_json::from_str(&json)?;
    assert_eq!(
        envelope["data"],
        serde_json::to_value(outputs::list())?
    );
    Ok(())
}
