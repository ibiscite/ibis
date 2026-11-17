//! Resolves `@string` macros and `crossref`/`xdata` inheritance, producing
//! one flat entry per reference for the field parser.
//!
//! Inheritance is computed here, field by field, rather than by the field
//! parser: an entry's own fields always win; `xdata` parents then fill
//! missing fields under the same names; a `crossref` parent fills the rest,
//! following BibLaTeX's default mapping (for example a proceedings `title`
//! becomes an inproceedings `booktitle`). Results are memoized within a root;
//! bounded clean first-parent snapshots are shared across roots. Recursion is bounded by
//! [`MAX_DEPTH`], cycles are reported and broken, and the parser never sees
//! a link it could follow. Indexed field positions and merged names avoid
//! quadratic name searches while preserving field order and precedence.

use super::Problem;
use crate::WarningCause;
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::Budget;
use super::dates::{DATE_PARTS, merge_dates};
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
/// Retained parent snapshots, including empty memo entries and field capacities.
const MAX_PARENT_CACHE_BYTES: usize = 4 * 1024 * 1024;

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
/// Shield verbatim dashes from biblatex 0.12's unconditional typography pass.
pub(crate) const HYPHEN: char = '\u{E004}';
/// Keep a real non-breaking space through upstream whitespace folding.
pub(crate) const NBSPACE: char = '\u{E005}';
pub(crate) const NARROW_NBSPACE: char = '\u{E006}';

/// Encode escaped braces and escape placeholder characters already present.
/// Decoded by `convert::clean`.
fn encode_literals(text: &str) -> Cow<'_, str> {
    if !text.contains([
        '\\',
        LEFT_BRACE,
        RIGHT_BRACE,
        BACKSLASH,
        ESCAPE,
        HYPHEN,
        NBSPACE,
        NARROW_NBSPACE,
    ]) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            LEFT_BRACE | RIGHT_BRACE | BACKSLASH | ESCAPE | HYPHEN | NBSPACE | NARROW_NBSPACE => {
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
    Cow::Owned(out)
}

/// Protect literal symbol commands until final cleaning, before upstream TeX
/// parsing or verbatim identifier decoding. Unknown commands stay unchanged.
pub(super) fn protect_backslashes(text: &str) -> Cow<'_, str> {
    if !text.as_bytes().contains(&b'\\') {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ESCAPE || c == '\\' && chars.peek() == Some(&'\\') {
            out.push(c);
            out.extend(chars.next());
            continue;
        }
        // An empty tilde accent is the literal symbol. Shield it before the
        // parser makes it indistinguishable from a typography tie (`~`).
        if c == '\\' && chars.clone().take(3).eq("~{}".chars()) {
            out.push(ESCAPE);
            out.push('~');
            for _ in 0..3 {
                chars.next();
            }
            continue;
        }
        let symbol = (c == '\\')
            .then(|| {
                [
                    ("textbackslash", BACKSLASH),
                    ("textbraceleft", LEFT_BRACE),
                    ("textbraceright", RIGHT_BRACE),
                    ("textasciitilde", '~'),
                    ("textasciicircum", '^'),
                    ("textasciigrave", '`'),
                    ("textquotesingle", '\''),
                ]
                .into_iter()
                .find(|(name, _)| {
                    chars.clone().take(name.len()).eq(name.chars())
                        && chars.clone().nth(name.len()).is_none_or(|c| !c.is_ascii_alphabetic())
                })
            })
            .flatten();
        if let Some((name, symbol)) = symbol {
            if !matches!(symbol, BACKSLASH | LEFT_BRACE | RIGHT_BRACE) {
                out.push(ESCAPE);
            }
            out.push(symbol);
            for _ in 0..name.len() {
                chars.next();
            }
            if chars.clone().take(2).eq("{}".chars()) {
                chars.next();
                chars.next();
            } else {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
        } else {
            out.push(c);
        }
    }
    Cow::Owned(out)
}

pub(super) const MONTHS: [(&str, &str); 12] = [
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

#[derive(Clone)]
#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
struct EffectiveField {
    name: String,
    value: String,
    line: usize,
    /// The original field if crossref mapped it to a different name.
    renamed_from: Option<&'static str>,
}

/// Ordered field list of one entry after inheritance.
type Fields = Rc<Vec<EffectiveField>>;
type Memo = HashMap<(usize, usize), Fields>;

/// Everything needed to resolve entries from one file.
pub(crate) struct Resolver<'a, 's> {
    pub(super) budget: &'a Budget,
    macros: HashMap<String, Vec<usize>>,
    definitions: Vec<Vec<Part<'s>>>,
    macro_cache: RefCell<HashMap<(usize, bool), Rc<CachedExpansion>>>,
    macro_cache_bytes: Cell<usize>,
    parent_cache: RefCell<HashMap<(usize, usize), Rc<Memo>>>,
    parent_cache_bytes: Cell<usize>,
    entries: &'a [RawEntry<'s>],
    index: HashMap<String, usize>,
    index_folded: HashMap<String, usize>,
    /// Work counts that let tests prove a wide file stays linear without a clock.
    #[cfg(test)]
    work: Work,
}

#[cfg(test)]
#[derive(Default)]
struct Work {
    /// Field values expanded, across every entry and parent evaluation.
    expansions: Cell<usize>,
    /// Field-name index lookups: one per own field and one per merged candidate.
    name_probes: Cell<usize>,
}

#[cfg(test)]
fn tick(cell: &Cell<usize>) {
    cell.set(cell.get() + 1);
}

/// An entry ready for parsing, plus problems found while resolving it.
#[derive(Default)]
pub(crate) struct Resolved {
    /// One flat entry: own and inherited fields, no links.
    pub source: String,
    /// Field values after expansion and inheritance, for checks that need raw text.
    pub fields: HashMap<String, String>,
    pub problems: Vec<Problem>,
}

/// State of resolving one entry.
struct Walk {
    path: Vec<usize>,
    memo: Memo,
    evaluations: usize,
    problems: Vec<Problem>,
}

impl<'a, 's> Resolver<'a, 's> {
    pub(crate) fn new(macros: Vec<(String, Vec<Part<'s>>)>, entries: &'a [RawEntry<'s>], budget: &'a Budget) -> Self {
        let mut index = HashMap::new();
        let mut index_folded = HashMap::new();
        for (i, entry) in entries.iter().enumerate() {
            if !budget.charge(entry.key.len().saturating_mul(6)) {
                break;
            }
            index.entry(entry.key.clone()).or_insert(i);
            index_folded.entry(entry.key.to_lowercase()).or_insert(i);
        }
        // Versions are retained once, not as a cloned environment per entry.
        // The epoch bounds each use to definitions already seen at that point.
        let mut versions: HashMap<String, Vec<usize>> = HashMap::new();
        let mut definitions = Vec::new();
        for (name, parts) in macros {
            if !budget.charge(name.len().saturating_add(128)) {
                break;
            }
            versions.entry(name).or_default().push(definitions.len());
            definitions.push(parts);
        }
        Resolver {
            budget,
            macros: versions,
            definitions,
            macro_cache: RefCell::new(HashMap::new()),
            macro_cache_bytes: Cell::new(0),
            parent_cache: RefCell::new(HashMap::new()),
            parent_cache_bytes: Cell::new(0),
            entries,
            index,
            index_folded,
            #[cfg(test)]
            work: Work::default(),
        }
    }

    /// One field-name lookup in an index (never a search through the fields).
    fn indexed<T>(&self, lookup: T) -> T {
        #[cfg(test)]
        tick(&self.work.name_probes);
        lookup
    }

    fn find(&self, key: &str) -> Option<usize> {
        self.index
            .get(key)
            .or_else(|| self.index_folded.get(&key.to_lowercase()))
            .copied()
    }

    fn macro_id(&self, name: &str, epoch: usize) -> Option<usize> {
        let versions = self.macros.get(name)?;
        let end = versions.partition_point(|&id| id < epoch);
        end.checked_sub(1).map(|i| versions[i])
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
        let entry = &self.entries[i];
        let mut problems = walk.problems;
        // The completed root belongs to this walk. Release its memo before
        // taking the fields; cached parent snapshots keep their own ownership.
        drop(walk.memo);
        let fields: Vec<_> = Rc::unwrap_or_clone(fields)
            .into_iter()
            .take_while(|field| {
                self.budget.charge(
                    field
                        .name
                        .len()
                        .saturating_add(field.value.len())
                        .saturating_mul(8)
                        .saturating_add(256),
                )
            })
            .filter(|field| {
                if let Err(cause) = validate_field_structure(&field.value) {
                    let mut problem = Problem::new(cause, Some(&field.name));
                    problem.field = field.renamed_from.or(problem.field);
                    problem.line = (field.line != entry.line).then_some(field.line);
                    problems.push(problem);
                    false
                } else {
                    true
                }
            })
            .map(|field| (field.name, field.value))
            .collect();
        if !self
            .budget
            .charge(entry.kind.len().saturating_add(entry.key.len()).saturating_mul(4))
        {
            return Resolved::default();
        }
        let capacity = fields
            .iter()
            .fold(entry.kind.len() + entry.key.len() + 6, |n, (name, value)| {
                n + name.len() + value.len() + 9
            });
        let mut source = String::with_capacity(capacity);
        source.push('@');
        source.push_str(&entry.kind);
        source.push('{');
        source.push_str(&entry.key);
        source.push_str(",\n");
        for (i, (name, value)) in fields.iter().enumerate() {
            if i != 0 {
                source.push_str(",\n");
            }
            render_field(&mut source, name, value);
        }
        source.push_str("}\n");
        // The same problem can be found many times; report each one once.
        let mut unique = HashSet::new();
        problems.retain(|p| self.budget.charge(p.size().saturating_mul(4)) && unique.insert(*p));
        Resolved {
            source,
            fields: fields.into_iter().collect(),
            problems,
        }
    }

    /// Fields of entry `idx` after inheritance, following at most `remaining`
    /// further levels. Memoized by (entry, remaining depth).
    fn effective(&self, idx: usize, remaining: usize, walk: &mut Walk) -> Fields {
        if !self.budget.charge(0) {
            return Rc::default();
        }
        if let Some(fields) = walk.memo.get(&(idx, remaining)) {
            return Rc::clone(fields);
        }
        let entry = &self.entries[idx];
        let root = walk.path.first().copied() == Some(idx);
        let origin = (!root).then_some(entry.line);
        let problem_at_entry = |cause, field: &str| Problem {
            line: origin,
            ..Problem::new(cause, Some(field))
        };
        if !root {
            let cached = self.parent_cache.borrow().get(&(idx, remaining)).cloned();
            if let Some(cached) = cached {
                // Charge before touching the local memo, so replay is atomic on exhaustion.
                if !self.budget.charge(cached.len().saturating_mul(128)) {
                    return Rc::default();
                }
                let missing = cached.keys().filter(|key| !walk.memo.contains_key(key)).count();
                // Reevaluate at the boundary to apply the per-walk evaluation limit.
                if walk.evaluations.saturating_add(missing) < MAX_EVALUATIONS {
                    for (key, fields) in cached.iter() {
                        walk.memo.entry(*key).or_insert_with(|| Rc::clone(fields));
                    }
                    walk.evaluations += missing;
                    return Rc::clone(&cached[&(idx, remaining)]);
                }
            }
        }
        // An empty memo at the first parent makes its completed snapshot self-contained.
        let cacheable = !root && walk.path.len() == 2 && walk.memo.is_empty();
        walk.evaluations += 1;
        // Problems in the entry itself, or in an `@xdata` helper it uses
        // (helpers are not loaded on their own), are reported.
        let report = root || entry.kind == "xdata";
        let mut own: Vec<EffectiveField> = Vec::new();
        let mut own_positions: HashMap<&str, usize> = HashMap::new();
        let mut xdata_keys = Vec::new();
        let mut crossref_key = None;
        for field in &entry.fields {
            if !self
                .budget
                .charge(field.name.len().saturating_mul(4).saturating_add(256))
            {
                return Rc::default();
            }
            let mut field_problems = Vec::new();
            #[cfg(test)]
            tick(&self.work.expansions);
            let value = self.expand(
                &field.name,
                &field.parts,
                entry.macro_epoch,
                &mut field_problems,
                report,
            );
            if !self.budget.charge(
                value
                    .len()
                    .saturating_mul(4)
                    .saturating_add(field_problems.iter().map(Problem::size).sum::<usize>())
                    .saturating_add(field_problems.len().saturating_mul(entry.key.len().saturating_add(128))),
            ) {
                return Rc::default();
            }
            if !field_problems.is_empty() {
                // A cached abbreviation problem gets its location from this
                // use. Attaching a static name costs no heap allocation.
                let field = super::diagnostic_field(&field.name);
                walk.problems.extend(field_problems.into_iter().map(|problem| Problem {
                    field: problem.field.or(field),
                    line: origin,
                    ..problem
                }));
            }
            match field.name.as_str() {
                "xdata" => xdata_keys.extend(
                    super::convert::literal_value(&value)
                        .split(',')
                        .map(|k| k.trim().to_string())
                        .filter(|k| !k.is_empty()),
                ),
                "crossref" => crossref_key = Some(super::convert::literal_value(&value)).filter(|k| !k.is_empty()),
                _ => {
                    // As in Biber and Pandoc, a repeated field keeps its last value.
                    if let Some(&position) = self.indexed(own_positions.get(field.name.as_str())) {
                        let existing = &mut own[position];
                        if root {
                            walk.problems
                                .push(problem_at_entry(WarningCause::Conflicting, &field.name));
                        }
                        existing.value = value;
                    } else {
                        own_positions.insert(field.name.as_str(), own.len());
                        own.push(EffectiveField {
                            name: field.name.clone(),
                            value,
                            line: entry.line,
                            renamed_from: None,
                        });
                    }
                }
            }
        }

        // Date normalization can consume components before the final field guard.
        // Check selected, fully expanded values, retaining last-value precedence.
        own.retain(|field| {
            if !DATE_PARTS.contains(&field.name.as_str()) {
                return true;
            }
            if !self.budget.charge(field.value.len()) {
                return false;
            }
            let Err(cause) = validate_field_structure(&field.value) else {
                return true;
            };
            walk.problems.push(problem_at_entry(cause, &field.name));
            false
        });

        // A parent listed more than once counts at its last position, where
        // it has the highest precedence (as in Biber).
        let mut last_seen = HashSet::new();
        let mut xdata_keys: Vec<String> = xdata_keys
            .into_iter()
            .rev()
            .filter(|k| last_seen.insert(k.clone()))
            .collect();
        xdata_keys.reverse();
        let mut parents: Vec<(Fields, Option<&str>)> = Vec::new();
        let links = xdata_keys
            .iter()
            .map(|k| (k.as_str(), false))
            .chain(crossref_key.iter().map(|k| (k.as_str(), true)));
        let mut seen_targets = HashSet::new();
        for (target, is_crossref) in links {
            let link = if is_crossref { "crossref" } else { "xdata" };
            if !self.budget.charge(
                entry
                    .key
                    .len()
                    .saturating_add(target.len().saturating_mul(8))
                    .saturating_add(256),
            ) {
                return Rc::default();
            }
            let Some(t) = self.find(target) else {
                walk.problems.push(problem_at_entry(WarningCause::Inheritance, link));
                continue;
            };
            if !seen_targets.insert(t) {
                continue;
            }
            if walk.path.contains(&t) {
                walk.problems.push(problem_at_entry(WarningCause::Inheritance, link));
                continue;
            }
            if remaining == 0 {
                walk.problems
                    .push(problem_at_entry(WarningCause::InheritanceLimit, link));
                continue;
            }
            if walk.evaluations >= MAX_EVALUATIONS {
                walk.problems
                    .push(problem_at_entry(WarningCause::InheritanceLimit, link));
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
        let mut sources = vec![own];
        let (crossref, xdata): (Vec<_>, Vec<_>) = parents.into_iter().partition(|(_, kind)| kind.is_some());
        for (inherited, crossref_kind) in xdata.into_iter().rev().chain(crossref) {
            if !self.budget.charge(inherited.len().saturating_mul(32)) {
                return Rc::default();
            }
            let mapped = inherited
                .iter()
                .filter(|field| !NOT_INHERITED.contains(&field.name.as_str()))
                .take_while(|field| {
                    self.budget
                        .charge(field.name.len().saturating_add(field.value.len()).saturating_mul(4))
                })
                .flat_map(|field| {
                    let targets = match crossref_kind {
                        Some(parent_kind) => crossref_targets(parent_kind, &entry.kind, &field.name),
                        None => vec![field.name.as_str()],
                    };
                    targets.into_iter().map(move |name| EffectiveField {
                        name: name.to_string(),
                        value: field.value.clone(),
                        line: field.line,
                        renamed_from: field.renamed_from.or_else(|| {
                            if name == field.name {
                                None
                            } else {
                                super::diagnostic_field(&field.name)
                            }
                        }),
                    })
                })
                .collect();
            sources.push(mapped);
        }
        let mut merged = Vec::new();
        let mut merged_names = HashSet::new();
        for source in &sources {
            for field in source {
                if !self.budget.charge(
                    field
                        .name
                        .len()
                        .saturating_mul(4)
                        .saturating_add(field.value.len().saturating_mul(8)),
                ) {
                    return Rc::default();
                }
                if !DATE_PARTS.contains(&field.name.as_str()) && self.indexed(merged_names.insert(field.name.as_str()))
                {
                    merged.push(field.clone());
                }
            }
        }
        // The component values were checked at their own entry. The date
        // composer can combine components from more than one source.
        let date_sources: Vec<_> = sources
            .into_iter()
            .map(|source| {
                source
                    .into_iter()
                    .filter(|field| DATE_PARTS.contains(&field.name.as_str()))
                    .map(|field| (field.name, field.value))
                    .collect()
            })
            .collect();
        merged.extend(
            merge_dates(&date_sources, root, &mut walk.problems)
                .into_iter()
                .map(|(name, value)| EffectiveField {
                    name,
                    value,
                    line: entry.line,
                    renamed_from: None,
                }),
        );
        let fields: Fields = Rc::new(merged);
        if !self.budget.exhausted() {
            walk.memo.insert((idx, remaining), Rc::clone(&fields));
            if cacheable && walk.problems.is_empty() {
                self.cache_parent((idx, remaining), &walk.memo);
            }
        }
        fields
    }

    fn cache_parent(&self, key: (usize, usize), memo: &Memo) {
        if self.parent_cache.borrow().contains_key(&key) {
            return;
        }
        let available = MAX_PARENT_CACHE_BYTES.saturating_sub(self.parent_cache_bytes.get());
        if memo.len().saturating_mul(256).saturating_add(256) > available
            || !self.budget.charge(memo.len().saturating_mul(128))
        {
            return;
        }
        let slots = memo.values().fold(0usize, |n, fields| n.saturating_add(fields.len()));
        if !self.budget.charge(slots.saturating_mul(128)) {
            return;
        }
        let bytes = memo.values().fold(256usize, |n, fields| {
            let allocation = fields.capacity().saturating_mul(std::mem::size_of::<EffectiveField>());
            fields
                .iter()
                .fold(n.saturating_add(256).saturating_add(allocation), |n, field| {
                    n.saturating_add(field.name.capacity())
                        .saturating_add(field.value.capacity())
                })
        });
        if bytes <= available {
            self.parent_cache.borrow_mut().insert(key, Rc::new(memo.clone()));
            self.parent_cache_bytes.set(self.parent_cache_bytes.get() + bytes);
        }
    }

    /// Concatenate parts, expanding macros (and macros used inside macros).
    /// Values larger than [`MAX_VALUE_BYTES`], or needing more than
    /// [`MAX_EXPANSION_STEPS`] expansions, are left empty and reported.
    fn expand(
        &self,
        field: &str,
        parts: &[Part<'_>],
        epoch: usize,
        problems: &mut Vec<Problem>,
        report: bool,
    ) -> String {
        let mut x = Expansion::default();
        for part in parts {
            match part {
                Part::Text(t) => {
                    if !self.budget.charge(t.len()) {
                        return String::new();
                    }
                    x.out.push_str(t);
                }
                Part::Number(t) => {
                    if !self.budget.charge(t.len()) {
                        return String::new();
                    }
                    x.out.push_str(t);
                }
                Part::Macro(name) => self.expand_macro(field, name, epoch, problems, report, &mut x),
            }
            x.check_size();
            if x.stopped() || self.budget.exhausted() {
                break;
            }
        }
        if !self.budget.charge(
            field
                .len()
                .saturating_add(x.out.len().saturating_mul(8))
                .saturating_add(256),
        ) {
            return String::new();
        }
        if x.stopped() {
            if report {
                problems.push(Problem::new(
                    if x.too_large {
                        WarningCause::TooLarge
                    } else {
                        WarningCause::Abbreviation
                    },
                    Some(field),
                ));
            }
            return String::new();
        }
        let mut out = x.out;
        if DATE_FIELDS.contains(&field) && !out.is_ascii() {
            out = ascii_digits(&out);
        }
        let out = match dotless_to_plain(&out) {
            Cow::Borrowed(_) => out,
            Cow::Owned(plain) => plain,
        };
        match encode_literals(&out) {
            Cow::Borrowed(_) => out,
            Cow::Owned(encoded) => encoded,
        }
    }

    fn expand_macro(
        &self,
        field: &str,
        name: &str,
        epoch: usize,
        problems: &mut Vec<Problem>,
        report: bool,
        x: &mut Expansion,
    ) {
        if !self.budget.charge(name.len().saturating_mul(4)) {
            return;
        }
        // Cache raw, top-level expansions across fields and entries. Month
        // abbreviations differ in month fields; digit/literal normalization
        // still happens afterwards in `expand`. Recursive calls stay uncached
        // because cycle cuts and depth limits depend on their current stack.
        let Some(id) = self.macro_id(name, epoch) else {
            self.expand_macro_uncached(field, name, epoch, problems, report, x);
            return;
        };
        let key = (id, field == "month");
        let cached = self.macro_cache.borrow().get(&key).cloned();
        let cached = cached.unwrap_or_else(|| {
            let mut expansion = Expansion::default();
            let mut problems = Vec::new();
            self.expand_macro_uncached(field, name, epoch, &mut problems, true, &mut expansion);
            let bytes = name.len() + expansion.out.len() + problems.iter().map(Problem::size).sum::<usize>() + 128;
            let cached = Rc::new(CachedExpansion {
                expansion,
                problems,
                bytes,
            });
            if !self.budget.exhausted() && bytes <= MAX_MACRO_CACHE_BYTES.saturating_sub(self.macro_cache_bytes.get()) {
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
            if !self.budget.charge(cached.bytes.saturating_mul(4)) {
                return;
            }
            x.out.push_str(&expanded.out);
            x.steps += expanded.steps;
            x.too_large |= expanded.too_large;
            x.too_complex |= expanded.too_complex;
            if report {
                problems.extend(cached.problems.iter().copied());
            }
        } else {
            self.expand_macro_uncached(field, name, epoch, problems, report, x);
        }
    }

    fn expand_macro_uncached(
        &self,
        field: &str,
        name: &str,
        epoch: usize,
        problems: &mut Vec<Problem>,
        report: bool,
        x: &mut Expansion,
    ) {
        if !self.budget.charge(
            name.len()
                .saturating_mul(x.stack.len().saturating_add(4))
                .saturating_add(field.len())
                .saturating_add(256),
        ) {
            return;
        }
        x.steps += 1;
        if x.steps > MAX_EXPANSION_STEPS {
            x.too_complex = true;
        }
        if x.stopped() {
            return;
        }
        let id = self.macro_id(name, epoch);
        if id.is_some_and(|id| x.stack.contains(&id)) {
            if report {
                problems.push(Problem::new(WarningCause::Abbreviation, None));
            }
            return;
        }
        if x.stack.len() >= MAX_MACRO_DEPTH {
            if report {
                problems.push(Problem::new(WarningCause::Abbreviation, None));
            }
            return;
        }
        if let Some(id) = id {
            x.stack.push(id);
            for part in &self.definitions[id] {
                match part {
                    Part::Text(t) => {
                        if !self.budget.charge(t.len()) {
                            break;
                        }
                        x.out.push_str(t);
                    }
                    Part::Number(t) => {
                        if !self.budget.charge(t.len()) {
                            break;
                        }
                        x.out.push_str(t);
                    }
                    // A definition captures its own environment, including
                    // itself for the existing warned self-reference check.
                    Part::Macro(inner) => self.expand_macro_uncached(field, inner, id + 1, problems, report, x),
                }
                x.check_size();
                if x.stopped() || self.budget.exhausted() {
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
            problems.push(Problem::new(WarningCause::Abbreviation, None));
        }
    }
}

/// Values already encode literal escaped braces as placeholders. Remaining
/// braces are groups, including those after an escaped pair of backslashes.
/// Every prefix must balance within the value, and the final depth must be zero,
/// so neither parser input nor the conversion fallback can escape its field.
fn validate_field_structure(value: &str) -> Result<(), WarningCause> {
    let mut depth = 0usize;
    for byte in value.bytes() {
        match byte {
            b'{' => {
                depth += 1;
                if depth > MAX_FIELD_NESTING {
                    return Err(WarningCause::TooLarge);
                }
            }
            b'}' => {
                let Some(remaining) = depth.checked_sub(1) else {
                    return Err(WarningCause::Unbalanced);
                };
                depth = remaining;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return Err(WarningCause::Unbalanced);
    }
    Ok(())
}

/// State of expanding one field value.
#[derive(Default)]
struct Expansion {
    out: String,
    stack: Vec<usize>,
    steps: usize,
    too_large: bool,
    too_complex: bool,
}

struct CachedExpansion {
    bytes: usize,
    expansion: Expansion,
    problems: Vec<Problem>,
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
fn render_field(out: &mut String, name: &str, value: &str) {
    // Shield spacing only after typed resolution, and only for text fields.
    // Names, dates and graph keys need ordinary whitespace for their parsers;
    // raw paths and verbatim identifiers are decoded separately from source.
    const TEXT_FIELDS: &[&str] = &[
        "title",
        "subtitle",
        "titleaddon",
        "shorttitle",
        "maintitle",
        "mainsubtitle",
        "maintitleaddon",
        "booktitle",
        "booksubtitle",
        "booktitleaddon",
        "journaltitle",
        "journalsubtitle",
        "journaltitleaddon",
        "journal",
        "abstract",
        "note",
        "publisher",
        "institution",
        "school",
        "organization",
        "address",
        "location",
        "series",
        "eventtitle",
        "type",
    ];
    let text = if TEXT_FIELDS.contains(&name) && value.contains(['\u{a0}', '\u{202f}']) {
        Cow::Owned(
            value
                .replace('\u{a0}', &NBSPACE.to_string())
                .replace('\u{202f}', &NARROW_NBSPACE.to_string()),
        )
    } else {
        Cow::Borrowed(value)
    };
    let value = text.as_ref();
    let bare_month = name == "month" && MONTHS.iter().any(|(abbr, _)| *abbr == value);
    out.push_str("  ");
    out.push_str(name);
    out.push_str(" = ");
    if bare_month {
        out.push_str(value);
    } else {
        out.push('{');
        // Keep the parser's canonical verbatim field intact too; final URL/DOI
        // decoding uses the resolved source rather than decoding these twice.
        let verbatim = matches!(name, "url" | "doi");
        let value = if verbatim {
            Cow::Owned(value.replace('-', &HYPHEN.to_string()))
        } else {
            Cow::Borrowed(value)
        };
        // URLs are verbatim: command-looking path text remains literal.
        if name == "url" {
            out.push_str(&value);
        } else {
            out.push_str(&protect_backslashes(&value));
        }
        out.push('}');
    }
}

/// Replace the dotless `\\i` and `\\j` commands with plain letters so accents
/// apply: `{\\'\\i}` becomes `{\\'i}`, which the parser reads as "í".
fn dotless_to_plain(text: &str) -> Cow<'_, str> {
    if !text.as_bytes().contains(&b'\\') {
        return Cow::Borrowed(text);
    }
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
    Cow::Owned(out)
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
