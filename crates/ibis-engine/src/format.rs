//! Which format a citation uses, decided once for every front end
//! (decision 0042): the writing chosen for this app, then this app's format,
//! then your format for this kind of app, then a suggestion for the kind.
//!
//! Your format is two values: a style for documents (word processors, mail,
//! browsers and apps Ibis does not know) and a key syntax for Markdown, LaTeX
//! and Typst apps and code editors. A value of the other kind never applies,
//! so a style chosen in Word never reaches a LaTeX file. The answer says which
//! level won, for the picker's header ("MLA · for Word", "Thesis").
use crate::outputs;
use ibis_core::contract::{KeySyntax, OutputSpec};
use ibis_core::destination::{AppId, OutputClass, OutputQuestion, lookup};
pub use ibis_core::destination::{FormatChoice, FormatFrom, FormatKind, WritingName};
use ibis_core::settings::Settings;

/// The kind of `app`, what is suggested for it when nothing is chosen, and
/// whether Ibis knows its kind; an app it does not counts as documents.
fn kind(app: Option<&AppId>) -> (FormatKind, &'static str, bool) {
    let entry = app.and_then(lookup);
    match (
        entry.and_then(|entry| entry.output),
        entry.and_then(|entry| entry.output_question),
    ) {
        (Some(OutputClass::Key(syntax)), _) => (FormatKind::Keys, key_id(syntax), true),
        (Some(OutputClass::Formatted), _) | (None, Some(OutputQuestion::Browser)) => {
            (FormatKind::Documents, outputs::DEFAULT_STYLE, true)
        }
        (None, Some(OutputQuestion::Editor)) => (FormatKind::Keys, outputs::DEFAULT, true),
        (None, None) => (FormatKind::Documents, outputs::DEFAULT_STYLE, false),
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
    let (kind, suggestion, _) = kind(app);
    if let Some(project) = app.and_then(|app| writing_for(settings, app)) {
        let writing = WritingName {
            id: project.id.clone(),
            name: project.name.clone(),
        };
        return for_writing(&project.output, writing, app);
    }
    // This app's format was chosen in it, so it holds whatever its kind.
    if let Some(entry) = app.and_then(|app| settings.app_output(app)).and_then(outputs::find) {
        return choice(entry, FormatFrom::App, kind);
    }
    let yours = match kind {
        FormatKind::Documents => settings.output.as_deref(),
        FormatKind::Keys => settings.key_output.as_deref(),
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

fn kind_of(output: &OutputSpec) -> FormatKind {
    if matches!(output, OutputSpec::Key { .. }) {
        FormatKind::Keys
    } else {
        FormatKind::Documents
    }
}

fn writing_for<'s>(
    settings: &'s Settings,
    app: &AppId,
) -> Option<&'s ibis_core::settings::writing_projects::WritingProject> {
    let id = settings
        .writing_project_apps
        .get(&format!("{}:{}", system(app), app.normalized()))?;
    settings.writing_projects.iter().find(|project| &project.id == id)
}

fn system(app: &AppId) -> &'static str {
    match app.system {
        ibis_core::destination::System::Macos => "macos",
        ibis_core::destination::System::Windows => "windows",
        ibis_core::destination::System::Linux => "linux",
    }
}

#[cfg(test)]
mod tests;
