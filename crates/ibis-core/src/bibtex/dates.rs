//! Publication dates for one resolved BibTeX entry: `date`, `year`, `month`
//! and `day` merged across inheritance into the fields the converter reads.

use biblatex::{Chunk, Date, DateValue, Spanned};

use super::resolve::MONTHS;
use crate::model::days_in_month;

/// The publication-date fields handled together by [`merge_dates`].
pub(super) const DATE_PARTS: [&str; 4] = ["date", "year", "month", "day"];

/// Merge publication dates from sources ordered by precedence (own fields
/// first), following Biber: each of `date`, `year`, `month` and `day` is
/// inherited as a field; an entry's own `date` is used as written and never
/// gains inherited precision; an inherited plain `date` is combined with the
/// entry's own year, month or day (own month `mar` with an inherited
/// `2024-02-05` gives 2024-03-05). Ranges and approximate dates are kept
/// whole. With `check` (the entry being read, not a parent it inherits
/// from), a day that is not a day of the month, or has no month or year, is
/// named in `problems` and left out. A parent keeps its day as written, in
/// its own field, so the inheriting entry's year and month decide.
pub(super) fn merge_dates(
    sources: &[Vec<(String, String)>],
    check: bool,
    problems: &mut Vec<String>,
) -> Vec<(String, String)> {
    let field = |source: &Vec<(String, String)>, n: &str| {
        source.iter().find(|(f, _)| f == n).map(|(_, v)| v.trim().to_string())
    };
    let own = &sources[0];
    if let Some(date) = field(own, "date") {
        return vec![("date".to_string(), date)];
    }
    let inherited = |n: &str| sources.iter().find_map(|s| field(s, n));
    if let Some((at, date)) = (1..sources.len()).find_map(|i| Some((i, field(&sources[i], "date")?))) {
        let Some((y, m, d)) = simple_date(&date) else {
            return vec![("date".to_string(), date)];
        };
        // Parts from sources that outrank the parent with the date win over its parts.
        let before = |n: &str| sources[..at].iter().find_map(|s| field(s, n));
        let year = before("year").unwrap_or(y);
        let month = before("month").as_deref().and_then(month_number).or(m);
        let day = before("day").or(d);
        return compose(year, month, day, check, problems);
    }
    let month = inherited("month");
    let (year, day) = (inherited("year"), inherited("day"));
    match (month.as_deref().map(month_number), year) {
        (numeric, Some(year)) if numeric != Some(None) => compose(year, numeric.flatten(), day, check, problems),
        // A month that is not a number or name ("Spring") stays as written.
        (numeric, year) => {
            let month = if numeric == Some(None) {
                month
            } else {
                numeric.flatten()
            };
            let day = match day {
                Some(day) if check => {
                    let missing = if year.is_none() { "year" } else { "month number" };
                    problems.push(format!("`day` `{day}` has no {missing}; it is ignored"));
                    None
                }
                day => day,
            };
            [("year", year), ("month", month), ("day", day)]
                .into_iter()
                .filter_map(|(name, value)| Some((name.to_string(), value?)))
                .collect()
        }
    }
}

/// Fields for a year with optional month and day, exactly as precise as given.
fn compose(
    year: String,
    month: Option<String>,
    day: Option<String>,
    check: bool,
    problems: &mut Vec<String>,
) -> Vec<(String, String)> {
    if !check {
        if let Some(day) = day {
            let mut out = vec![("year".to_string(), year)];
            out.extend(month.map(|m| ("month".to_string(), m)));
            out.push(("day".to_string(), day));
            return out;
        }
        return fields(year, month, None);
    }
    let day = day.and_then(|text| {
        let Some(m) = month.as_deref().and_then(|m| m.parse::<u8>().ok()) else {
            problems.push(format!("`day` `{text}` has no month; it is ignored"));
            return None;
        };
        let last = days_in_month(astronomical_year(&year), m);
        let digits: String = text.chars().filter(|c| !matches!(c, '{' | '}')).collect();
        let day = digits.trim().parse::<u8>().ok().filter(|d| (1..=last).contains(d));
        if day.is_none() {
            problems.push(format!("`day` `{text}` is not a day of that month; it is ignored"));
        }
        day
    });
    fields(year, month, day)
}

fn fields(year: String, month: Option<String>, day: Option<u8>) -> Vec<(String, String)> {
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

/// The year as BibLaTeX reads a `year` field (signs and eras, so `45 BC` is
/// -44), or a leap year when it cannot be read, so that an unreadable year
/// never loses a 29 February.
fn astronomical_year(year: &str) -> i32 {
    let year: String = year.chars().filter(|c| !matches!(c, '{' | '}')).collect();
    let year = year.trim();
    // The parser panics on a sign followed by a space; conversion reads no date from it.
    if year.starts_with(['+', '-']) && year[1..].starts_with(char::is_whitespace) {
        return 2000;
    }
    let chunks = [Spanned::detached(Chunk::Normal(year.to_string()))];
    match Date::parse_three_fields(&chunks, None, None) {
        Ok(Date {
            value: DateValue::At(date),
            ..
        }) => date.year,
        _ => 2000,
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

/// Two-digit month for `3`, `03`, `mar` or `March`; `None` for anything else.
fn month_number(text: &str) -> Option<String> {
    let lower: String = text.chars().filter(|c| !matches!(c, '{' | '}')).collect();
    let lower = lower.trim().to_ascii_lowercase();
    if let Some(i) = MONTHS
        .iter()
        .position(|(abbr, full)| *abbr == lower || full.eq_ignore_ascii_case(&lower))
    {
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
