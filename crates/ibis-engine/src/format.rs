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
//!
//! In a browser the website in front stands in for the app when Ibis could
//! read it (decision 0031): its own format, then what the destination table
//! says it is, so Overleaf gets LaTeX keys while Google Docs keeps your
//! style. A site Ibis does not list falls back to the browser. Projects stay
//! with the app: an address never selects one.
use crate::outputs;
use crate::settings::Settings;
use ibis_core::contract::{KeySyntax, OutputSpec};
use ibis_core::destination::{AppId, OutputClass, System, is_browser, lookup};
pub use ibis_core::destination::{FormatChoice, FormatFrom, FormatKind, WritingName};

/// The kind of `app`, what is suggested for it when nothing is chosen, and
/// whether Ibis knows its kind. An app Ibis does not list takes keys (the
/// engine's default output). A listed app of unknown kind (Notepad, a
/// terminal, an AI app) counts as documents, and so does no app at all: with
/// nothing captured (Linux, or a picker opened from Ibis) the text is copied
/// for any app.
fn kind(app: Option<&AppId>) -> (FormatKind, &'static str, bool) {
    let Some(app) = app else {
        return (FormatKind::Documents, outputs::DEFAULT_STYLE, false);
    };
    match lookup(app).map(|entry| entry.output) {
        Some(Some(OutputClass::Key(syntax))) => (FormatKind::Keys, key_id(syntax), true),
        Some(Some(OutputClass::Formatted)) => (FormatKind::Documents, outputs::DEFAULT_STYLE, true),
        Some(None) => (FormatKind::Documents, outputs::DEFAULT_STYLE, false),
        // A website Ibis does not list is a page in a browser.
        None if app.system == System::Web => (FormatKind::Documents, outputs::DEFAULT_STYLE, false),
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
    if let Some(chosen) = writing_choice(settings, app, app) {
        return chosen;
    }
    // This app's format was chosen in it, so it holds whatever its kind.
    if let Some(entry) = app.and_then(|app| settings.app_output(app)).and_then(outputs::find) {
        return choice(entry, FormatFrom::App, kind);
    }
    usual(settings, app)
}

/// The project chosen for `app`, with `kind_of`'s kind.
fn writing_choice(settings: &Settings, app: Option<&AppId>, kind_of: Option<&AppId>) -> Option<FormatChoice> {
    let writing = writing_for(settings, app?)?;
    let name = WritingName {
        id: writing.name.clone(),
        name: writing.name.clone(),
    };
    Some(for_writing(&writing.format, name, kind_of))
}

/// The website that chooses the format: `site`, when it is one and `app` is
/// a browser. Anything else leaves the app to choose.
fn site_for<'a>(app: Option<&AppId>, site: Option<&'a AppId>) -> Option<&'a AppId> {
    site.filter(|site| site.system == System::Web && !site.normalized().is_empty() && app.is_some_and(is_browser))
}

/// Whose kind applies: a listed site's, or else the app's.
fn kind_owner<'a>(app: Option<&'a AppId>, site: Option<&'a AppId>) -> Option<&'a AppId> {
    site_for(app, site).filter(|site| lookup(site).is_some()).or(app)
}

/// As [`choose`], for `app` with the website in front in it, if any: the
/// project chosen for the app, then the site's own format, then what the
/// table says the site is; a site Ibis does not list goes on with the app.
pub fn choose_at(settings: &Settings, app: Option<&AppId>, site: Option<&AppId>) -> FormatChoice {
    let Some(at) = site_for(app, site) else {
        return choose(settings, app);
    };
    let owner = kind_owner(app, site);
    let mut chosen = if let Some(chosen) = writing_choice(settings, app, owner) {
        chosen
    } else if let Some(entry) = settings.app_output(at).and_then(outputs::find) {
        choice(entry, FormatFrom::App, kind(owner).0)
    } else if lookup(at).is_some() {
        usual(settings, Some(at))
    } else {
        choose(settings, app)
    };
    chosen.kind = kind(owner).0;
    chosen
}

/// As [`usual`], for `app` with the website in front in it.
pub fn usual_at(settings: &Settings, app: Option<&AppId>, site: Option<&AppId>) -> FormatChoice {
    usual(settings, kind_owner(app, site))
}

/// The app or website whose remembered format [`choose_at`] uses, if one does.
pub fn remembered_for<'a>(settings: &Settings, app: Option<&'a AppId>, site: Option<&'a AppId>) -> Option<&'a AppId> {
    let saved = |id: &&AppId| settings.app_output(id).and_then(outputs::find).is_some();
    match site_for(app, site) {
        Some(site) if saved(&site) => Some(site),
        Some(site) if lookup(site).is_some() => None,
        _ => app.filter(saved),
    }
}

/// Where a format the person chooses now is remembered: the website, when
/// one is in front in a browser, or else the app.
pub fn remembers_for(app: Option<&AppId>, site: Option<&AppId>) -> Option<AppId> {
    site_for(app, site).or(app).cloned()
}

/// Forget the remembered format [`choose_at`] uses, whichever of the website
/// or the app it belongs to.
pub fn forget_at(settings: &mut Settings, app: Option<&AppId>, site: Option<&AppId>) {
    if let Some(id) = remembered_for(settings, app, site).cloned() {
        settings.forget_app_output(&id);
    }
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
    offers_projects_at(settings, app, None)
}

/// As [`offers_projects`], for `app` with the website in front in it.
pub fn offers_projects_at(settings: &Settings, app: Option<&AppId>, site: Option<&AppId>) -> bool {
    !settings.device.writings.is_empty()
        || remembered_for(settings, app, site)
            .and_then(|id| settings.app_output(id))
            .and_then(outputs::find)
            .is_some_and(|saved| usual_at(settings, app, site).id.as_deref() != Some(saved.id.as_str()))
}

fn kind_of(output: &OutputSpec) -> FormatKind {
    if matches!(output, OutputSpec::Key { .. }) {
        FormatKind::Keys
    } else {
        FormatKind::Documents
    }
}

/// A website never selects a project, however it was named (docs/design/delivery.md).
fn writing_for<'s>(settings: &'s Settings, app: &AppId) -> Option<&'s crate::settings::writings::Writing> {
    if app.system == System::Web {
        return None;
    }
    let id = settings.device.writing_apps.get(&crate::app_context::app_key(app))?;
    settings.device.writings.iter().find(|writing| &writing.name == id)
}

#[cfg(test)]
mod tests;

/// Confirm the exact suggested format after successful paste, remembering it
/// for the site when present or else the app. A newer choice
/// makes this an inert stale completion. The return value asks for the one-time
/// explanation; saving the returned settings makes it durable across launches.
pub fn accept_suggestion(
    settings: &mut Settings,
    app: Option<&AppId>,
    site: Option<&AppId>,
    captured: &FormatChoice,
) -> bool {
    if captured.from != FormatFrom::Suggestion || choose_at(settings, app, site) != *captured {
        return false;
    }
    let Some(id) = captured.id.as_ref() else {
        return false;
    };
    if let Some(owner) = remembers_for(app, site) {
        settings.remember_app_output(&owner, id.clone());
    }
    if settings.device.suggestion_reports.contains(&captured.kind) {
        return false;
    }
    settings.device.suggestion_reports.push(captured.kind);
    true
}
