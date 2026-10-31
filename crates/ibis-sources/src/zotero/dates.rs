//! Zotero date conversions with a fixed en-US/UTC interpretation.
//!
//! Ported from zotero/utilities date.js and utilities_item.js, revision
//! 4051881d59c6c12d34417d4c226f37218051be90.
//! Copyright © Corporation for Digital Scholarship. AGPL-3.0-or-later.
//! See ../../vendor/zotero/COPYING and the upstream notices.

use serde_json::{Value, json};

/// Natural-language fallback does not run on arbitrarily long source text.
pub(super) const MAX_PARSE_BYTES: usize = 4096;

/// Source dates are data: no operating-system locale, timezone or current year
/// participates in their interpretation. Ambiguous two-digit years stay literal.
pub(super) fn to_csl(value: &str, accessed: bool) -> Value {
    if value.len() > MAX_PARSE_BYTES {
        return json!({"literal": value});
    }
    if !accessed {
        if let Some(date) = extra_date(value) {
            return date;
        }
        if looks_like_edtf(value) {
            return json!({"literal": value});
        }
    }
    let access_value;
    let text = if accessed {
        access_value = access_calendar(value).unwrap_or_default();
        &access_value
    } else {
        value
    };
    natural_date(text).unwrap_or_else(|| json!({"literal": text}))
}

/// Quoted literals and valid EDTF follow Zotero. Source year letters and
/// publication status also stay literal, avoiding citeproc's numeric truncation.
/// Other Extra date lines remain in `note` for its field-hack parser.
pub(super) fn extra_date(value: &str) -> Option<Value> {
    if value.len() > MAX_PARSE_BYTES {
        return None;
    }
    let text = value.trim();
    let mut chars = text.chars();
    if matches!(chars.next(), Some('"' | '“')) && matches!(chars.next_back(), Some('"' | '”')) {
        let literal = chars.as_str();
        if !literal.is_empty() && !literal.contains(['\r', '\n', '\u{2028}', '\u{2029}']) {
            return Some(json!({"literal": literal}));
        }
    }
    if ibis_core::citable::literal_date(text) {
        return Some(json!({"literal":text}));
    }
    edtf(text)
}

fn normalize_dashes(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut dash = false;
    for ch in value.chars() {
        let is_dash = matches!(ch, '-' | '–' | '—' | '−');
        if !is_dash || !dash {
            normalized.push(if is_dash { '-' } else { ch });
        }
        dash = is_dash;
    }
    normalized
}

fn without_circa(value: &str) -> (&str, bool) {
    for prefix in [
        "~", "circa ", "ca. ", "ca.", "ca ", "ca", "c. ", "c.", "about ", "around ",
    ] {
        if value
            .get(..prefix.len())
            .is_some_and(|part| part.eq_ignore_ascii_case(prefix))
        {
            return (value[prefix.len()..].trim_start(), true);
        }
    }
    (value, false)
}

/// A year marker's signed multiplier. Punctuation and spacing are cosmetic.
fn era(value: &str) -> Option<i32> {
    let marker: String = value.chars().filter(|ch| !ch.is_whitespace() && *ch != '.').collect();
    match marker.to_ascii_uppercase().as_str() {
        "BC" | "BCE" => Some(-1),
        "AD" | "CE" => Some(1),
        _ => None,
    }
}

fn era_year_parts(value: &str) -> Option<(i32, Option<i32>)> {
    let start = value.find(|ch: char| ch.is_ascii_digit())?;
    let end = start + value[start..].bytes().take_while(u8::is_ascii_digit).count();
    if end - start > 4 {
        return None;
    }
    let year = value[start..end].parse::<i32>().ok()?;
    let before = value[..start].trim();
    let after = value[end..].trim();
    let marker = match (before.is_empty(), after.is_empty()) {
        (true, true) => None,
        (false, true) => {
            let marker = era(before)?;
            if marker < 0 {
                return None;
            }
            Some(marker)
        }
        (true, false) => Some(era(after)?),
        (false, false) => return None,
    };
    Some((year, marker))
}

fn era_edtf(value: &str) -> Option<String> {
    let (left, right) = match value.split_once('-') {
        Some((left, right)) if !left.is_empty() => (left, Some(right)),
        _ => (value, None),
    };
    let (begin, begin_era) = era_year_parts(left)?;
    let end = match right {
        Some(right) => {
            // Zotero accepts an era suffix on the interval's right-hand year,
            // but no prefix there (`27 BC-AD 14` remains a literal).
            if !right.trim_start().as_bytes().first().is_some_and(u8::is_ascii_digit) {
                return None;
            }
            Some(era_year_parts(right)?)
        }
        None => None,
    };
    let end_era = end.and_then(|(_, marker)| marker);
    if begin == 0 || end.is_some_and(|(year, _)| year == 0) || (begin_era.is_none() && end_era.is_none()) {
        return None;
    }
    let first = begin_era.or(end_era)?;
    let mut result = format!("{}{begin:04}", if first < 0 { "-" } else { "" });
    if let Some((year, marker)) = end {
        let sign = marker.or(begin_era)?;
        result.push_str(&format!("/{}{year:04}", if sign < 0 { "-" } else { "" }));
    }
    Some(result)
}

fn days_in_month(year: i32, month: i32) -> i32 {
    match month {
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn edtf_parts(value: &str) -> Option<(Vec<i32>, bool)> {
    let circa = value.ends_with(['~', '?', '%']);
    let value = if circa { &value[..value.len() - 1] } else { value };
    let negative = value.starts_with('-');
    let text = value.strip_prefix('-').unwrap_or(value);
    let mut components = text.split('-');
    let year_text = components.next()?;
    // Zotero accepts unpadded negative years, but positive years require four digits.
    if !(if negative {
        (1..=4).contains(&year_text.len())
    } else {
        year_text.len() == 4
    }) || !year_text.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let year = year_text.parse::<i32>().ok()? * if negative { -1 } else { 1 };
    let mut parts = vec![year];
    if let Some(month_text) = components.next() {
        if month_text.len() != 2 || !month_text.bytes().all(|byte| byte.is_ascii_digit()) {
            return None;
        }
        let month = month_text.parse::<i32>().ok()?;
        if !(1..=12).contains(&month) {
            return None;
        }
        parts.push(month);
        if let Some(day_text) = components.next() {
            if day_text.len() != 2 || !day_text.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let day = day_text.parse::<i32>().ok()?;
            if !(1..=days_in_month(year, month)).contains(&day) {
                return None;
            }
            parts.push(day);
        }
    }
    if components.next().is_some() {
        return None;
    }
    Some((parts, circa))
}

fn edtf(value: &str) -> Option<Value> {
    let normalized = normalize_dashes(value.trim());
    let (text, circa) = without_circa(&normalized);
    let era_value = era_edtf(text);
    let text = era_value.as_deref().unwrap_or(text);
    let mut interval = None;
    if text.len() >= 7 && text.as_bytes().get(4) == Some(&b'-') {
        let (left, right) = (&text[..4], &text[5..]);
        if left.bytes().all(|byte| byte.is_ascii_digit())
            && matches!(right.len(), 2 | 4)
            && right.bytes().all(|byte| byte.is_ascii_digit())
        {
            let begin = left.parse::<i32>().ok()?;
            let end = right.parse::<i32>().ok()?;
            let end = if right.len() == 2 {
                if end > 12 { begin / 100 * 100 + end } else { 0 }
            } else {
                end
            };
            if end > begin {
                interval = Some(format!("{left}/{end:04}"));
            }
        }
    }
    let text = interval.as_deref().unwrap_or(text);
    let mut dates = text.split('/');
    let (begin, first_circa) = edtf_parts(dates.next()?)?;
    let mut parts = vec![begin];
    let mut uncertain = circa || first_circa;
    if let Some(end) = dates.next() {
        let (end, end_circa) = edtf_parts(end)?;
        let comparable = |parts: &[i32]| {
            parts[0] * 10000 + parts.get(1).copied().unwrap_or(0) * 100 + parts.get(2).copied().unwrap_or(0)
        };
        if comparable(&end) <= comparable(&parts[0]) {
            return None;
        }
        parts.push(end);
        uncertain |= end_circa;
    }
    if dates.next().is_some() {
        return None;
    }
    let mut result = json!({"date-parts": parts});
    if uncertain {
        result["circa"] = json!(true);
    }
    Some(result)
}

fn edtf_shape(value: &str) -> bool {
    let mut sides = value.split('/');
    let side = |part: &str| {
        let part = part.strip_prefix('-').unwrap_or(part);
        part.len() >= 4
            && part.as_bytes()[..4].iter().all(u8::is_ascii_digit)
            && part[4..]
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b'~' | b'?' | b'%'))
    };
    sides.next().is_some_and(side) && sides.next().is_none_or(side) && sides.next().is_none()
}

fn looks_like_edtf(value: &str) -> bool {
    let normalized = normalize_dashes(value.trim());
    let (text, _) = without_circa(&normalized);
    if edtf_shape(text) || text.starts_with('-') && text.as_bytes().get(1).is_some_and(u8::is_ascii_digit) {
        return true;
    }
    if text
        .split('/')
        .any(|part| part.starts_with('-') && part.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
    {
        return true;
    }
    // An era marker accompanied only by EDTF punctuation is protected as a
    // literal even if it is malformed or gives a reversed range.
    let mut marker = false;
    let mut token = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_alphabetic() || ch == '.' {
            token.push(ch);
        } else {
            if !token.is_empty() {
                if era(&token).is_some() {
                    marker = true;
                } else if !token.chars().all(|c| matches!(c, 'T' | 't' | 'Z' | 'z' | '.')) {
                    return false;
                }
                token.clear();
            }
            if !ch.is_ascii_digit()
                && !ch.is_whitespace()
                && !matches!(ch, '-' | ':' | '+' | '~' | '?' | '%' | '/' | '.')
            {
                return false;
            }
        }
    }
    marker
}

fn word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Digit runs beginning at JavaScript-style ASCII word boundaries. A year
/// followed immediately by an era marker defers its end-boundary check.
fn numbers(value: &str, require_end_boundary: bool) -> Vec<(usize, usize)> {
    let bytes = value.as_bytes();
    let mut runs = Vec::new();
    let mut pos = 0;
    while pos < bytes.len() {
        if bytes[pos].is_ascii_digit() {
            let start = pos;
            while pos < bytes.len() && bytes[pos].is_ascii_digit() {
                pos += 1;
            }
            if (start == 0 || !word(bytes[start - 1]))
                && (!require_end_boundary || pos == bytes.len() || !word(bytes[pos]))
            {
                runs.push((start, pos));
            }
        } else {
            pos += 1;
        }
    }
    runs
}

fn take_number(value: &str, start: usize, max: usize) -> Option<(usize, usize)> {
    let length = value
        .as_bytes()
        .get(start..)?
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    (length > 0 && length <= max).then_some((start, start + length))
}

struct NumericDate {
    year: Option<String>,
    month: Option<i32>,
    day: Option<i32>,
    parts: Vec<String>,
}

fn numeric_date(value: &str) -> Option<NumericDate> {
    let (start, end) = numbers(value, true).into_iter().find(|(start, end)| end - start <= 4)?;
    let first = &value[start..end];
    let mut cursor = end;
    let mut separators = Vec::new();
    let mut extra = Vec::new();
    for max in [2, 4] {
        let Some(separator) = value[cursor..].chars().next() else {
            break;
        };
        if !matches!(separator, '-' | '/' | '.' | '年' | '月') {
            break;
        }
        let number_start = cursor + separator.len_utf8();
        // The optional middle component is absent in month/year dates.
        let number = take_number(value, number_start, max).or_else(|| {
            (extra.is_empty() && max == 2)
                .then(|| take_number(value, number_start, 4))
                .flatten()
        });
        let Some((part_start, part_end)) = number else { break };
        separators.push(separator);
        extra.push(&value[part_start..part_end]);
        cursor = part_end;
    }
    let full = start == 0 && cursor == value.len();
    if (!full && extra.len() != 2)
        || (separators.len() == 2 && separators[0] != separators[1] && separators.as_slice() != ['年', '月'])
    {
        return None;
    }
    let (year, mut month, mut day) = if first.len() >= 3 || separators.first() == Some(&'年') {
        (Some(first), extra.first().copied(), extra.get(1).copied())
    } else if extra.len() == 1 && extra[0].len() >= 3 {
        (Some(extra[0]), Some(first), None)
    } else {
        (extra.get(1).copied(), Some(first), extra.first().copied())
    };
    let mut month_number = month.take().and_then(|part| part.parse::<i32>().ok());
    let mut day_number = day.take().and_then(|part| part.parse::<i32>().ok());
    if month_number.is_some_and(|month| month > 12) {
        std::mem::swap(&mut month_number, &mut day_number);
    }
    if month_number.is_some_and(|month| month > 12) || day_number.is_some_and(|day| day > 31) {
        return None;
    }
    let year = year.map(str::to_string);
    Some(NumericDate {
        year,
        month: month_number.filter(|month| *month != 0),
        day: day_number.filter(|day| *day != 0),
        parts: vec![value[..start].into(), value[cursor..].into()],
    })
}

fn fallback_year(value: &str) -> Option<(usize, usize, Value)> {
    for (start, end) in numbers(value, false) {
        let digits = &value[start..end];
        if digits.len() > 4 {
            continue;
        }
        // Year-with-era syntax may use fewer than three digits.
        let tail = &value[end..];
        let marker = tail
            .char_indices()
            .take_while(|(pos, _)| *pos <= 16)
            .filter_map(|(pos, ch)| {
                let end = pos + ch.len_utf8();
                let candidate = tail[..end].trim_end();
                let sign = era(candidate)?;
                let end = candidate.len();
                if tail.as_bytes().get(end).is_some_and(|byte| word(*byte)) {
                    return None;
                }
                Some((end, sign))
            })
            .last();
        if let Some((marker_end, sign)) = marker
            && let Ok(year) = digits.parse::<i32>()
        {
            return Some((start, end + marker_end, json!(year * sign)));
        }
        if (3..=4).contains(&digits.len()) && value.as_bytes().get(end).is_none_or(|byte| !word(*byte)) {
            return Some((start, end, json!(digits)));
        }
    }
    None
}

fn find_month(value: &str) -> Option<(usize, usize, i32)> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let mut result = None;
    for (start, ch) in value.char_indices() {
        if !ch.is_ascii_alphabetic() || value[..start].chars().next_back().is_some_and(char::is_alphabetic) {
            continue;
        }
        if let Some(part) = value.get(start..start + 3)
            && let Some(month) = MONTHS.iter().position(|name| part.eq_ignore_ascii_case(name))
        {
            let end = value[start..].find(' ').map_or(value.len(), |pos| start + pos);
            let prefix = value[..start].char_indices().next_back().map_or(start, |(pos, _)| pos);
            result = Some((prefix, end, month as i32 + 1));
        }
    }
    result
}

fn first_day(value: &str) -> Option<(usize, usize, i32)> {
    let bytes = value.as_bytes();
    let mut cursor = 0;
    while cursor < bytes.len() {
        if !bytes[cursor].is_ascii_digit() {
            cursor += 1;
            continue;
        }
        let start = cursor;
        while cursor < bytes.len() && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor - start > 2 || (start > 0 && word(bytes[start - 1])) {
            continue;
        }
        let mut end = cursor;
        if value.get(end..end + 2).is_some_and(|suffix| {
            ["st", "nd", "rd", "th"]
                .iter()
                .any(|expected| suffix.eq_ignore_ascii_case(expected))
        }) {
            end += 2;
        }
        if end == bytes.len() || !word(bytes[end]) {
            let day = value[start..cursor].parse().ok()?;
            return Some((start, end, day));
        }
    }
    None
}

fn natural_date(value: &str) -> Option<Value> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let NumericDate {
        mut year,
        mut month,
        mut day,
        mut parts,
    } = numeric_date(&value).unwrap_or_else(|| NumericDate {
        year: None,
        month: None,
        day: None,
        parts: vec![value.clone()],
    });
    // Zotero resolves 1- and 2-digit years against the wall clock. Retaining the
    // original literal is deterministic and avoids silently choosing a century.
    if year
        .as_ref()
        .is_some_and(|year| year.len() < 3 && year.parse::<i32>() != Ok(0))
    {
        return None;
    }
    let mut year_value = year.take().map(|year| {
        let normalized = year.parse::<i32>().map_or(year.clone(), |year| year.to_string());
        json!(normalized)
    });
    if year_value.is_none() || year_value == Some(json!("0")) {
        for index in 0..parts.len() {
            if let Some((start, end, found)) = fallback_year(&parts[index]) {
                let before = parts[index][..start].to_string();
                let after = parts[index][end..].to_string();
                parts.splice(index..=index, [before, after]);
                year_value = Some(found);
                break;
            }
        }
    }
    if month.is_none() {
        for index in 0..parts.len() {
            if let Some((start, end, found)) = find_month(&parts[index]) {
                let before = parts[index][..start].to_string();
                let after = parts[index][end..].trim_start_matches(' ').to_string();
                parts.splice(index..=index, [before, after]);
                month = Some(found);
                break;
            }
        }
    }
    if day.is_none() {
        for part in &mut parts {
            if let Some((start, end, found)) = first_day(part)
                && found <= 31
            {
                let before = &part[..start];
                let after = &part[end..];
                *part = format!("{before} {after}");
                day = Some(found);
                break;
            }
        }
    }
    let year_value = year_value?;
    let mut date_parts = vec![year_value];
    if let Some(month) = month {
        date_parts.push(json!(month));
        if let Some(day) = day.filter(|day| *day != 0) {
            date_parts.push(json!(day));
        }
    }
    let mut result = json!({"date-parts": [date_parts]});
    if month.is_none() {
        let season = parts.join(" ");
        let season = season.trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
        if !season.is_empty() {
            result["season"] = json!(season);
        }
    }
    Some(result)
}

/// Zotero shifts access timestamps through the computer's timezone. The neutral
/// engine instead fixes that conversion to UTC, keeping both access paths equal.
fn access_calendar(value: &str) -> Option<String> {
    let two_digits = |text: &str| -> Option<i32> {
        (text.len() == 2 && text.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| text.parse().ok())
            .flatten()
    };
    // Reduced-precision ISO dates pass through Date and then Date.UTC. The
    // latter treats years 0..99 as 1900..1999, as Zotero's conversion does.
    if matches!(value.len(), 4 | 7) {
        let (parts, _) = edtf_parts(value)?;
        if parts.len() <= 2 {
            let year = access_utc_year(parts[0]);
            return Some(format!("{year:04}-{:02}-01", parts.get(1).copied().unwrap_or(1)));
        }
        return None;
    }
    let date = value.get(..10)?;
    if date.as_bytes().get(4) != Some(&b'-')
        || date.as_bytes().get(7) != Some(&b'-')
        || !date.as_bytes()[..4].iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    let year = date[..4].parse::<i32>().ok()?;
    let month = two_digits(date.get(5..7)?)?;
    let day = two_digits(date.get(8..10)?)?;
    // Zotero's SQL-date predicate checks the field ranges, not calendar-day
    // validity. A date alone bypasses Date, retaining e.g. February 30.
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if value.len() == 10 {
        return Some(date.into());
    }
    let iso = match value.as_bytes().get(10) {
        Some(b'T') => true,
        Some(b' ') => false,
        _ => return None,
    };
    let time = value.get(11..)?;
    let hour = two_digits(time.get(..2)?)?;
    let minute = two_digits(time.get(3..5)?)?;
    if time.as_bytes().get(2) != Some(&b':') || minute > 59 {
        return None;
    }
    let mut end = 5;
    let mut second = 0;
    if time.as_bytes().get(end) == Some(&b':') {
        second = two_digits(time.get(end + 1..end + 3)?)?;
        if second > 59 {
            return None;
        }
        end += 3;
    }
    let mut fractional_nonzero = false;
    if iso && end == 8 && time.as_bytes().get(end) == Some(&b'.') {
        let fraction_start = end + 1;
        end = fraction_start;
        while time.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
            fractional_nonzero |= time.as_bytes()[end] != b'0';
            end += 1;
        }
        if end == fraction_start {
            return None;
        }
    }
    // SQL timestamps have no zone or fractional component. Checking the entire
    // tail prevents malformed data from becoming a plausible access date.
    if !iso {
        if hour > 23 || end != time.len() {
            return None;
        }
        let (year, month, day) = access_normalize_calendar(access_utc_year(year), month, day);
        return Some(format!("{year:04}-{month:02}-{day:02}"));
    }
    // JavaScript Date accepts 24:00 as midnight on the next day, but no other
    // hour-24 time. Zotero passes ISO values through that constructor.
    if hour > 24 || hour == 24 && (minute != 0 || second != 0 || fractional_nonzero) {
        return None;
    }
    let zone = time.get(end..)?;
    let offset = match zone {
        "" | "Z" => 0,
        _ if zone.len() == 6 && matches!(zone.as_bytes()[0], b'+' | b'-') && zone.as_bytes()[3] == b':' => {
            let hours = two_digits(zone.get(1..3)?)?;
            let minutes = two_digits(zone.get(4..6)?)?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            let sign = if zone.starts_with('+') { 1 } else { -1 };
            sign * (hours * 60 + minutes)
        }
        _ => return None,
    };
    let utc_minutes = hour * 60 + minute - offset;
    let delta = utc_minutes.div_euclid(1440);
    // ISO timestamps first normalize through Date's parser (including UTC
    // offset/day overflow), then Zotero formats SQL and invokes Date.UTC. Apply
    // its 0..99-year rule after that first normalization, not before it.
    let (year, month, day) = access_normalize_calendar(year, month, day + delta);
    if !(0..=9999).contains(&year) {
        return None;
    }
    let (year, month, day) = access_normalize_calendar(access_utc_year(year), month, day);
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn access_utc_year(year: i32) -> i32 {
    if (0..=99).contains(&year) { year + 1900 } else { year }
}

fn access_normalize_calendar(mut year: i32, mut month: i32, mut day: i32) -> (i32, i32, i32) {
    if day < 1 {
        month -= 1;
        if month == 0 {
            year -= 1;
            month = 12;
        }
        day += days_in_month(year, month);
    } else if day > days_in_month(year, month) {
        day -= days_in_month(year, month);
        month += 1;
        if month == 13 {
            year += 1;
            month = 1;
        }
    }
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edtf_dates_intervals_eras_and_qualifiers() {
        for (source, expected) in [
            ("2026", json!({"date-parts": [[2026]]})),
            ("2026-10-25", json!({"date-parts": [[2026,10,25]]})),
            ("~2026", json!({"date-parts": [[2026]], "circa": true})),
            ("ca. 2026-10", json!({"date-parts": [[2026,10]], "circa": true})),
            ("1995–96", json!({"date-parts": [[1995],[1996]]})),
            ("2021-22", json!({"date-parts": [[2021],[2022]]})),
            (
                "2004-06~/2005-07",
                json!({"date-parts": [[2004,6],[2005,7]], "circa": true}),
            ),
            ("429 BCE", json!({"date-parts": [[-429]]})),
            ("AD 429", json!({"date-parts": [[429]]})),
            ("1000-900 BCE", json!({"date-parts": [[-1000],[-900]]})),
            ("100 BCE-50 CE", json!({"date-parts": [[-100],[50]]})),
            ("-429", json!({"date-parts": [[-429]]})),
            ("0000", json!({"date-parts": [[0]]})),
        ] {
            assert_eq!(to_csl(source, false), expected, "{source}");
        }
    }

    #[test]
    fn invalid_edtf_and_explicit_literals_are_preserved() {
        for value in [
            "2026-02-30",
            "2026-15",
            "2026/2025",
            "2026/2026",
            "0000 BCE",
            "-429-99",
            "2026-10~-25",
        ] {
            assert_eq!(to_csl(value, false), json!({"literal": value}), "{value}");
            assert_eq!(extra_date(value), None);
        }
        assert_eq!(to_csl("“Invented date”", false), json!({"literal": "Invented date"}));
        assert_eq!(extra_date("\"2026\""), Some(json!({"literal": "2026"})));
        assert_eq!(extra_date("May 3, 2026"), None);
    }

    #[test]
    fn natural_dates_keep_zotero_string_years_and_seasons() {
        for (source, expected) in [
            ("May 3, 2026", json!({"date-parts": [["2026",5,3]]})),
            ("3 May 2026", json!({"date-parts": [["2026",5,3]]})),
            ("January 3rd, 2026", json!({"date-parts": [["2026",1,3]]})),
            ("22nd May 2026", json!({"date-parts": [["2026",5,22]]})),
            ("Spring 2026", json!({"date-parts": [["2026"]], "season": "Spring"})),
            ("2026/10/25", json!({"date-parts": [["2026",10,25]]})),
            ("25.10.2026", json!({"date-parts": [["2026",10,25]]})),
            ("2026年10月25日", json!({"date-parts": [["2026",10,25]]})),
            ("January 10 200 BCE", json!({"date-parts": [[-200,1,10]]})),
            ("200 BCE January 3", json!({"date-parts": [[-200,1,3]]})),
            ("Invented undated", json!({"literal": "Invented undated"})),
            ("5/3/26", json!({"literal": "5/3/26"})),
        ] {
            assert_eq!(to_csl(source, false), expected, "{source}");
        }
    }

    #[test]
    fn access_dates_use_utc_without_the_machine_timezone() {
        for (source, expected) in [
            ("2026-10-25", json!({"date-parts": [["2026",10,25]]})),
            ("2026-10-25 00:01:00", json!({"date-parts": [["2026",10,25]]})),
            ("2026-10-25T00:01:00Z", json!({"date-parts": [["2026",10,25]]})),
            ("2026-01-01T00:01:00+02:00", json!({"date-parts": [["2025",12,31]]})),
            ("2024-02-28T23:59:00-02:00", json!({"date-parts": [["2024",2,29]]})),
        ] {
            assert_eq!(to_csl(source, true), expected, "{source}");
        }
    }

    #[test]
    fn era_boundaries_match_zotero_and_preserve_invalid_range_prefixes() {
        assert_eq!(to_csl("27 BC–AD 14", false), json!({"literal": "27 BC–AD 14"}));
        assert_eq!(extra_date("27 BC–AD 14"), None);
        assert_eq!(to_csl("March 15 44BC", false), json!({"date-parts": [[-44,3,15]]}));
        assert_eq!(
            to_csl("Spring 200BC", false),
            json!({"date-parts": [[-200]], "season": "Spring"})
        );
    }

    #[test]
    fn quoted_dates_reject_every_javascript_line_terminator() {
        for separator in ['\r', '\n', '\u{2028}', '\u{2029}'] {
            let value = format!("\"Invented{separator}date\"");
            assert_eq!(extra_date(&value), None);
            assert_eq!(to_csl(&value, false), json!({"literal": value}));
        }
    }

    #[test]
    fn access_calendar_matches_zotero_constructor_order() {
        for (source, expected) in [
            ("2026-02-30", json!({"date-parts": [["2026",2,30]]})),
            ("2026-02-30T00:00:00Z", json!({"date-parts": [["2026",3,2]]})),
            ("2026-02-30 12:00:00", json!({"date-parts": [["2026",3,2]]})),
            ("0001", json!({"date-parts": [["1901",1,1]]})),
            ("0001-01-01", json!({"date-parts": [["1",1,1]]})),
            ("0001-01-01T00:00:00Z", json!({"date-parts": [["1901",1,1]]})),
            ("0001-01-01 00:00:00", json!({"date-parts": [["1901",1,1]]})),
            ("0000-02-29T00:00:00Z", json!({"date-parts": [["1900",3,1]]})),
            ("0099-12-31T23:00:00-02:00", json!({"date-parts": [["100",1,1]]})),
        ] {
            assert_eq!(to_csl(source, true), expected, "{source}");
        }
    }

    #[test]
    fn malformed_access_timestamp_tails_follow_zotero_empty_literal() {
        for source in [
            "2026-10-25 nonsense",
            "2026-10-25T99:99:99Z",
            "2026-10-25TbadZ",
            "2026-10-25 99:99:99",
            "2026-10-25 12:34:56Z",
            "2026-10-25T12:34:56+0230",
            "2026-10-25T12:34:56.123badZ",
            "2026-10-25T12:34:56.Z",
            "2026-10-25T12:34:60Z",
            "2026-10-25T24:00:01Z",
        ] {
            assert_eq!(to_csl(source, true), json!({"literal": ""}), "{source}");
        }
        for source in [
            "2026-10-25 12:34",
            "2026-10-25 12:34:56",
            "2026-10-25T12:34",
            "2026-10-25T12:34Z",
            "2026-10-25T12:34:56.123Z",
            "2026-10-25T12:34:56.123+02:00",
        ] {
            assert_eq!(
                to_csl(source, true),
                json!({"date-parts": [["2026",10,25]]}),
                "{source}"
            );
        }
        assert_eq!(
            to_csl("2026-10-25T24:00:00Z", true),
            json!({"date-parts": [["2026",10,26]]})
        );
    }

    #[test]
    fn arbitrary_unicode_and_long_literals_do_not_panic() {
        for value in ["日", "é2026年", "2026☃-14", "“日”", "💡💡💡💡💡2026", "١٤٤٧"] {
            let _ = to_csl(value, false);
            let _ = to_csl(value, true);
        }
        let text = "Invented 日本語 ".repeat(10_000);
        assert_eq!(to_csl(&text, false), json!({"literal": text}));
    }
}
