//! The words every front end shows for what the engine reports (#1675).
//!
//! The engine returns codes and arguments; this one catalogue,
//! `locales/en-US.ftl` at the repository root, words them for the command
//! line, agents (MCP) and the app, which also reads its own interface text
//! from it. A message's value is the sentence everyone reads; its `.cli`,
//! `.agent` and `.app` attributes say what to do next where that differs.
//! Ids come from the codes' wire names: `ErrorCode::SourceUnavailable` is
//! `error-source-unavailable`.
use fluent_bundle::{FluentArgs, FluentResource, FluentValue, concurrent::FluentBundle};
use ibis_core::contract::{EngineError, ErrorCode};
use serde::Serialize;
use std::sync::OnceLock;

/// The English catalogue, as it is bundled into every build: the source every
/// translation follows message for message.
pub const SOURCE: &str = include_str!("../../../locales/en-US.ftl");

/// A language the catalogue is written in. The interface language only: a
/// citation's language stays its style's or locale's own.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, serde::Deserialize, schemars::JsonSchema)]
pub enum Language {
    #[default]
    #[serde(rename = "en-US")]
    English,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "ar")]
    Arabic,
}

impl Language {
    pub const ALL: [Self; 3] = [Self::English, Self::German, Self::Arabic];

    /// Its BCP 47 tag, as the catalogue file and the app's `lang` use it.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::German => "de",
            Self::Arabic => "ar",
        }
    }

    /// Its catalogue, bundled into every build.
    pub const fn source(self) -> &'static str {
        match self {
            Self::English => SOURCE,
            Self::German => include_str!("../../../locales/de.ftl"),
            Self::Arabic => include_str!("../../../locales/ar.ftl"),
        }
    }

    /// The first of the system's preferred languages Ibis speaks, else English.
    pub fn negotiate<'a>(preferred: impl IntoIterator<Item = &'a str>) -> Self {
        preferred
            .into_iter()
            .find_map(|tag| {
                let primary = tag.split(['-', '_', '.', '@']).next()?.to_ascii_lowercase();
                Self::ALL
                    .into_iter()
                    .find(|language| language.tag().split('-').next() == Some(primary.as_str()))
            })
            .unwrap_or_default()
    }
}

static CURRENT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Speak this language for the rest of the process. The app calls it once at
/// start; the command line and MCP never do, so they stay English.
pub fn use_language(language: Language) {
    let index = Language::ALL.iter().position(|each| *each == language).unwrap_or_default();
    CURRENT.store(u8::try_from(index).unwrap_or_default(), std::sync::atomic::Ordering::Relaxed);
}

/// The language this process speaks.
pub fn language() -> Language {
    Language::ALL
        .get(usize::from(CURRENT.load(std::sync::atomic::Ordering::Relaxed)))
        .copied()
        .unwrap_or_default()
}

/// Who reads a message, for the attribute that tells them what to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// Someone at a terminal, with the command line's options.
    Cli,
    /// An AI agent calling MCP tools.
    Agent,
    /// Someone in the app's picker or Settings.
    App,
}

impl Audience {
    const fn attribute(self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::Agent => "agent",
            Self::App => "app",
        }
    }
}

/// The groups of engine codes, each a message per code.
pub const GROUPS: [&str; 9] = [
    "error",
    "reason",
    "problem",
    "warning",
    "source-warning",
    "loss",
    "notice",
    "form",
    "cause",
];

fn bundle_in(language: Language) -> &'static FluentBundle<FluentResource> {
    static BUNDLES: [OnceLock<FluentBundle<FluentResource>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let index = Language::ALL.iter().position(|each| *each == language).unwrap_or_default();
    BUNDLES[index].get_or_init(|| {
        let resource = FluentResource::try_new(language.source().to_owned()).unwrap_or_else(|(resource, _)| resource);
        let tag: unic_langid::LanguageIdentifier = language.tag().parse().unwrap_or_default();
        let mut bundle = FluentBundle::new_concurrent(vec![tag]);
        // Terminals, agents, native menus and accessible names read plain
        // text: no invisible direction marks around arguments.
        bundle.set_use_isolating(false);
        // A duplicate id keeps the first; the catalogue tests refuse both.
        let _ = bundle.add_resource(resource);
        bundle
    })
}

/// The id of a code in its group: `id("error", ErrorCode::NotFound)` is `error-not-found`.
pub fn id(group: &str, code: impl Serialize) -> String {
    let name = serde_json::to_value(code)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    format!("{group}-{name}")
}

/// A message's value, or its attribute, with these arguments. A missing
/// message reads as its id, which the catalogue test makes impossible.
pub fn text(id: &str, attribute: Option<&str>, args: &[(&str, &str)]) -> Option<String> {
    text_in(language(), id, attribute, args)
}

/// [`text`] in a given language, whatever this process speaks.
pub fn text_in(language: Language, id: &str, attribute: Option<&str>, args: &[(&str, &str)]) -> Option<String> {
    let bundle = bundle_in(language);
    let message = bundle.get_message(id)?;
    let pattern = match attribute {
        Some(name) => message.get_attribute(name)?.value(),
        None => message.value()?,
    };
    let mut values = FluentArgs::new();
    for (name, value) in args {
        values.set(*name, FluentValue::from(*value));
    }
    let mut errors = Vec::new();
    let text = bundle.format_pattern(pattern, Some(&values), &mut errors);
    Some(text.into_owned())
}

/// The sentence for a code, then what this audience can do about it, if anything.
pub fn sentence(group: &str, code: impl Serialize, audience: Audience, args: &[(&str, &str)]) -> String {
    let id = id(group, code);
    render(&id, audience, None, args)
}

/// The same diagnostic with the reference named where the client knows it.
pub fn citation(error: ibis_core::contract::CitationError, audience: Audience, args: &[(&str, &str)]) -> String {
    let problem = error.problem();
    let mut arguments = args.to_vec();
    arguments.extend(
        problem
            .info
            .message
            .args
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str())),
    );
    render(error.message_id(), audience, None, &arguments)
}

fn render(
    id: &str,
    audience: Audience,
    context: Option<ibis_core::contract::CitationSurface>,
    args: &[(&str, &str)],
) -> String {
    if audience == Audience::Cli && context.is_none() {
        let detail = if args.iter().any(|(key, value)| *key == "form" && *value != "normal") {
            text(id, Some("cli-form"), args)
        } else if args.iter().any(|(key, value)| *key == "options" && !value.is_empty()) {
            text(id, Some("cli-options"), args)
        } else {
            None
        };
        if let Some(detail) = detail {
            return detail;
        }
    }
    let named = args.iter().any(|(key, _)| *key == "reference");
    let app_detail = (audience == Audience::App && named && args.iter().any(|(key, _)| *key == "form"))
        .then(|| text(id, Some("app-named"), args))
        .flatten();
    let said = app_detail
        .or_else(|| named.then(|| text(id, Some("named"), args)).flatten())
        .or_else(|| text(id, None, args))
        .unwrap_or_else(|| id.into());
    let attribute = match context {
        Some(ibis_core::contract::CitationSurface::Bibliography) => "bibliography",
        None => audience.attribute(),
    };
    match text(id, Some(attribute), args) {
        Some(fix) => format!("{said} {fix}"),
        None => said,
    }
}

/// Why a library skipped a record or asks for a field to be checked, naming
/// the field when the source gave one. `None` when the warning gives no cause.
pub fn cause(warning: &ibis_sources::SourceWarning) -> Option<String> {
    let id = id("cause", warning.cause?);
    warning
        .field
        .as_deref()
        .and_then(|field| text(&id, Some("field"), &[("field", field)]))
        .or_else(|| text(&id, None, &[]))
}

/// An engine error in words: its reason when it gives one, else its code.
pub fn error(error: &EngineError, audience: Audience) -> String {
    problem(&(*error).into(), audience)
}

/// Render the engine's declared message and arguments at the last boundary.
pub fn problem(problem: &ibis_core::contract::Problem, audience: Audience) -> String {
    message(&problem.info.message, audience)
}

/// A declared message in this audience's words, as the app's catalogue says it.
pub fn message(message: &ibis_core::contract::Message, audience: Audience) -> String {
    let args: Vec<_> = message
        .args
        .iter()
        .map(|(name, value)| (name.as_str(), value.as_str()))
        .collect();
    render(&message.id, audience, message.context, &args)
}

/// The fallback words for a code alone.
pub fn code(code: ErrorCode, audience: Audience) -> String {
    sentence("error", code, audience, &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::{CitationError, CitationForm, CitationWarningCode, ErrorReason, LossField, Notice};
    use schemars::JsonSchema;
    use std::collections::BTreeSet;

    #[test]
    fn recovery_messages_resolve_without_source_text_or_missing_arguments() {
        let mut problems: Vec<ibis_core::contract::Problem> = ErrorCode::ALL
            .into_iter()
            .map(|code| EngineError::new(code).into())
            .collect();
        problems.extend(CitationError::ALL.into_iter().map(CitationError::problem));
        for name in names::<ErrorReason>() {
            let reason = serde_json::from_value(serde_json::json!(name)).unwrap_or_else(|error| panic!("{error}"));
            problems.push(EngineError::because(ErrorCode::BadInput, reason).into());
        }
        for problem in problems {
            let said = super::problem(&problem, Audience::App);
            assert!(
                !said.is_empty() && !said.contains("{$") && !said.contains("{ $"),
                "{problem:?}: {said}"
            );
            assert!(text(&problem.info.message.id, None, &[]).is_some(), "{problem:?}");
        }
    }

    /// Every value a code can take, from its own schema.
    fn names<T: JsonSchema>() -> Vec<String> {
        let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default();
        let listed = schema["enum"]
            .as_array()
            .cloned()
            .or_else(|| {
                // A code with an unlisted `Unknown` lists its values in one `oneOf` entry.
                schema["oneOf"].as_array().map(|variants| {
                    variants
                        .iter()
                        .flat_map(|variant| match variant["enum"].as_array() {
                            Some(values) => values.clone(),
                            None => vec![variant["const"].clone()],
                        })
                        .collect()
                })
            })
            .unwrap_or_default();
        listed
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect()
    }

    fn engine_ids() -> BTreeSet<String> {
        let mut ids = BTreeSet::new();
        for (group, names) in [
            ("error", names::<ErrorCode>()),
            ("reason", names::<ErrorReason>()),
            ("warning", names::<CitationWarningCode>()),
            ("source-warning", names::<ibis_sources::WarningCode>()),
            ("loss", names::<LossField>()),
            ("notice", names::<Notice>()),
            ("form", names::<CitationForm>()),
            ("cause", names::<ibis_sources::WarningCause>()),
        ] {
            assert!(!names.is_empty(), "{group} lists its codes");
            ids.extend(names.into_iter().map(|name| format!("{group}-{name}")));
        }
        ids.extend(
            CitationError::ALL
                .into_iter()
                .map(|error| error.message_id().to_owned()),
        );
        ids
    }

    /// Each message's id, from the lines that start one.
    fn catalogue_ids() -> Vec<String> {
        assert!(
            FluentResource::try_new(SOURCE.to_owned()).is_ok(),
            "the catalogue parses"
        );
        SOURCE
            .lines()
            .filter_map(|line| line.split_once(" =").map(|(id, _)| id))
            .filter(|id| id.chars().next().is_some_and(|first| first.is_ascii_lowercase()))
            .filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_engine_code_has_words_and_no_engine_message_is_unused() {
        let catalogue = catalogue_ids();
        let unique: BTreeSet<_> = catalogue.iter().cloned().collect();
        assert_eq!(unique.len(), catalogue.len(), "a message id is repeated");
        let engine: BTreeSet<_> = unique
            .iter()
            .filter(|id| GROUPS.iter().any(|group| id.starts_with(&format!("{group}-"))))
            .cloned()
            .collect();
        assert_eq!(
            engine,
            engine_ids(),
            "the catalogue's engine messages and the engine's codes differ"
        );
        for id in &engine {
            assert!(
                text(id, None, &[]).is_some_and(|text| !text.is_empty()),
                "{id} has a sentence"
            );
        }
    }

    #[test]
    fn every_language_parses_and_says_every_engine_code() {
        for language in Language::ALL {
            assert!(
                FluentResource::try_new(language.source().to_owned()).is_ok(),
                "{} parses",
                language.tag()
            );
            for id in engine_ids() {
                assert!(
                    text_in(language, &id, None, &[]).is_some_and(|text| !text.is_empty()),
                    "{id} has a sentence in {}",
                    language.tag()
                );
            }
        }
    }

    #[test]
    fn the_system_language_picks_the_first_one_ibis_speaks() {
        assert_eq!(Language::negotiate(["fr-FR", "de-AT", "en-GB"]), Language::German);
        assert_eq!(Language::negotiate(["ar_EG.UTF-8"]), Language::Arabic);
        assert_eq!(Language::negotiate(["en-GB", "ar"]), Language::English);
        assert_eq!(Language::negotiate(["fr", "C", ""]), Language::English);
    }
}
