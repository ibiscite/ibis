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

/// The catalogue, as it is bundled into every build.
pub const SOURCE: &str = include_str!("../../../locales/en-US.ftl");

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
pub const GROUPS: [&str; 7] = ["error", "reason", "refusal", "warning", "loss", "notice", "form"];

fn bundle() -> &'static FluentBundle<FluentResource> {
    static BUNDLE: OnceLock<FluentBundle<FluentResource>> = OnceLock::new();
    BUNDLE.get_or_init(|| {
        let resource = FluentResource::try_new(SOURCE.to_owned()).unwrap_or_else(|(resource, _)| resource);
        let english: unic_langid::LanguageIdentifier = "en-US".parse().unwrap_or_default();
        let mut bundle = FluentBundle::new_concurrent(vec![english]);
        // Terminals, agents and accessible names read plain text: no
        // invisible direction marks around arguments.
        bundle.set_use_isolating(false);
        // A duplicate id keeps the first; the catalogue test refuses both.
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
    let bundle = bundle();
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
    let said = text(&id, None, args).unwrap_or_else(|| id.clone());
    match text(&id, Some(audience.attribute()), args) {
        Some(fix) => format!("{said} {fix}"),
        None => said,
    }
}

/// An engine error in words: its reason when it gives one, else its code.
pub fn error(error: &EngineError, audience: Audience) -> String {
    match error.reason {
        Some(reason) => sentence("reason", reason, audience, &[]),
        None => sentence("error", error.code, audience, &[]),
    }
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

    /// Every value a code can take, from its own schema.
    fn names<T: JsonSchema>() -> Vec<String> {
        let schema = serde_json::to_value(schemars::schema_for!(T)).unwrap_or_default();
        let listed = schema["enum"]
            .as_array()
            .cloned()
            .or_else(|| {
                schema["oneOf"]
                    .as_array()
                    .map(|variants| variants.iter().map(|variant| variant["const"].clone()).collect())
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
            ("refusal", names::<CitationError>()),
            ("warning", names::<CitationWarningCode>()),
            ("loss", names::<LossField>()),
            ("notice", names::<Notice>()),
            ("form", names::<CitationForm>()),
        ] {
            assert!(!names.is_empty(), "{group} lists its codes");
            ids.extend(names.into_iter().map(|name| format!("{group}-{name}")));
        }
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
    fn library_format_words_resolve_the_shared_term_for_every_front_end() {
        let formats = "BibTeX, BibLaTeX, CSL-JSON, RIS or CSV";
        for id in [
            "reason-unknown-format",
            "choice-library-file-help",
            "choice-drop-file",
            "operation-failures-drop-refused",
            "cli-no-references",
            "cli-file-help",
            "cli-file-unreadable",
            "app-file-dialog-formats",
        ] {
            let said = text(id, None, &[]).unwrap_or_default();
            assert!(said.contains(formats), "{id}: {said}");
        }
        for audience in [Audience::Cli, Audience::Agent, Audience::App] {
            assert!(
                text("reason-unknown-format", Some(audience.attribute()), &[])
                    .is_some_and(|fix| fix.contains("another file"))
            );
        }
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
    fn a_reason_says_more_than_its_code_and_each_audience_gets_its_own_fix() {
        let profile = EngineError::because(ErrorCode::SourceUnavailable, ErrorReason::ZoteroProfileNeeded);
        let cli = error(&profile, Audience::Cli);
        let agent = error(&profile, Audience::Agent);
        assert!(cli.contains("--zotero-profile"), "{cli}");
        assert!(
            agent.starts_with(cli.split_once(". ").map_or(&*cli, |(said, _)| said)),
            "{agent}"
        );
        assert_ne!(cli, agent);
        assert_eq!(code(ErrorCode::Cancelled, Audience::App), "The request was cancelled.");
    }
}
