//! OpenDocument text: character data inside `text:p` and `text:h`, with
//! spans for formatting. A replacement is written in place, inside whatever
//! span holds its start, so it keeps that span's formatting; its own
//! formatting comes from automatic styles added for it.
use super::archive::Archive;
use super::xml::{Events, Frames, Plan, Segment, attribute, escape, reference};
use super::{Part, Refusal};
use ibis_core::contract::{DisplayRun, Script};
use quick_xml::Writer;
use quick_xml::events::Event;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

const BODY: &str = "content.xml";
/// The most spaces one `text:s` may stand for.
const MAX_SPACES: usize = 1024;

pub fn scan(archive: &mut Archive<'_>, frames: &mut Frames) -> Result<(), Refusal> {
    let body = archive.text(BODY)?.ok_or(Refusal::NotOffice)?;
    walk(&body, Part::Body, frames, None, None)?;
    if let Some(styles) = archive.text("styles.xml")? {
        walk(&styles, Part::Header, frames, None, None)?;
    }
    Ok(())
}

pub fn write(archive: &mut Archive<'_>, plan: &Plan<'_>) -> Result<Vec<u8>, Refusal> {
    let body = archive.text(BODY)?.ok_or(Refusal::NotOffice)?;
    // Names no style in the document starts with.
    let mut prefix = String::from("IbisFormat");
    while body.contains(&prefix) {
        prefix.push('X');
    }
    // The reference list's paragraphs follow the style of the paragraph they
    // replace; an automatic style cannot be a parent, so its own parent is.
    let found = walk(&body, Part::Body, &mut Frames::default(), None, plan.edits.bibliography.at)?;
    let parent = found
        .at_style
        .map(|style| found.automatic.get(&style).cloned().unwrap_or(Some(style)))
        .unwrap_or_default();
    let mut writer = Writer::new(Vec::with_capacity(body.len() + 4096));
    let mut out = Out {
        plan,
        writer: &mut writer,
        prefix,
        parent,
        inserted: 0,
    };
    walk(&body, Part::Body, &mut Frames::default(), Some(&mut out), plan.edits.bibliography.at)?;
    if out.inserted != plan.edits.citations.len() {
        return Err(Refusal::Mismatch);
    }
    let body = String::from_utf8(writer.into_inner()).map_err(|_| Refusal::Damaged)?;
    archive.rewrite(&[(BODY, body)])
}

struct Out<'p, 'w> {
    plan: &'p Plan<'p>,
    writer: &'w mut Writer<Vec<u8>>,
    prefix: String,
    parent: Option<String>,
    inserted: usize,
}

impl Out<'_, '_> {
    fn raw(&mut self, text: &str) -> Result<(), Refusal> {
        self.writer
            .get_mut()
            .write_all(text.as_bytes())
            .map_err(|_| Refusal::Damaged)
    }

    /// Every combination of formatting the replacements use.
    fn looks(&self) -> BTreeSet<Look> {
        let bibliography = &self.plan.edits.bibliography;
        self.plan
            .edits
            .citations
            .iter()
            .flat_map(|edit| &edit.runs)
            .chain(bibliography.entries.iter().flat_map(|entry| entry.margin.iter().chain(&entry.content)))
            .map(Look::of)
            .filter(|look| *look != Look::default())
            .collect()
    }

    fn styles(&mut self) -> Result<(), Refusal> {
        let mut xml = String::new();
        for look in self.looks() {
            let mut properties = String::new();
            if look.italic {
                properties.push_str(" fo:font-style=\"italic\" style:font-style-asian=\"italic\" style:font-style-complex=\"italic\"");
            }
            if look.bold {
                properties.push_str(" fo:font-weight=\"bold\" style:font-weight-asian=\"bold\" style:font-weight-complex=\"bold\"");
            }
            if look.small_caps {
                properties.push_str(" fo:font-variant=\"small-caps\"");
            }
            if look.underline {
                properties.push_str(" style:text-underline-style=\"solid\" style:text-underline-width=\"auto\" style:text-underline-color=\"font-color\"");
            }
            match look.script {
                1 => properties.push_str(" style:text-position=\"super 58%\""),
                2 => properties.push_str(" style:text-position=\"sub 58%\""),
                _ => {}
            }
            xml.push_str(&format!(
                "<style:style style:name=\"{}\" style:family=\"text\"><style:text-properties{properties}/></style:style>",
                look.name(&self.prefix)
            ));
        }
        let bibliography = &self.plan.edits.bibliography;
        let indent = bibliography.hanging_indent || bibliography.entries.iter().any(|entry| !entry.margin.is_empty());
        xml.push_str(&format!(
            "<style:style style:name=\"{}Entry\" style:family=\"paragraph\"{}>{}</style:style>",
            self.prefix,
            self.parent
                .as_deref()
                .map_or(String::new(), |parent| format!(" style:parent-style-name=\"{}\"", escape(parent))),
            if indent {
                "<style:paragraph-properties fo:margin-left=\"0.5in\" fo:text-indent=\"-0.5in\"/>"
            } else {
                ""
            }
        ));
        self.raw(&xml)
    }

    fn runs(&self, runs: &[DisplayRun]) -> String {
        runs.iter()
            .map(|run| {
                let look = Look::of(run);
                if look == Look::default() {
                    escape(&run.text).into_owned()
                } else {
                    format!(
                        "<text:span text:style-name=\"{}\">{}</text:span>",
                        look.name(&self.prefix),
                        escape(&run.text)
                    )
                }
            })
            .collect()
    }

    fn entries(&mut self) -> Result<(), Refusal> {
        let mut xml = String::new();
        for entry in &self.plan.edits.bibliography.entries {
            xml.push_str(&format!("<text:p text:style-name=\"{}Entry\">", self.prefix));
            if !entry.margin.is_empty() {
                xml.push_str(&self.runs(&entry.margin));
                xml.push_str("<text:tab/>");
            }
            xml.push_str(&self.runs(&entry.content));
            xml.push_str("</text:p>");
        }
        self.raw(&xml)
    }
}

/// One combination of the formatting a run can ask for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct Look {
    italic: bool,
    bold: bool,
    small_caps: bool,
    underline: bool,
    script: u8,
}

impl Look {
    fn of(run: &DisplayRun) -> Self {
        Self {
            italic: run.italic,
            bold: run.bold,
            small_caps: run.small_caps,
            underline: run.underline,
            script: match run.script {
                Script::Normal => 0,
                Script::Superscript => 1,
                Script::Subscript => 2,
            },
        }
    }
    fn name(self, prefix: &str) -> String {
        format!(
            "{prefix}{}{}{}{}{}",
            if self.italic { "I" } else { "" },
            if self.bold { "B" } else { "" },
            if self.small_caps { "C" } else { "" },
            if self.underline { "U" } else { "" },
            ["", "Sup", "Sub"][usize::from(self.script)]
        )
    }
}

/// Elements whose own text is not the paragraph's: shapes and their
/// descriptions, a note's number, comments and recorded changes.
fn opaque(name: &[u8]) -> bool {
    [b"draw:".as_slice(), b"svg:", b"dr3d:", b"chart:", b"office:annotation"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || matches!(name, b"text:note-citation" | b"text:tracked-changes")
}

#[derive(Default)]
struct Places {
    notes: usize,
    comments: usize,
    shapes: usize,
    hidden: usize,
}

impl Places {
    fn part(&self, part: Part) -> Part {
        if self.hidden > 0 {
            Part::Hidden
        } else if self.comments > 0 {
            Part::Comment
        } else if self.shapes > 0 {
            Part::TextBox
        } else if self.notes > 0 {
            Part::Note
        } else {
            part
        }
    }
    fn count(&mut self, name: &[u8], open: bool) {
        let counter = match name {
            b"text:note-body" => &mut self.notes,
            b"office:annotation" => &mut self.comments,
            b"text:tracked-changes" => &mut self.hidden,
            name if name.starts_with(b"draw:") => &mut self.shapes,
            _ => return,
        };
        *counter = if open { *counter + 1 } else { counter.saturating_sub(1) };
    }
}

/// Character data waiting to be written as one piece.
#[derive(Default)]
struct Pending {
    events: Vec<Event<'static>>,
    text: String,
}

/// What a first reading finds for the reference list's paragraph style.
#[derive(Default)]
struct Found {
    at_style: Option<String>,
    automatic: BTreeMap<String, Option<String>>,
}

fn walk(
    xml: &str,
    part: Part,
    frames: &mut Frames,
    mut out: Option<&mut Out<'_, '_>>,
    at: Option<usize>,
) -> Result<Found, Refusal> {
    let mut events = Events::new(xml);
    let mut places = Places::default();
    let mut paragraphs: Vec<bool> = Vec::new();
    let mut pending = Pending::default();
    let mut found = Found::default();
    let mut in_automatic = false;
    let mut styled = false;
    let mut swallow: Option<usize> = None;
    loop {
        let event = events.next()?;
        if let Event::Text(_) | Event::GeneralRef(_) = &event
            && frames.current().is_some()
        {
            match &event {
                Event::Text(chunk) => pending.text.push_str(&chunk.xml10_content()),
                Event::GeneralRef(entity) => pending.text.push(reference(entity)?),
                _ => {}
            }
            pending.events.push(event.into_owned());
            continue;
        }
        flush(&mut pending, frames, out.as_deref_mut(), swallow.is_some())?;
        if matches!(event, Event::Eof) {
            break;
        }
        let mut write = true;
        match &event {
            Event::Start(element) | Event::Empty(element) => {
                let name = element.name().into_inner().as_bytes().to_vec();
                let empty = matches!(event, Event::Empty(_));
                match name.as_slice() {
                    b"text:p" | b"text:h" => {
                        let index = frames.open(places.part(part))?;
                        if Some(index) == at {
                            found.at_style = attribute(element, b"text:style-name")?;
                            if out.is_some() {
                                swallow = Some(paragraphs.len());
                            }
                        }
                        if empty {
                            frames.close();
                            if swallow.is_some() {
                                swallow = None;
                                if let Some(out) = out.as_deref_mut() {
                                    out.entries()?;
                                }
                                continue;
                            }
                        } else {
                            paragraphs.push(true);
                        }
                    }
                    b"text:s" | b"text:tab" | b"text:line-break" if empty => {
                        if let Some(paragraph) = frames.current() {
                            let piece = match name.as_slice() {
                                b"text:s" => {
                                    let count = attribute(element, b"text:c")?
                                        .map_or(Ok(1), |count| count.parse::<usize>())
                                        .map_err(|_| Refusal::Damaged)?;
                                    if count > MAX_SPACES {
                                        return Err(Refusal::TooLarge);
                                    }
                                    " ".repeat(count)
                                }
                                b"text:tab" => "\t".into(),
                                _ => "\n".into(),
                            };
                            let at = frames.push(paragraph, &piece)?;
                            if let Some(out) = out.as_deref() {
                                write = !out.plan.removes(paragraph, at, piece.len())?;
                            }
                        }
                    }
                    b"office:automatic-styles" => {
                        if empty {
                            if let Some(out) = out.as_deref_mut() {
                                styled = true;
                                out.raw("<office:automatic-styles>")?;
                                out.styles()?;
                                out.raw("</office:automatic-styles>")?;
                                continue;
                            }
                        } else {
                            in_automatic = true;
                        }
                    }
                    b"style:style" if in_automatic => {
                        if let Some(style) = attribute(element, b"style:name")? {
                            found
                                .automatic
                                .insert(style, attribute(element, b"style:parent-style-name")?);
                        }
                    }
                    // A document without automatic styles gets them before its body.
                    b"office:body" => {
                        if let Some(out) = out.as_deref_mut()
                            && !styled
                        {
                            styled = true;
                            out.raw("<office:automatic-styles>")?;
                            out.styles()?;
                            out.raw("</office:automatic-styles>")?;
                        }
                    }
                    _ => {}
                }
                if !empty {
                    places.count(&name, true);
                    if opaque(&name) {
                        frames.opaque(true);
                    }
                }
            }
            Event::End(element) => {
                let name = element.name().into_inner().as_bytes();
                places.count(name, false);
                if opaque(name) {
                    frames.opaque(false);
                }
                match name {
                    b"text:p" | b"text:h" => {
                        frames.close();
                        paragraphs.pop();
                        if swallow == Some(paragraphs.len()) {
                            swallow = None;
                            if let Some(out) = out.as_deref_mut() {
                                out.entries()?;
                            }
                            continue;
                        }
                    }
                    b"office:automatic-styles" => {
                        in_automatic = false;
                        if let Some(out) = out.as_deref_mut() {
                            styled = true;
                            out.styles()?;
                        }
                    }
                    b"office:text" => {
                        if let Some(out) = out.as_deref_mut()
                            && at.is_none()
                        {
                            out.entries()?;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if swallow.is_some() || !write {
            continue;
        }
        if let Some(out) = out.as_deref_mut() {
            out.writer.write_event(event).map_err(|_| Refusal::Damaged)?;
        }
    }
    if out.is_some() && (swallow.is_some() || !styled) {
        return Err(Refusal::Damaged);
    }
    Ok(found)
}

/// Write waiting character data, with any replacement that starts in it.
fn flush(pending: &mut Pending, frames: &mut Frames, out: Option<&mut Out<'_, '_>>, swallowed: bool) -> Result<(), Refusal> {
    if pending.events.is_empty() {
        return Ok(());
    }
    let Pending { events, text } = std::mem::take(pending);
    let Some(paragraph) = frames.current() else {
        return Err(Refusal::Damaged);
    };
    let at = frames.push(paragraph, &text)?;
    let Some(out) = out else { return Ok(()) };
    if swallowed {
        return Ok(());
    }
    match out.plan.segments(paragraph, at, &text) {
        None => {
            for event in events {
                out.writer.write_event(event).map_err(|_| Refusal::Damaged)?;
            }
        }
        Some(segments) => {
            let mut xml = String::new();
            for segment in segments {
                match segment {
                    Segment::Text(kept) => xml.push_str(&escape(kept)),
                    Segment::Insert(index) => {
                        out.inserted += 1;
                        xml.push_str(&out.runs(&out.plan.edits.citations[index].runs));
                    }
                }
            }
            out.raw(&xml)?;
        }
    }
    Ok(())
}
