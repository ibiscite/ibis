//! Human record rendering accepts presentation only, never lookup identity or raw JSON.
use super::{Out, catalogue, field, out};
use ibis_core::summary::RecordSummary;

pub(super) enum Key {
    Present(String),
    Missing,
    Invalid,
    Conflicting,
}
impl Key {
    pub(super) fn label(&self) -> &str {
        match self {
            Self::Present(key) => key,
            Self::Missing => "[no citation key]",
            Self::Invalid => "[invalid citation key]",
            Self::Conflicting => "[conflicting citation keys]",
        }
    }
}
pub(super) struct Row {
    pub summary: RecordSummary,
    pub key: Key,
    pub partial: bool,
    pub typo: bool,
    pub details: Vec<(&'static str, Option<String>)>,
}
pub(super) fn search_row(hit: &Row) -> Option<String> {
    let summary = &hit.summary;
    // The engine decides a word matched only with a spelling change; the row says so.
    let spelling = (hit.typo && !hit.partial)
        .then(|| catalogue::text("cli-spelling-match", None, &[]))
        .flatten()
        .map(|label| format!(" {label}"))
        .unwrap_or_default();
    Some(format!(
        "{}\t{}\t{}\t{}{spelling}",
        field(hit.key.label()),
        if summary.authors.is_empty() {
            "—".into()
        } else {
            field(&authors(summary, Some(3)))
        },
        field(summary.year.as_deref().unwrap_or("n.d.")),
        field(summary.title.as_deref().unwrap_or("[untitled]"))
    ))
}

pub(super) fn terminal_search(hits: &[Row], width: usize, out: Out) -> Option<String> {
    let rows = hits
        .iter()
        .map(|hit| {
            let summary = &hit.summary;
            let key = field(hit.key.label());
            let author = match summary.authors.first() {
                None => "—".into(),
                Some(first)
                    if summary.authors.len() > 2
                        || summary.authors.last().is_some_and(|author| author.name == "et al.") =>
                {
                    format!("{} et al.", first.name)
                }
                Some(_) => authors(summary, None),
            };
            let year = field(summary.year.as_deref().unwrap_or("n.d."));
            let author = out::truncate(&field(&author), (width / 4).saturating_sub(out::width(&year) + 1));
            let byline = if author.is_empty() {
                year
            } else {
                format!("{author} {year}")
            };
            let mut title = field(summary.title.as_deref().unwrap_or("[untitled]"));
            if hit.typo
                && !hit.partial
                && let Some(label) = catalogue::text("cli-spelling-match", None, &[])
            {
                title = format!("{label} {title}");
            }
            Some((key, byline, title))
        })
        .collect::<Option<Vec<_>>>()?;
    // Bound the two secondary columns so ordinary keys never squeeze out the
    // title. Outlier keys keep their own line rather than widening every row.
    let key_limit = width / 3;
    let key_columns = rows
        .iter()
        .map(|(key, _, _)| out::width(key))
        .filter(|w| *w <= key_limit)
        .max()
        .unwrap_or(0);
    let byline_columns = rows.iter().map(|(_, byline, _)| out::width(byline)).max().unwrap_or(0);
    let mut lines = Vec::new();
    for (key, byline, title) in rows {
        let separate = out::width(&key) > key_limit;
        if separate {
            lines.push(out.bold(&key));
        }
        let prefix = if separate {
            2.min(width)
        } else {
            key_columns.saturating_add(2).min(width)
        };
        let secondary = out.dim(&format!(
            "{byline}{}",
            " ".repeat(byline_columns.saturating_sub(out::width(&byline)))
        ));
        let title_start = prefix + byline_columns + 2;
        let title = out::truncate(&title, width.saturating_sub(title_start));
        let lead = if separate {
            " ".repeat(prefix)
        } else {
            format!(
                "{}{}",
                out.bold(&key),
                " ".repeat(key_columns.saturating_sub(out::width(&key)) + 2)
            )
        };
        // When even the year cannot fit, keep it and the key intact on their
        // own lines. Only author lists and titles may be shortened.
        if title_start > width {
            if !separate {
                lines.push(out.bold(&key));
            }
            lines.push(out.dim(&byline));
        } else {
            lines.push(format!("{lead}{secondary}  {title}"));
        }
    }
    Some(lines.join("\n"))
}

/// The authors' reading names, the first `limit` then "et al.".
fn authors(summary: &RecordSummary, limit: Option<usize>) -> String {
    let limit = limit.unwrap_or(summary.authors.len());
    let mut names: Vec<_> = summary
        .authors
        .iter()
        .take(limit)
        .map(|author| author.name.as_str())
        .collect();
    if summary.authors.len() > limit && names.last() != Some(&"et al.") {
        names.push("et al.");
    }
    names.join(", ")
}

/// A record for reading, projected before it reaches this renderer.
pub(super) fn reference(row: &Row) -> String {
    let summary = &row.summary;
    let authors = Some(authors(summary, None)).filter(|authors| !authors.is_empty());
    let mut lines = vec![field(row.key.label())];
    let common = [
        ("Title", summary.title.clone()),
        ("Authors", authors),
        ("Year", summary.year.clone()),
    ];
    lines.extend(
        common
            .into_iter()
            .chain(row.details.clone())
            .filter_map(|(label, value)| Some(format!("{label}: {}", field(&value?)))),
    );
    lines.join("\n")
}
