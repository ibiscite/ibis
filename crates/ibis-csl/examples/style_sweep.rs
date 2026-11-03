//! Renders the fixture items in every bundled style: one line of JSON for each
//! style, sorted by id, on stdout. Used by the weekly styles workflow, whose
//! `scripts/style-sweep.mjs` compares two runs. A dependent style runs its
//! parent's rules, so styles that share rules and language render once.
#![allow(clippy::expect_used)]
use ibis_core::contract::*;
use ibis_csl::{Formatter, styles};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

fn library() -> Snapshot {
    let items: Vec<BTreeMap<String, Value>> =
        serde_json::from_str(include_str!("../tests/sweep-items.json")).expect("the sweep items are valid");
    let objects = items
        .into_iter()
        .enumerate()
        .map(|(index, csl)| {
            LibraryObject::Record(Record {
                added_at: None,
                id: reference(index),
                csl,
                citation_keys: vec![],
                aliases: vec![],
                warnings: vec![],
                identifiers: vec![],
            })
        })
        .collect();
    Snapshot::new(objects).expect("the sweep library is valid")
}

fn reference(index: usize) -> ItemRef {
    ItemRef {
        library: LibraryId::file(b"style-sweep"),
        key: format!("item-{index}"),
    }
}

fn request(style: &str, items: &[usize], form: CitationForm) -> CitationRequest {
    let items = items
        .iter()
        .map(|index| json!({"item":reference(*index)}))
        .collect::<Vec<_>>();
    serde_json::from_value(json!({"items":items,"form":form,"output":{"kind":"csl","style":style}}))
        .expect("the sweep request is valid")
}

fn line(result: Result<CitationResult, ibis_csl::FormatError>) -> Value {
    match result {
        Ok(result) if result.copy_content(result.outcomes.len()).is_some() => json!(result.text),
        Ok(_) => json!({"refused":true}),
        Err(error) => json!({"failed":error.to_string()}),
    }
}

fn render(formatter: &Formatter, snapshot: &Snapshot, style: &str) -> Value {
    let count = snapshot.objects().len();
    let all: Vec<usize> = (0..count).collect();
    json!({
        "citations": all.iter().map(|index| {
            line(formatter.format(snapshot, &request(style, &[*index], CitationForm::Normal)))
        }).collect::<Vec<_>>(),
        "bibliography": line(formatter.format(snapshot, &request(style, &all, CitationForm::BibliographyOnly))),
    })
}

fn main() {
    let page = styles("", usize::MAX, 0).expect("the bundle is readable");
    let mut groups = BTreeMap::<(String, Option<String>), Vec<String>>::new();
    for style in page.styles {
        let rules = style.parent.clone().unwrap_or_else(|| style.id.clone());
        let locale = style.parent.as_ref().and(style.default_locale.clone());
        groups.entry((rules, locale)).or_default().push(style.id);
    }
    let groups = Arc::new(Mutex::new(groups.into_values().collect::<Vec<_>>()));
    let snapshot = Arc::new(library());
    let threads = std::thread::available_parallelism().map_or(2, usize::from);
    let rendered = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                let (groups, snapshot) = (groups.clone(), snapshot.clone());
                scope.spawn(move || {
                    let formatter = Formatter::new().expect("the formatter starts");
                    let mut lines = Vec::new();
                    loop {
                        let next = groups.lock().expect("no worker panicked").pop();
                        let Some(ids) = next else { return lines };
                        let output = render(&formatter, &snapshot, &ids[0]);
                        lines.extend(ids.into_iter().map(|id| (id, output.clone())));
                    }
                })
            })
            .collect();
        let mut lines = BTreeMap::new();
        for worker in workers {
            lines.extend(worker.join().expect("no worker panicked"));
        }
        lines
    });
    for (style, output) in rendered {
        println!("{}", json!({"style":style,"output":output}));
    }
}
