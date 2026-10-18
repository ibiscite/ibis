//! Resolves `@string` macros and `crossref`/`xdata` inheritance, producing
//! one flat entry per reference for the field parser.
//!
//! Inheritance is computed here, field by field, rather than by the field
//! parser: an entry's own fields always win; `xdata` parents then fill
//! missing fields under the same names; a `crossref` parent fills the rest,
//! following BibLaTeX's default mapping (for example a proceedings `title`
//! becomes an inproceedings `booktitle`). Results are memoized per entry, so
//! shared ancestors cost nothing extra, recursion is bounded by
//! [`MAX_DEPTH`], cycles are reported and broken, and the parser never sees
//! a link it could follow. Indexed field positions and merged names avoid
//! quadratic name searches while preserving field order and precedence.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::lexer::{Part, RawEntry};

/// Fields that hold dates; digits in other scripts are converted to ASCII
/// before parsing so every script is read the same way.
const DATE_FIELDS: [&str; 9] = [
    "date",
    "year",
    "month",
    "day",
    "endyear",
    "urldate",
    "origdate",
    "eventdate",
    "pubstate-date",
];

/// Fields never inherited (BibLaTeX's non-inheritable fields plus links).
const NOT_INHERITED: [&str; 18] = [
    "ids",
    "crossref",
    "xref",
    "xdata",
    "entryset",
    "entrysubtype",
    "execute",
    "label",
    "options",
    "presort",
    "related",
    "relatedoptions",
    "relatedstring",
    "relatedtype",
    "shorthand",
    "shorthandintro",
    "sortkey",
    "sorttitle",
];

/// Longest inheritance chain (and macro nesting) followed.
const MAX_DEPTH: usize = 16;

/// Most entry evaluations performed while resolving one entry.
const MAX_EVALUATIONS: usize = 10_000;

/// Deepest nesting of abbreviations defined in terms of other abbreviations.
const MAX_MACRO_DEPTH: usize = 64;

/// Most abbreviation expansions performed for one field value.
const MAX_EXPANSION_STEPS: usize = 100_000;

/// Largest field value accepted after expanding abbreviations.
const MAX_VALUE_BYTES: usize = 64 * 1024;

/// Most text retained by the per-load abbreviation cache. Include a fixed
/// charge per entry so empty expansions cannot grow the cache without bound.
const MAX_MACRO_CACHE_BYTES: usize = 4 * 1024 * 1024;

/// Most nested groups in an expanded field; bounds upstream parser recursion
/// on the desktop's 2 MiB loading and watcher threads.
const MAX_FIELD_NESTING: usize = 256;

/// Private-use characters standing in for escaped braces (`\{`, `\}`) so they
/// can never be mistaken for grouping braces; mapped back after conversion.
pub(crate) const LEFT_BRACE: char = '\u{E000}';
pub(crate) const RIGHT_BRACE: char = '\u{E001}';
/// A literal backslash must survive the parser's command decoding without
/// being interpreted as a new command by our subsequent TeX conversion.
pub(crate) const BACKSLASH: char = '\u{E003}';
/// Marks a placeholder character that occurred in the source itself, so real
/// text using these private-use characters is never altered.
pub(crate) const ESCAPE: char = '\u{E002}';

/// Encode escaped braces and escape placeholder characters already present.
/// Decoded by `convert::clean`.
fn encode_literals(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            LEFT_BRACE | RIGHT_BRACE | BACKSLASH | ESCAPE => {
                out.push(ESCAPE);
                out.push(c);
            }
            // An escaped backslash: keep both, so a following brace is a group.
            '\\' if chars.peek() == Some(&'\\') => {
                out.push('\\');
                out.extend(chars.next());
            }
            '\\' if matches!(chars.peek(), Some('{' | '}')) => {
                out.push(if chars.next() == Some('{') {
                    LEFT_BRACE
                } else {
                    RIGHT_BRACE
                });
            }
            _ => out.push(c),
        }
    }
    out
}

/// `biblatex` decodes this symbol before our TeX conversion; protect it until
/// final cleaning. Apply only to parser input, retaining resolved source text
/// for the unchanged fallback used when a field has an unknown command.
fn protect_backslashes(text: &str) -> String {
    const BACKSLASH_COMMAND: &str = "textbackslash";
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ESCAPE => {
                out.push(c);
                out.extend(chars.next());
            }
            '\\' if chars.peek() == Some(&'\\') => {
                out.push(c);
                out.extend(chars.next());
            }
            '\\' if chars
                .clone()
                .take(BACKSLASH_COMMAND.len())
                .eq(BACKSLASH_COMMAND.chars())
                && chars
                    .clone()
                    .nth(BACKSLASH_COMMAND.len())
                    .is_none_or(|c| !c.is_ascii_alphabetic()) =>
            {
                out.push(BACKSLASH);
                for _ in 0..BACKSLASH_COMMAND.len() {
                    chars.next();
                }
                if chars.clone().take(2).eq("{}".chars()) {
                    chars.next();
                    chars.next();
                } else {
                    // Whitespace terminates the control word, as in TeX.
                    while chars.peek().is_some_and(|c| c.is_whitespace()) {
                        chars.next();
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

const MONTHS: [(&str, &str); 12] = [
    ("jan", "January"),
    ("feb", "February"),
    ("mar", "March"),
    ("apr", "April"),
    ("may", "May"),
    ("jun", "June"),
    ("jul", "July"),
    ("aug", "August"),
    ("sep", "September"),
    ("oct", "October"),
    ("nov", "November"),
    ("dec", "December"),
];

/// Ordered field list of one entry after inheritance: (name, value).
type Fields = Rc<Vec<(String, String)>>;

/// Everything needed to resolve entries from one file.
pub(crate) struct Resolver<'a> {
    macros: HashMap<String, Vec<Part>>,
    macro_cache: RefCell<HashMap<(String, bool), Rc<CachedExpansion>>>,
    macro_cache_bytes: Cell<usize>,
    entries: &'a [RawEntry],
    index: HashMap<String, usize>,
    index_folded: HashMap<String, usize>,
}

/// An entry ready for parsing, plus problems found while resolving it.
pub(crate) struct Resolved {
    /// One flat entry: own and inherited fields, no links.
    pub source: String,
    /// Field values after expansion and inheritance, for checks that need raw text.
    pub fields: HashMap<String, String>,
    pub problems: Vec<String>,
}

/// State of resolving one entry.
struct Walk {
    path: Vec<usize>,
    memo: HashMap<(usize, usize), Fields>,
    evaluations: usize,
    problems: Vec<String>,
}

impl<'a> Resolver<'a> {
    pub(crate) fn new(macros: Vec<(String, Vec<Part>)>, entries: &'a [RawEntry]) -> Self {
        let mut index = HashMap::new();
        let mut index_folded = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            index.entry(entry.key.clone()).or_insert(i);
            index_folded.entry(entry.key.to_lowercase()).or_insert(i);
        }
        // Later definitions of the same macro replace earlier ones, as in BibTeX.
        Resolver {
            macros: macros.into_iter().collect(),
            macro_cache: RefCell::new(HashMap::new()),
            macro_cache_bytes: Cell::new(0),
            entries,
            index,
            index_folded,
        }
    }

    fn find(&self, key: &str) -> Option<usize> {
        self.index
            .get(key)
            .or_else(|| self.index_folded.get(&key.to_lowercase()))
            .copied()
    }

    /// Resolve entry `i` into one flat entry with inherited fields applied.
    pub(crate) fn resolve(&self, i: usize) -> Resolved {
        let mut walk = Walk {
            path: vec![i],
            memo: HashMap::new(),
            evaluations: 0,
            problems: Vec::new(),
        };
        let fields = self.effective(i, MAX_DEPTH, &mut walk);
        let mut problems = walk.problems;
        let fields: Vec<_> = fields
            .iter()
            .filter(|(name, value)| {
                if nesting_exceeds_limit(value) {
                    problems.push(format!(
                        "field `{name}` is nested more than {MAX_FIELD_NESTING} levels deep; it is omitted"
                    ));
                    false
                } else {
                    true
                }
            })
            .collect();
        let entry = &self.entries[i];
        let rendered: Vec<String> = fields.iter().map(|(name, value)| render_field(name, value)).collect();
        let source = format!("@{}{{{},\n{}}}\n", entry.kind, entry.key, rendered.join(",\n"));
        // The same problem can be found many times; report each one once.
        let mut unique = HashSet::new();
        problems.retain(|p| unique.insert(p.clone()));
        Resolved {
            source,
            fields: fields.into_iter().cloned().collect(),
            problems,
        }
    }

    /// Fields of entry `idx` after inheritance, following at most `remaining`
    /// further levels. Memoized by (entry, remaining depth).
    fn effective(&self, idx: usize, remaining: usize, walk: &mut Walk) -> Fields {
        if let Some(fields) = walk.memo.get(&(idx, remaining)) {
            return Rc::clone(fields);
        }
        walk.evaluations += 1;
        let entry = &self.entries[idx];
        let root = walk.path.first().copied() == Some(idx);
        // Problems in the entry itself, or in an `@xdata` helper it uses
        // (helpers are not loaded on their own), are reported.
        let report = root || entry.kind == "xdata";
        let mut own: Vec<(String, String)> = Vec::new();
        let mut own_positions: HashMap<&str, usize> = HashMap::new();
        let mut xdata_keys = Vec::new();
        let mut crossref_key = None;
        for field in &entry.fields {
            let mut field_problems = Vec::new();
            let value = self.expand(&field.name, &field.parts, &mut field_problems, report);
            walk.problems.extend(field_problems.into_iter().map(|p| {
                if root {
                    p
                } else {
                    format!("in inherited `{}`: {p}", entry.key)
                }
            }));
            match field.name.as_str() {
                "xdata" => xdata_keys.extend(value.split(',').map(|k| k.trim().to_string()).filter(|k| !k.is_empty())),
                "crossref" => crossref_key = Some(value.trim().to_string()).filter(|k| !k.is_empty()),
                _ => {
                    // As in Biber and Pandoc, a repeated field keeps its last value.
                    if let Some(&position) = own_positions.get(field.name.as_str()) {
                        let existing = &mut own[position];
                        if root {
                            walk.problems
                                .push(format!("field `{}` appears twice; the last value is used", field.name));
                        }
                        existing.1 = value;
                    } else {
                        own_positions.insert(field.name.as_str(), own.len());
                        own.push((field.name.clone(), value));
                    }
                }
            }
        }

        // A parent listed more than once counts at its last position, where
        // it has the highest precedence (as in Biber).
        let mut last_seen = HashSet::new();
        let mut xdata_keys: Vec<String> = xdata_keys
            .into_iter()
            .rev()
            .filter(|k| last_seen.insert(k.to_lowercase()))
            .collect();
        xdata_keys.reverse();
        let mut parents: Vec<(Fields, Option<&str>)> = Vec::new();
        let links = xdata_keys
            .iter()
            .map(|k| (k.as_str(), false))
            .chain(crossref_key.iter().map(|k| (k.as_str(), true)));
        let mut seen_targets = HashSet::new();
        for (target, is_crossref) in links {
            let Some(t) = self.find(target) else {
                walk.problems.push(format!(
                    "`{}` refers to `{target}`, which is not in this file; nothing is inherited from it",
                    entry.key
                ));
                continue;
            };
            if !seen_targets.insert(t) {
                continue;
            }
            if walk.path.contains(&t) {
                walk.problems.push(format!(
                    "cross-reference cycle through `{}` and `{}`; the link back is ignored",
                    entry.key, self.entries[t].key
                ));
                continue;
            }
            if remaining == 0 {
                walk.problems.push(format!(
                    "inheritance is deeper than {MAX_DEPTH} levels; further levels are ignored"
                ));
                continue;
            }
            if walk.evaluations >= MAX_EVALUATIONS {
                walk.problems.push(format!(
                    "inheritance requires more than {MAX_EVALUATIONS} steps; remaining inherited entries are ignored"
                ));
                break;
            }
            walk.path.push(t);
            let inherited = self.effective(t, remaining - 1, walk);
            walk.path.pop();
            let parent_kind = is_crossref.then(|| self.entries[t].kind.as_str());
            parents.push((inherited, parent_kind));
        }

        // Precedence, highest first: the entry's own fields, then its `xdata`
        // parents with later ones overriding earlier ones (as in Biber), then
        // its `crossref` parent. Each source only fills fields still missing.
        let mut sources: Vec<Vec<(String, String)>> = vec![own];
        let (crossref, xdata): (Vec<_>, Vec<_>) = parents.into_iter().partition(|(_, kind)| kind.is_some());
        for (inherited, crossref_kind) in xdata.into_iter().rev().chain(crossref) {
            let mapped = inherited
                .iter()
                .filter(|(name, _)| !NOT_INHERITED.contains(&name.as_str()))
                .flat_map(|(name, value)| {
                    let targets = match crossref_kind {
                        Some(parent_kind) => crossref_targets(parent_kind, &entry.kind, name),
                        None => vec![name.as_str()],
                    };
                    targets.into_iter().map(move |t| (t.to_string(), value.clone()))
                })
                .collect();
            sources.push(mapped);
        }
        let mut merged: Vec<(String, String)> = Vec::new();
        let mut merged_names = HashSet::new();
        for source in &sources {
            for (name, value) in source {
                if !DATE_PARTS.contains(&name.as_str()) && merged_names.insert(name.as_str()) {
                    merged.push((name.clone(), value.clone()));
                }
            }
        }
        merged.extend(merge_dates(&sources));
        let fields: Fields = Rc::new(merged);
        walk.memo.insert((idx, remaining), Rc::clone(&fields));
        fields
    }

    /// Concatenate parts, expanding macros (and macros used inside macros).
    /// Values larger than [`MAX_VALUE_BYTES`], or needing more than
    /// [`MAX_EXPANSION_STEPS`] expansions, are left empty and reported.
    fn expand(&self, field: &str, parts: &[Part], problems: &mut Vec<String>, report: bool) -> String {
        let mut x = Expansion::default();
        for part in parts {
            match part {
                Part::Text(t) | Part::Number(t) => x.out.push_str(t),
                Part::Macro(name) => self.expand_macro(field, name, problems, report, &mut x),
            }
            x.check_size();
            if x.stopped() {
                break;
            }
        }
        if x.stopped() {
            if report {
                problems.push(if x.too_large {
                    format!(
                        "field `{field}` is too large after expanding abbreviations (over {} KiB); it is left empty",
                        MAX_VALUE_BYTES / 1024
                    )
                } else {
                    format!(
                        "field `{field}` uses abbreviations that are too complex to expand (over {MAX_EXPANSION_STEPS} steps); it is left empty"
                    )
                });
            }
            return String::new();
        }
        let mut out = x.out;
        if DATE_FIELDS.contains(&field) {
            out = ascii_digits(&out);
        }
        encode_literals(&dotless_to_plain(&out))
    }

    fn expand_macro(&self, field: &str, name: &str, problems: &mut Vec<String>, report: bool, x: &mut Expansion) {
        // Cache raw, top-level expansions across fields and entries. Month
        // abbreviations differ in month fields; digit/literal normalization
        // still happens afterwards in `expand`. Recursive calls stay uncached
        // because cycle cuts and depth limits depend on their current stack.
        let key = (name.to_string(), field == "month");
        let cached = self.macro_cache.borrow().get(&key).cloned();
        let cached = cached.unwrap_or_else(|| {
            let mut expansion = Expansion::default();
            let mut problems = Vec::new();
            self.expand_macro_uncached(field, name, &mut problems, true, &mut expansion);
            let bytes = name.len() + expansion.out.len() + problems.iter().map(String::len).sum::<usize>() + 128;
            let cached = Rc::new(CachedExpansion { expansion, problems });
            if bytes <= MAX_MACRO_CACHE_BYTES.saturating_sub(self.macro_cache_bytes.get()) {
                self.macro_cache_bytes.set(self.macro_cache_bytes.get() + bytes);
                self.macro_cache.borrow_mut().insert(key, Rc::clone(&cached));
            }
            cached
        });
        let expanded = &cached.expansion;
        // A cached result must fit the *original* field quotas, including its
        // logical expansion count. If a preceding part would change where
        // expansion stops, walk again to retain the exact output and warnings.
        if (x.steps == 0 && x.out.is_empty())
            || (!expanded.stopped()
                && x.steps + expanded.steps <= MAX_EXPANSION_STEPS
                && x.out.len() + expanded.out.len() <= MAX_VALUE_BYTES)
        {
            x.out.push_str(&expanded.out);
            x.steps += expanded.steps;
            x.too_large |= expanded.too_large;
            x.too_complex |= expanded.too_complex;
            if report {
                problems.extend(cached.problems.iter().cloned());
            }
        } else {
            self.expand_macro_uncached(field, name, problems, report, x);
        }
    }

    fn expand_macro_uncached(
        &self,
        field: &str,
        name: &str,
        problems: &mut Vec<String>,
        report: bool,
        x: &mut Expansion,
    ) {
        x.steps += 1;
        if x.steps > MAX_EXPANSION_STEPS {
            x.too_complex = true;
        }
        if x.stopped() {
            return;
        }
        if x.stack.iter().any(|s| s == name) {
            if report {
                problems.push(format!("abbreviation `{name}` refers to itself; it is left empty"));
            }
            return;
        }
        if x.stack.len() >= MAX_MACRO_DEPTH {
            if report {
                problems.push(format!(
                    "abbreviations are nested more than {MAX_MACRO_DEPTH} levels deep; deeper levels are left empty"
                ));
            }
            return;
        }
        if let Some(parts) = self.macros.get(name) {
            x.stack.push(name.to_string());
            for part in parts {
                match part {
                    Part::Text(t) | Part::Number(t) => x.out.push_str(t),
                    Part::Macro(inner) => self.expand_macro_uncached(field, inner, problems, report, x),
                }
                x.check_size();
                if x.stopped() {
                    break;
                }
            }
            x.stack.pop();
            return;
        }
        if let Some((abbr, full)) = MONTHS.iter().find(|(abbr, _)| *abbr == name) {
            // In a month field the parser understands the abbreviation itself.
            x.out.push_str(if field == "month" { abbr } else { full });
            return;
        }
        if report {
            problems.push(format!(
                "undefined abbreviation `{name}`; the field using it is left empty"
            ));
        }
    }
}

/// Values already encode literal escaped braces as placeholders. Remaining
/// braces are groups, including those after an escaped pair of backslashes.
fn nesting_exceeds_limit(value: &str) -> bool {
    let mut depth = 0usize;
    for byte in value.bytes() {
        match byte {
            b'{' => {
                depth += 1;
                if depth > MAX_FIELD_NESTING {
                    return true;
                }
            }
            b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    false
}

/// State of expanding one field value.
#[derive(Default)]
struct Expansion {
    out: String,
    stack: Vec<String>,
    steps: usize,
    too_large: bool,
    too_complex: bool,
}

struct CachedExpansion {
    expansion: Expansion,
    problems: Vec<String>,
}

impl Expansion {
    fn check_size(&mut self) {
        if self.out.len() > MAX_VALUE_BYTES {
            self.too_large = true;
        }
    }

    fn stopped(&self) -> bool {
        self.too_large || self.too_complex
    }
}

/// The publication-date fields handled together by [`merge_dates`].
const DATE_PARTS: [&str; 4] = ["date", "year", "month", "day"];

/// Merge publication dates from sources ordered by precedence (own fields
/// first), following Biber: each of `date`, `year`, `month` and `day` is
/// inherited as a field; an entry's own `date` is used as written and never
/// gains inherited precision; an inherited plain `date` is combined with the
/// entry's own year, month or day (own month `mar` with an inherited
/// `2024-02-05` gives 2024-03-05). Ranges and approximate dates are kept
/// whole.
fn merge_dates(sources: &[Vec<(String, String)>]) -> Vec<(String, String)> {
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
        return compose(year, month, day);
    }
    let month = inherited("month");
    match (month.as_deref().map(month_number), inherited("year")) {
        // A month that is not a number or abbreviation ("Spring") stays as written.
        (Some(None), year) => year
            .map(|y| ("year".to_string(), y))
            .into_iter()
            .chain(month.map(|m| ("month".to_string(), m)))
            .collect(),
        (numeric, Some(year)) => compose(year, numeric.flatten(), inherited("day")),
        (numeric, None) => numeric
            .flatten()
            .map(|m| ("month".to_string(), m))
            .into_iter()
            .collect(),
    }
}

/// Fields for a year with optional month and day, exactly as precise as given.
fn compose(year: String, month: Option<String>, day: Option<String>) -> Vec<(String, String)> {
    let day = day.and_then(|d| d.parse::<u8>().ok()).filter(|d| (1..=31).contains(d));
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

/// Field names in the child for a field inherited through `crossref`,
/// following BibLaTeX's default inheritance rules (`biblatex.def`). An empty
/// list means the field is not inherited.
fn crossref_targets<'n>(parent: &str, child: &str, field: &'n str) -> Vec<&'n str> {
    const TITLE_FIELDS: [&str; 3] = ["title", "subtitle", "titleaddon"];
    const SKIPPED: [&str; 4] = ["shorttitle", "sorttitle", "indextitle", "indexsorttitle"];
    let book_part = matches!(child, "inbook" | "bookinbook" | "suppbook");
    let collection_like = matches!(
        child,
        "collection" | "reference" | "incollection" | "inreference" | "suppcollection"
    );
    let collection_part = matches!(child, "incollection" | "inreference" | "suppcollection");
    let prefix = match (parent, child) {
        ("mvbook", "book" | "inbook" | "bookinbook" | "suppbook") => Some("main"),
        ("mvcollection" | "mvreference", _) if collection_like => Some("main"),
        ("mvproceedings", "proceedings" | "inproceedings") => Some("main"),
        ("book", _) if book_part => Some("book"),
        ("collection" | "reference", _) if collection_part => Some("book"),
        ("proceedings", "inproceedings") => Some("book"),
        ("periodical", "article" | "suppperiodical") => Some("journal"),
        _ => None,
    };
    let mut targets = Vec::new();
    // The authors of a book are also the book author of its parts.
    if field == "author" && matches!(parent, "mvbook" | "book") && book_part {
        targets.push("bookauthor");
    }
    match prefix {
        Some(prefix) if TITLE_FIELDS.contains(&field) => targets.push(match (prefix, field) {
            ("main", "title") => "maintitle",
            ("main", "subtitle") => "mainsubtitle",
            ("main", _) => "maintitleaddon",
            ("book", "title") => "booktitle",
            ("book", "subtitle") => "booksubtitle",
            ("book", _) => "booktitleaddon",
            ("journal", "title") => "journaltitle",
            ("journal", "subtitle") => "journalsubtitle",
            _ => "journaltitleaddon",
        }),
        Some(_) if SKIPPED.contains(&field) => {}
        _ => targets.push(field),
    }
    targets
}

/// Render one field canonically. A month written as a three-letter
/// abbreviation stays bare so the parser reads it as a month.
fn render_field(name: &str, value: &str) -> String {
    let bare_month = name == "month" && MONTHS.iter().any(|(abbr, _)| *abbr == value);
    if bare_month {
        format!("  {name} = {value}")
    } else {
        // URLs are verbatim: command-looking path text remains literal.
        let value = if name == "url" {
            value.to_string()
        } else {
            protect_backslashes(value)
        };
        format!("  {name} = {{{value}}}")
    }
}

/// Replace the dotless `\\i` and `\\j` commands with plain letters so accents
/// apply: `{\\'\\i}` becomes `{\\'i}`, which the parser reads as "í".
fn dotless_to_plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find(['\\']) {
        out.push_str(&rest[..pos]);
        let after = &rest[pos + 1..];
        let letter = after.chars().next();
        let next = after.chars().nth(1);
        match letter {
            Some(l @ ('i' | 'j')) if next.is_none_or(|c| !c.is_ascii_alphabetic()) => {
                out.push(l);
                // A space after the command only terminates it.
                rest = after[1..].strip_prefix(' ').unwrap_or(&after[1..]);
            }
            _ => {
                out.push('\\');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Convert decimal digits from common scripts (Arabic-Indic, Persian,
/// N'Ko, Devanagari, Bengali, Gurmukhi, Gujarati, Oriya, Tamil, Telugu,
/// Thai, full-width) to ASCII digits.
pub(crate) fn ascii_digits(text: &str) -> String {
    const ZEROS: [u32; 12] = [
        0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0E50, 0xFF10,
    ];
    text.chars()
        .map(|c| {
            ZEROS
                .iter()
                .find(|&&zero| (zero..zero + 10).contains(&(c as u32)))
                .and_then(|&zero| char::from_digit(c as u32 - zero, 10))
                .unwrap_or(c)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::lexer::{parse_entry, parse_string, split_blocks};
    use super::*;

    fn resolve(src: &str, key: &str) -> Resolved {
        let blocks = split_blocks(src);
        let macros = blocks
            .iter()
            .filter(|b| b.kind == "string")
            .filter_map(parse_string)
            .collect();
        let entries: Vec<RawEntry> = blocks
            .iter()
            .filter(|b| b.kind != "string")
            .filter_map(|b| parse_entry(b).ok())
            .collect();
        let i = entries
            .iter()
            .position(|e| e.key == key)
            .unwrap_or_else(|| panic!("no {key}"));
        Resolver::new(macros, &entries).resolve(i)
    }

    #[test]
    fn wide_parent_shared_children_finish_before_the_deadline() {
        use std::process::Command;
        use std::time::{Duration, Instant};

        const CHILD: &str = "IBIS_FIELD_LOOKUP_TEST_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let mut text = String::from("@book{parent,title={Parent}");
            for i in 0..4_000 {
                text.push_str(&format!(",field{i:05}={{Value{i:05}}}"));
            }
            text.push_str("}\n");
            for i in 0..20 {
                text.push_str(&format!("@inbook{{child{i},title={{Child}},crossref={{parent}}}}\n"));
            }
            let entries: Vec<_> = split_blocks(&text)
                .iter()
                .map(|block| parse_entry(block).unwrap_or_else(|e| panic!("{e}")))
                .collect();
            let resolver = Resolver::new(Vec::new(), &entries);
            let start = Instant::now();
            for i in 0..entries.len() {
                let resolved = resolver.resolve(i);
                assert_eq!(resolved.fields.len(), 4_000 + if i == 0 { 1 } else { 2 });
                assert_eq!(
                    resolved.fields.get("field00000").map(String::as_str),
                    Some("Value00000")
                );
                assert_eq!(
                    resolved.fields.get("field03999").map(String::as_str),
                    Some("Value03999")
                );
                assert_eq!(
                    resolved.fields.get("title").map(String::as_str),
                    Some(if i == 0 { "Parent" } else { "Child" })
                );
                if i != 0 {
                    assert_eq!(resolved.fields.get("booktitle").map(String::as_str), Some("Parent"));
                }
                assert!(resolved.problems.is_empty());
            }
            eprintln!("synthetic resolver: {:?}", start.elapsed());
            return;
        }
        // Keep the deadline outside the resolver process so a regression cannot hang CI.
        let mut child = Command::new(std::env::current_exe().unwrap_or_else(|e| panic!("{e}")))
            .args([
                "--exact",
                "bibtex::resolve::tests::wide_parent_shared_children_finish_before_the_deadline",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .spawn()
            .unwrap_or_else(|e| panic!("{e}"));
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap_or_else(|e| panic!("{e}")) {
                assert!(status.success());
                break;
            }
            if start.elapsed() >= Duration::from_secs(2) {
                child.kill().unwrap_or_else(|e| panic!("{e}"));
                child.wait().unwrap_or_else(|e| panic!("{e}"));
                panic!("synthetic resolver exceeded the 2 second deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn indexed_fields_keep_order_duplicates_and_parent_precedence() {
        let definitions = "@book{p,title={Parent},publisher={Crossref},note={Parent note}}\n\
                           @xdata{first,publisher={First},pages={1}}\n\
                           @xdata{second,publisher={Second},pages={2}}\n";
        for links in ["crossref={p},xdata={first,second}", "xdata={first,second},crossref={p}"] {
            let resolved = resolve(
                &format!("{definitions}@inbook{{k,title={{First}},note={{Own}},title={{Last}},{links}}}"),
                "k",
            );
            assert_eq!(
                resolved.source,
                "@inbook{k,\n  title = {Last},\n  note = {Own},\n  publisher = {Second},\n  pages = {2},\n  booktitle = {Parent}}\n"
            );
            assert_eq!(
                resolved.problems,
                ["field `title` appears twice; the last value is used"]
            );
        }
    }

    #[test]
    fn excessive_nesting_is_omitted_after_expansion_and_inheritance() {
        let value = format!("{}x{}", "\\emph{".repeat(257), "}".repeat(257));
        for field in ["title", "journal", "note", "unknown", "url"] {
            for written in [format!("{{{value}}}"), format!("\"{value}\""), "bad # {}".to_string()] {
                let source = format!(
                    "@string{{bad={{{value}}}}}\n@xdata{{p,{field}={written}}}\n@article{{k,xdata={{p}},year=2024}}"
                );
                let result = resolve(&source, "k");
                assert!(!result.fields.contains_key(field), "{field}");
                assert!(!result.source.contains("emph"), "{field}");
                assert!(result.source.contains("year = {2024}"));
                assert_eq!(result.problems.len(), 1);
                assert!(result.problems[0].contains(field) && result.problems[0].contains("omitted"));
            }
        }
        let result = resolve(
            &format!("@proceedings{{p,title={{{value}}}}}\n@inproceedings{{k,title={{Safe}},crossref={{p}}}}"),
            "k",
        );
        assert_eq!(result.fields.get("title").map(String::as_str), Some("Safe"));
        assert!(!result.fields.contains_key("booktitle"));
        assert!(result.problems[0].contains("booktitle"));
    }

    #[test]
    fn nesting_guard_preserves_literal_braces_and_backslash_parity() {
        for slashes in 1..=4 {
            let prefix = "\\".repeat(slashes);
            let value = format!(
                "{}x{}",
                format!("{prefix}{{").repeat(257),
                format!("{prefix}}}").repeat(257)
            );
            assert_eq!(nesting_exceeds_limit(&encode_literals(&value)), slashes % 2 == 0);
        }
        assert!(!nesting_exceeds_limit(&encode_literals(
            &"\u{E000}\u{E001}\u{E002}\u{E003}".repeat(4_000)
        )));
    }

    #[test]
    fn output_is_one_flat_entry_without_links() {
        for src in [
            "@book{a,title={A},crossref={a}}",
            "@book{a,title={A},crossref={b}}\n@book{b,title={B},crossref={a}}",
        ] {
            let r = resolve(src, "a");
            assert!(
                r.problems.iter().any(|p| p.contains("cycle")),
                "{src}: {:?}",
                r.problems
            );
            assert_eq!(r.source.matches('@').count(), 1, "{}", r.source);
            assert!(!r.source.contains("crossref"), "{}", r.source);
        }
    }

    #[test]
    fn chained_macros_expand_fully() {
        let r = resolve(
            "@string{a = \"Alpha\"}\n@string{b = a # \" Beta\"}\n@article{m, journal = b}",
            "m",
        );
        assert!(r.source.contains("journal = {Alpha Beta}"), "{}", r.source);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
    }

    #[test]
    fn proceedings_title_becomes_booktitle_through_every_level() {
        let r = resolve(
            "@mvproceedings{c, title={Series}, year={2019}}\n@proceedings{b, title={Volume}, crossref={c}}\n@inproceedings{a, title={Paper}, crossref={b}}",
            "a",
        );
        assert_eq!(r.fields.get("title").map(String::as_str), Some("Paper"));
        assert_eq!(r.fields.get("booktitle").map(String::as_str), Some("Volume"));
        assert_eq!(r.fields.get("year").map(String::as_str), Some("2019"));
    }

    #[test]
    fn own_month_keeps_inherited_year_but_own_date_wins_over_it() {
        let month = resolve("@xdata{p,year=2024}\n@book{a,title={T},month=mar,xdata={p}}", "a");
        assert_eq!(month.fields.get("date").map(String::as_str), Some("2024-03"));
        let date = resolve("@xdata{p,year=2000}\n@book{a,title={T},date={2024},xdata={p}}", "a");
        assert_eq!(date.fields.get("date").map(String::as_str), Some("2024"));
        assert_eq!(date.fields.get("year"), None);
    }

    #[test]
    fn digits_in_date_fields_are_normalized_but_not_elsewhere() {
        let r = resolve("@article{d, title={২০২০ পর্যবেক্ষণ}, year={২০২০}, month=mar}", "d");
        assert!(r.source.contains("date = {2020-03}"), "{}", r.source);
        assert!(r.source.contains("title = {২০২০ পর্যবেক্ষণ}"));
    }

    #[test]
    fn dotless_i_and_j_become_plain_letters() {
        assert_eq!(dotless_to_plain("Garc{\\'\\i}a"), "Garc{\\'i}a");
        assert_eq!(dotless_to_plain("\\i nt \\item \\j"), "int \\item j");
    }
}
