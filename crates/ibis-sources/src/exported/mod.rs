//! Libraries other programs export without citation keys: RIS and Ibis's CSV
//! template (decision 0041). Each record becomes CSL data whose identity is a
//! versioned hash of that data, so it lasts across refreshes and fresh exports.
use std::collections::{BTreeMap, HashSet};

use ibis_core::contract::{CitationWarningCode, Fingerprint, ItemRef, LibraryId, Record, Snapshot};
use serde_json::{Value, json};

use crate::{ParsedLibrary, SourceWarning, WarningCode};

pub(crate) mod csv;
pub(crate) mod ris;

/// One person as written: `Family, Given` (or `Family, Given, Suffix`) only when
/// that is clear, otherwise the whole name as a literal. CJK names are never split.
pub(crate) fn person(text: &str) -> Option<Value> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let cjk = text.chars().any(|c| {
        matches!(c as u32, 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF)
    });
    let parts: Vec<_> = text.split(',').map(str::trim).collect();
    Some(match parts[..] {
        [family, given] if !cjk && !family.is_empty() && !given.is_empty() => {
            json!({"family": family, "given": given})
        }
        [family, given, suffix] if !cjk && !family.is_empty() && !given.is_empty() && !suffix.is_empty() => {
            json!({"family": family, "given": given, "suffix": suffix})
        }
        _ => json!({"literal": text}),
    })
}

/// A date's parts, never invented: a year, then a month and a day only when given.
pub(crate) fn date_parts(year: &str, month: Option<&str>, day: Option<&str>) -> Option<Value> {
    let number = |text: &str, range: std::ops::RangeInclusive<i64>| {
        (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<i64>().ok())
            .flatten()
            .filter(|value| range.contains(value))
    };
    let year = number(year, 1..=9999).filter(|_| year.len() == 4)?;
    let mut parts = vec![year];
    if let Some(month) = month {
        parts.push(number(month, 1..=12)?);
        if let Some(day) = day {
            parts.push(number(day, 1..=31)?);
        }
    } else if day.is_some() {
        return None;
    }
    Some(json!({"date-parts": [parts]}))
}

/// The DOI as written when it is one; `None` drops it with a warning.
pub(crate) fn doi(text: &str) -> Option<String> {
    match ibis_core::identifier::parse_query(text)? {
        ibis_core::identifier::Identifier::Doi(canonical) => Some(if text.trim().starts_with("10.") {
            text.trim().to_owned()
        } else {
            canonical
        }),
        _ => None,
    }
}

/// The ISBNs or ISSNs a cell gives, as written, and whether it held anything
/// else; text such as `(Print)` beside a valid one is not a loss.
pub(crate) fn serials(kind: Serial, values: &[&str]) -> (Option<String>, bool) {
    // First-seen order, each once; the set keeps a long list linear.
    let mut found: Vec<&str> = vec![];
    let mut seen = HashSet::new();
    let mut lost = false;
    for value in values {
        let mut valid = vec![];
        // A whole part first, so `978 0 00 000000 2` keeps its grouping spaces.
        for part in value.split([',', ';', '(', ')']).map(str::trim) {
            if kind.valid(part) {
                valid.push(part);
            } else {
                valid.extend(part.split_whitespace().filter(|word| kind.valid(word)));
            }
        }
        lost |= valid.is_empty();
        found.extend(valid.into_iter().filter(|serial| seen.insert(*serial)));
    }
    ((!found.is_empty()).then(|| found.join(", ")), lost)
}

#[derive(Clone, Copy)]
pub(crate) enum Serial {
    Isbn,
    Issn,
}

impl Serial {
    pub(crate) fn field(self) -> &'static str {
        match self {
            Self::Isbn => "ISBN",
            Self::Issn => "ISSN",
        }
    }

    fn valid(self, token: &str) -> bool {
        match self {
            Self::Isbn => matches!(
                ibis_core::identifier::parse_query(token),
                Some(ibis_core::identifier::Identifier::Isbn(_))
            ),
            // Seven digits and a check digit (or X), weighted 8 down to 1, mod 11.
            Self::Issn => {
                let compact: Vec<char> = token.chars().filter(|c| *c != '-').collect();
                compact.len() == 8
                    && token.len() <= 9
                    && compact
                        .iter()
                        .enumerate()
                        .try_fold(0, |sum, (index, c)| {
                            let digit = match c {
                                'X' | 'x' if index == 7 => 10,
                                c => c.to_digit(10)?,
                            };
                            Some(sum + digit * (8 - index as u32))
                        })
                        .is_some_and(|sum| sum % 11 == 0)
            }
        }
    }
}

/// Collects records, keeping one of each identity, in the shape the file
/// loader expects.
pub(crate) struct Records<'a> {
    library: &'a LibraryId,
    previous: Option<&'a Snapshot>,
    prefix: &'static str,
    seen: HashSet<String>,
    /// Records the file holds, read or not: the record limit counts these.
    started: usize,
    pub(crate) parsed: ParsedLibrary,
}

impl<'a> Records<'a> {
    pub(crate) fn new(library: &'a LibraryId, previous: Option<&'a Snapshot>, prefix: &'static str) -> Self {
        Self {
            library,
            previous,
            prefix,
            seen: HashSet::new(),
            started: 0,
            parsed: ParsedLibrary::default(),
        }
    }

    /// Counts a record that starts at `line`; past the record limit it warns
    /// once and says to stop reading.
    pub(crate) fn start(&mut self, line: usize) -> bool {
        self.started += 1;
        if self.started > crate::MAX_LIBRARY_RECORDS {
            self.warn(WarningCode::WorkLimit, line);
            self.parsed.cut_short = true;
            return false;
        }
        true
    }

    pub(crate) fn warn(&mut self, code: WarningCode, line: usize) {
        self.parsed.warnings.push(SourceWarning {
            code,
            line: Some(line),
            key: None,
        });
    }

    /// Adds the record that starts at `line` unless one with the same citation
    /// fields is already here. `uncertain` marks one that lost a field on the way.
    pub(crate) fn push(&mut self, mut csl: BTreeMap<String, Value>, line: usize, mut uncertain: bool) {
        if crate::urls::remove_private_urls(&mut csl) {
            self.warn(WarningCode::Field, line);
            uncertain = true;
        }
        // Serialising the sorted map gives the same bytes for the same fields.
        let fields = serde_json::to_vec(&csl).unwrap_or_default();
        let digest = Fingerprint::of_bytes(&[self.prefix.as_bytes(), b":v1\0", &fields].concat());
        let key = format!("{}:{}", self.prefix, &digest.as_str()[..32]);
        if !self.seen.insert(key.clone()) {
            return;
        }
        csl.insert("id".into(), Value::String(key.clone()));
        let record = Record {
            id: ItemRef {
                library: self.library.clone(),
                key,
            },
            added_at: None,
            csl: csl.into(),
            citation_keys: vec![],
            aliases: vec![],
            warnings: if uncertain {
                vec![CitationWarningCode::SourceField]
            } else {
                vec![]
            },
            identifiers: vec![],
        };
        self.parsed.push_record(record, self.previous);
    }
}
