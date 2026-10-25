//! Publication dates for one resolved BibTeX entry: `date`, `year`, `month`
//! and `day` merged across inheritance into the fields the converter reads.

use super::resolve::MONTHS;

/// The publication-date fields handled together by [`merge_dates`].
pub(super) const DATE_PARTS: [&str; 4] = ["date", "year", "month", "day"];

/// Merge publication dates from sources ordered by precedence (own fields
/// first), following Biber: each of `date`, `year`, `month` and `day` is
/// inherited as a field; an entry's own `date` is used as written and never
/// gains inherited precision; an inherited plain `date` is combined with the
/// entry's own year, month or day (own month `mar` with an inherited
/// `2024-02-05` gives 2024-03-05). Ranges and approximate dates are kept
/// whole. A day that is not a day of the month, or has no month, is named
/// in `problems` and left out.
pub(super) fn merge_dates(sources: &[Vec<(String, String)>], problems: &mut Vec<String>) -> Vec<(String, String)> {
    let field = |source: &Vec<(String, String)>, n: &str| {
        source.iter().find(|(f, _)| f == n).map(|(_, v)| v.trim().to_string())
    };
    let own = &sources[0];
    if let Some(date) = field(own, "date") {
        return vec![("date".to_string(), date)];
    }
    let inherited = |n: &str| sources.iter().find_map(|s| field(s, n));
    if let Some(date) = sources[1..].iter().find_map(|s| field(s, "date")) {
        let Some((y, m, d)) = simple_date(&date) else {
            return vec![("date".to_string(), date)];
        };
        let year = field(own, "year").unwrap_or(y);
        let month = field(own, "month").as_deref().and_then(month_number).or(m);
        let day = field(own, "day").or(d);
        return compose(year, month, day, problems);
    }
    let month = inherited("month");
    match (month.as_deref().map(month_number), inherited("year")) {
        // A month that is not a number or abbreviation ("Spring") stays as written.
        (Some(None), year) => {
            if let Some(day) = inherited("day") {
                problems.push(format!("`day` `{day}` has no month number; it is ignored"));
            }
            year.map(|y| ("year".to_string(), y))
                .into_iter()
                .chain(month.map(|m| ("month".to_string(), m)))
                .collect()
        }
        (numeric, Some(year)) => compose(year, numeric.flatten(), inherited("day"), problems),
        (numeric, None) => numeric
            .flatten()
            .map(|m| ("month".to_string(), m))
            .into_iter()
            .collect(),
    }
}

/// Fields for a year with optional month and day, exactly as precise as given.
fn compose(
    year: String,
    month: Option<String>,
    day: Option<String>,
    problems: &mut Vec<String>,
) -> Vec<(String, String)> {
    let day = day.and_then(|text| {
        let Some(day) = text.parse::<u8>().ok().filter(|d| (1..=31).contains(d)) else {
            problems.push(format!("`day` `{text}` is not a day of the month; it is ignored"));
            return None;
        };
        if month.is_none() {
            problems.push(format!("`day` `{text}` has no month; it is ignored"));
            return None;
        }
        Some(day)
    });
    match month {
        Some(m) if is_digits(&year, 4) => {
            let date = match day {
                Some(d) => format!("{year}-{m}-{d:02}"),
                None => format!("{year}-{m}"),
            };
            vec![("date".to_string(), date)]
        }
        month => {
            let mut out = vec![("year".to_string(), year)];
            out.extend(month.map(|m| ("month".to_string(), m)));
            out.extend(day.map(|d| ("day".to_string(), d.to_string())));
            out
        }
    }
}

/// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`, split into components (month and day
/// zero-padded to two digits).
fn simple_date(text: &str) -> Option<(String, Option<String>, Option<String>)> {
    let mut parts = text.split('-');
    let year = parts.next().filter(|y| is_digits(y, 4))?.to_string();
    let month = parts.next().map(|m| {
        m.parse::<u8>()
            .ok()
            .filter(|m| (1..=12).contains(m))
            .map(|m| format!("{m:02}"))
    });
    let day = parts.next().map(|d| {
        d.parse::<u8>()
            .ok()
            .filter(|d| (1..=31).contains(d))
            .map(|d| format!("{d:02}"))
    });
    if parts.next().is_some() || month == Some(None) || day == Some(None) {
        return None;
    }
    Some((year, month.flatten(), day.flatten()))
}

/// Two-digit month for `3`, `03` or `mar`; `None` for anything else.
fn month_number(text: &str) -> Option<String> {
    let lower = text.trim().to_ascii_lowercase();
    if let Some(i) = MONTHS.iter().position(|(abbr, _)| *abbr == lower) {
        return Some(format!("{:02}", i + 1));
    }
    lower
        .parse::<u8>()
        .ok()
        .filter(|m| (1..=12).contains(m))
        .map(|m| format!("{m:02}"))
}

fn is_digits(text: &str, len: usize) -> bool {
    text.len() == len && text.bytes().all(|b| b.is_ascii_digit())
}
