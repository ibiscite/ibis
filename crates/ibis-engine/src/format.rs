//! Which format a citation uses, decided once for every front end
//! (decision 0042): the writing chosen for this app, then this app's format,
//! then your format for this kind of app, then a suggestion for the kind.
//!
//! Your format is two values: a key syntax for LaTeX, Markdown, Obsidian and
//! Typst apps, code editors and apps Ibis does not list, and a style for
//! documents (word processors, mail, browsers). Key formats come first: the
//! people Ibis serves first write in LaTeX, Markdown or Obsidian (maintainer,
//! 2026-11-16). A value of the other kind never applies, so a style chosen in
//! Word never reaches a LaTeX file. The answer says which
//! level won, for the picker's header ("MLA · for Word", "Thesis").
use crate::outputs;
use crate::settings::Settings;
use ibis_core::contract::{KeySyntax, OutputSpec};
use ibis_core::destination::{AppId, OutputClass, lookup};
pub use ibis_core::destination::{FormatChoice, FormatFrom, FormatKind, WritingName};

/// The kind of `app`, what is suggested for it when nothing is chosen, and
/// whether Ibis knows its kind. An app Ibis does not list, or none, takes keys
/// (the engine's default output); a listed app of unknown kind (Notepad, a
/// terminal, an AI app) counts as documents.
fn kind(app: Option<&AppId>) -> (FormatKind, &'static str, bool) {
    match app.and_then(lookup).map(|entry| entry.output) {
        Some(Some(OutputClass::Key(syntax))) => (FormatKind::Keys, key_id(syntax), true),
        Some(Some(OutputClass::Formatted)) => (FormatKind::Documents, outputs::DEFAULT_STYLE, true),
        Some(None) => (FormatKind::Documents, outputs::DEFAULT_STYLE, false),
        None => (FormatKind::Keys, outputs::DEFAULT, false),
    }
}

fn key_id(syntax: KeySyntax) -> &'static str {
    let output = OutputSpec::Key { syntax, command: None };
    outputs::named(&output).map_or(outputs::DEFAULT, |entry| entry.id.as_str())
}

/// A listed output of the right kind, by id; anything else reads as no choice.
fn listed(id: Option<&str>, kind: FormatKind) -> Option<&'static outputs::OutputInfo> {
    outputs::find(id?).filter(|entry| kind_of(&entry.output) == kind)
}

fn choice(entry: &outputs::OutputInfo, from: FormatFrom, kind: FormatKind) -> FormatChoice {
    FormatChoice {
        output: entry.output.clone(),
        id: Some(entry.id.clone()),
        from,
        kind,
        writing: None,
    }
}

/// The format for a citation into `app` (none when no app was captured).
pub fn choose(settings: &Settings, app: Option<&AppId>) -> FormatChoice {
    let (kind, _, _) = kind(app);
    if let Some(writing) = app.and_then(|app| writing_for(settings, app)) {
        let name = WritingName {
            id: writing.name.clone(),
            name: writing.name.clone(),
        };
        return for_writing(&writing.format, name, app);
    }
    // This app's format was chosen in it, so it holds whatever its kind.
    if let Some(entry) = app.and_then(|app| settings.app_output(app)).and_then(outputs::find) {
        return choice(entry, FormatFrom::App, kind);
    }
    usual(settings, app)
}

/// Your format for this kind, or its suggestion, without an app override.
pub fn usual(settings: &Settings, app: Option<&AppId>) -> FormatChoice {
    let (kind, suggestion, _) = kind(app);
    usual_of(settings, kind, suggestion)
}

/// Your style for documents, or its suggestion: what Settings shows for them.
pub fn documents(settings: &Settings) -> FormatChoice {
    usual_of(settings, FormatKind::Documents, outputs::DEFAULT_STYLE)
}

fn usual_of(settings: &Settings, kind: FormatKind, suggestion: &str) -> FormatChoice {
    let yours = match kind {
        FormatKind::Documents => settings.portable.output.as_deref(),
        FormatKind::Keys => settings.portable.key_output.as_deref(),
    };
    if let Some(entry) = listed(yours, kind) {
        return choice(entry, FormatFrom::Yours, kind);
    }
    let entry = outputs::find(suggestion).unwrap_or_else(|| outputs::or_default(None));
    choice(entry, FormatFrom::Suggestion, kind)
}

/// The answer when a writing is chosen, also one that holds only for this
/// session: its format, in any kind of app, since the person chose it there.
pub fn for_writing(output: &OutputSpec, writing: WritingName, app: Option<&AppId>) -> FormatChoice {
    FormatChoice {
        id: outputs::named(output).map(|entry| entry.id.clone()),
        output: output.clone(),
        from: FormatFrom::Writing,
        kind: kind(app).0,
        writing: Some(writing),
    }
}

/// As [`choose`], with `yours` standing in for your format: what a request
/// names (the CLI's `--format` or `--style`, MCP's `format` or `style`). A
/// writing or the app's own format still wins, as in the app. For an app of
/// a known kind it must be of that kind, as your format is; an app Ibis does
/// not know takes it whatever its kind, so nothing named is lost there.
pub fn choose_with(settings: &Settings, app: Option<&AppId>, yours: Option<&OutputSpec>) -> FormatChoice {
    let chosen = choose(settings, app);
    let known = kind(app).2;
    match yours {
        Some(output)
            if matches!(chosen.from, FormatFrom::Yours | FormatFrom::Suggestion)
                && (!known || kind_of(output) == chosen.kind) =>
        {
            FormatChoice {
                id: outputs::named(output).map(|entry| entry.id.clone()),
                output: output.clone(),
                from: FormatFrom::Yours,
                kind: kind_of(output),
                writing: None,
            }
        }
        _ => chosen,
    }
}

/// Projects stay out of the way until they can help: someone who saved a
/// format of its own for this app, other than the usual one, may write more
/// than one kind of document there. A saved project is always offered.
pub fn offers_projects(settings: &Settings, app: Option<&AppId>) -> bool {
    !settings.device.writings.is_empty()
        || app.and_then(|app| settings.app_output(app)).is_some_and(|saved| {
            usual(settings, app).id.as_deref() != Some(saved)
        })
}

fn kind_of(output: &OutputSpec) -> FormatKind {
    if matches!(output, OutputSpec::Key { .. }) {
        FormatKind::Keys
    } else {
        FormatKind::Documents
    }
}

fn writing_for<'s>(settings: &'s Settings, app: &AppId) -> Option<&'s crate::settings::writings::Writing> {
    let id = settings.device.writing_apps.get(&crate::app_context::app_key(app))?;
    settings.device.writings.iter().find(|writing| &writing.name == id)
}

#[cfg(test)]
mod tests;

/// Confirm the exact suggested format after successful paste. A newer choice
/// makes this an inert stale completion. The return value asks for the one-time
/// explanation; saving the returned settings makes it durable across launches.
pub fn accept_suggestion(settings: &mut Settings, app: Option<&AppId>, captured: &FormatChoice) -> bool {
    if captured.from != FormatFrom::Suggestion || choose(settings, app) != *captured {
        return false;
    }
    let Some(id) = captured.id.as_ref() else {
        return false;
    };
    if let Some(app) = app {
        settings.remember_app_output(app, id.clone());
    }
    if settings.device.suggestion_reports.contains(&captured.kind) {
        return false;
    }
    settings.device.suggestion_reports.push(captured.kind);
    true
}
