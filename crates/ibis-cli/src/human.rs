use ibis_core::model::{Name, strip_markup};
use ibis_engine::Method;
use serde_json::Value;

pub(super) fn summary(method: Method, data: &Value) -> Option<String> {
    match method {
        Method::Search => {
            let hits = data.get("hits")?.as_array()?;
            if hits.is_empty() {
                return Some("No references found.".into());
            }
            Some(hits.iter().map(search_row).collect::<Option<Vec<_>>>()?.join("\n"))
        }
        Method::Libraries => {
            let libraries = data.get("libraries")?.as_array()?;
            if libraries.is_empty() {
                return Some("No libraries loaded. Open one with --file <library>.".into());
            }
            let mut lines = vec![format!(
                "{} {}",
                libraries.len(),
                if libraries.len() == 1 { "library" } else { "libraries" }
            )];
            for library in libraries {
                let count = library.get("objects")?.as_u64()?;
                lines.push(format!(
                    "{}\t{} {}{}",
                    field(library.get("id")?.as_str()?),
                    count,
                    if count == 1 { "item" } else { "items" },
                    if library.get("refresh_error").is_some_and(|value| !value.is_null()) {
                        " (last good snapshot; refresh failed)"
                    } else {
                        ""
                    }
                ));
            }
            Some(lines.join("\n"))
        }
        _ => None,
    }
}

fn search_row(hit: &Value) -> Option<String> {
    let record = hit.pointer("/item/object/data")?;
    let csl = record.get("csl")?;
    let keys = record.get("citation_keys")?.as_array()?;
    let key = match keys.first().and_then(|key| key.get("value")).and_then(Value::as_str) {
        None => "[no citation key]",
        Some(key) if key.is_empty() || key.chars().any(|c| c.is_whitespace() || c.is_control()) => {
            "[invalid citation key]"
        }
        Some(key)
            if keys
                .iter()
                .any(|other| other.get("value").and_then(Value::as_str) != Some(key)) =>
        {
            "[conflicting citation keys]"
        }
        Some(key) => key,
    };
    let authors = csl
        .get("author")
        .and_then(Value::as_array)
        .map_or_else(String::new, |names| {
            let mut authors = names
                .iter()
                .take(3)
                .map(author)
                .filter(|name| !name.is_empty())
                .collect::<Vec<_>>();
            if names.len() > 3 {
                authors.push("et al.".into());
            }
            authors.join(", ")
        });
    let issued = csl.get("issued");
    let year = issued
        .and_then(|date| date.pointer("/date-parts/0/0"))
        .map(|year| year.as_str().map_or_else(|| year.to_string(), str::to_owned))
        .or_else(|| {
            issued
                .and_then(|date| date.get("literal"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "n.d.".into());
    Some(format!(
        "{}\t{}\t{}\t{}",
        field(key),
        if authors.is_empty() {
            "—".into()
        } else {
            field(&authors)
        },
        field(&year),
        field(&strip_markup(
            csl.get("title").and_then(Value::as_str).unwrap_or("[untitled]")
        ))
    ))
}

fn author(value: &Value) -> String {
    let text = |name| value.get(name).and_then(Value::as_str).map(str::to_owned);
    Name {
        family: text("family"),
        given: text("given"),
        literal: text("literal"),
        dropping_particle: text("dropping-particle"),
        non_dropping_particle: text("non-dropping-particle"),
        suffix: text("suffix"),
    }
    .display()
}

fn field(text: &str) -> String {
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    super::terminal_display(&line)
}
