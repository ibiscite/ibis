//! WordprocessingML: text lives in `w:t` inside runs (`w:r`), split wherever
//! the formatting or an edit history changes. A replacement splits the run
//! it starts in and takes that run's properties.
use super::archive::Archive;
use super::xml::{Capture, Child, Events, Frames, Plan, Segment, attribute, escape, ordered, reference};
use super::{Part, Refusal};
use ibis_core::contract::{DisplayRun, Script};
use quick_xml::Writer;
use quick_xml::events::Event;
use std::io::Write;

const BODY: &str = "word/document.xml";

/// `CT_RPr`'s sequence, which Word reads strictly.
const RUN_ORDER: &[&str] = &[
    "w:rStyle",
    "w:rFonts",
    "w:b",
    "w:bCs",
    "w:i",
    "w:iCs",
    "w:caps",
    "w:smallCaps",
    "w:strike",
    "w:dstrike",
    "w:outline",
    "w:shadow",
    "w:emboss",
    "w:imprint",
    "w:noProof",
    "w:snapToGrid",
    "w:vanish",
    "w:webHidden",
    "w:color",
    "w:spacing",
    "w:w",
    "w:kern",
    "w:position",
    "w:sz",
    "w:szCs",
    "w:highlight",
    "w:u",
    "w:effect",
    "w:bdr",
    "w:shd",
    "w:fitText",
    "w:vertAlign",
    "w:rtl",
    "w:cs",
    "w:em",
    "w:lang",
    "w:eastAsianLayout",
    "w:specVanish",
    "w:oMath",
];
/// `CT_PPr`'s sequence.
const PARAGRAPH_ORDER: &[&str] = &[
    "w:pStyle",
    "w:keepNext",
    "w:keepLines",
    "w:pageBreakBefore",
    "w:framePr",
    "w:widowControl",
    "w:numPr",
    "w:suppressLineNumbers",
    "w:pBdr",
    "w:shd",
    "w:tabs",
    "w:suppressAutoHyphens",
    "w:kinsoku",
    "w:wordWrap",
    "w:overflowPunct",
    "w:topLinePunct",
    "w:autoSpaceDE",
    "w:autoSpaceDN",
    "w:bidi",
    "w:adjustRightInd",
    "w:snapToGrid",
    "w:spacing",
    "w:ind",
    "w:contextualSpacing",
    "w:mirrorIndents",
    "w:suppressOverlap",
    "w:jc",
    "w:textDirection",
    "w:textAlignment",
    "w:textboxTightWrap",
    "w:outlineLvl",
    "w:divId",
    "w:cnfStyle",
    "w:rPr",
];

/// The body first, then notes, comments, headers and footers.
pub fn scan(archive: &mut Archive<'_>, frames: &mut Frames) -> Result<(), Refusal> {
    let body = archive.text(BODY)?.ok_or(Refusal::Unsupported)?;
    walk(&body, Part::Body, frames, None)?;
    let others: Vec<(String, Part)> = archive
        .names()
        .filter_map(|name| {
            let file = name.strip_prefix("word/")?.strip_suffix(".xml")?;
            let part = match file.trim_end_matches(|c: char| c.is_ascii_digit()) {
                "footnotes" | "endnotes" => Part::Note,
                "comments" => Part::Comment,
                "header" | "footer" => Part::Header,
                _ => return None,
            };
            Some((name.to_owned(), part))
        })
        .collect();
    for (name, part) in others {
        if let Some(xml) = archive.text(&name)? {
            walk(&xml, part, frames, None)?;
        }
    }
    Ok(())
}

pub fn write(archive: &mut Archive<'_>, plan: &Plan<'_>) -> Result<Vec<u8>, Refusal> {
    let body = archive.text(BODY)?.ok_or(Refusal::Unsupported)?;
    let mut frames = Frames::default();
    let mut writer = Writer::new(Vec::with_capacity(body.len() + 4096));
    let inserted = walk(&body, Part::Body, &mut frames, Some((plan, &mut writer)))?;
    if inserted != plan.edits.citations.len() {
        return Err(Refusal::Mismatch);
    }
    let body = String::from_utf8(writer.into_inner()).map_err(|_| Refusal::Damaged)?;
    archive.rewrite(&[(BODY, body)])
}

/// The paragraph being replaced by the reference list.
struct Swallow {
    depth: usize,
    paragraph: Vec<Child>,
    run: Option<Vec<Child>>,
}

/// Read one part's paragraphs into `frames`; with a plan, write the part
/// with it applied. Returns how many replacements were written.
fn walk(
    xml: &str,
    part: Part,
    frames: &mut Frames,
    mut out: Option<(&Plan<'_>, &mut Writer<Vec<u8>>)>,
) -> Result<usize, Refusal> {
    let plan = out.as_ref().map(|(plan, _)| *plan);
    let mut events = Events::new(xml);
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut boxes = 0_usize;
    let mut run: Vec<Child> = Vec::new();
    let mut capture: Option<(Capture, bool)> = None;
    let mut swallow: Option<Swallow> = None;
    let mut appended = false;
    let mut inserted = 0;
    loop {
        let event = events.next()?;
        if matches!(event, Event::Eof) {
            break;
        }
        let parent = stack.last().map(Vec::as_slice);
        // Properties of the current run, or of the paragraph being replaced.
        if let Some((open, paragraph)) = &mut capture
            && !open.take(&event)?
        {
            let children = std::mem::take(&mut open.children);
            match (swallow.as_mut(), *paragraph) {
                (Some(swallow), true) => swallow.paragraph = children,
                (Some(swallow), false) => {
                    if swallow.run.is_none() {
                        swallow.run = Some(children.clone());
                    }
                    run = children;
                }
                (None, _) => run = children,
            }
            capture = None;
        }
        let mut write = true;
        // The reference list goes before the body's own section properties.
        let append = |name: &[u8], parent: Option<&[u8]>| {
            (name == b"w:sectPr" && parent == Some(b"w:body".as_slice())) || name == b"w:body"
        };
        match &event {
            Event::Start(element) => {
                let name = element.name().into_inner().as_bytes().to_vec();
                match name.as_slice() {
                    b"w:p" => {
                        let index = frames.open(if boxes > 0 { Part::TextBox } else { part })?;
                        if plan.is_some_and(|plan| plan.edits.bibliography.at == Some(index)) {
                            swallow = Some(Swallow {
                                depth: stack.len(),
                                paragraph: Vec::new(),
                                run: None,
                            });
                        }
                    }
                    b"w:txbxContent" => boxes += 1,
                    // Word shows the choice; the fallback repeats it for older readers.
                    b"mc:Fallback" => frames.opaque(true),
                    b"w:r" => run.clear(),
                    b"w:rPr" if parent == Some(b"w:r") => capture = Some((Capture::default(), false)),
                    b"w:pPr"
                        if parent == Some(b"w:p") && swallow.as_ref().is_some_and(|s| stack.len() == s.depth + 1) =>
                    {
                        capture = Some((Capture::default(), true));
                    }
                    b"w:t" if parent == Some(b"w:r") => {
                        let preserve = attribute(element, b"xml:space")?.as_deref() == Some("preserve");
                        let mut original = vec![event.clone().into_owned()];
                        let mut text = String::new();
                        loop {
                            let inner = events.next()?;
                            match &inner {
                                Event::Text(chunk) => text.push_str(&chunk.xml10_content()),
                                Event::GeneralRef(entity) => text.push(reference(entity)?),
                                Event::CData(data) => {
                                    text.push_str(&data[..]);
                                }
                                Event::End(_) => {
                                    original.push(inner.into_owned());
                                    break;
                                }
                                _ => return Err(Refusal::Damaged),
                            }
                            original.push(inner.into_owned());
                        }
                        // Word drops the spaces around text it was not told to keep.
                        let text = if preserve { text } else { text.trim().to_owned() };
                        let segments = match (frames.current(), plan) {
                            (Some(paragraph), plan) => {
                                let at = frames.push(paragraph, &text)?;
                                plan.and_then(|plan| plan.segments(paragraph, at, &text))
                            }
                            (None, _) => None,
                        };
                        let (Some(plan), Some((_, writer))) = (plan, &mut out) else {
                            continue;
                        };
                        if swallow.is_some() {
                            continue;
                        }
                        let Some(segments) = segments else {
                            for event in original {
                                writer.write_event(event).map_err(|_| Refusal::Damaged)?;
                            }
                            continue;
                        };
                        let mut text = String::from("<w:t xml:space=\"preserve\">");
                        for segment in segments {
                            match segment {
                                Segment::Text(kept) => text.push_str(&escape(kept)),
                                Segment::Insert(index) => {
                                    inserted += 1;
                                    text.push_str("</w:t></w:r>");
                                    for piece in &plan.edits.citations[index].runs {
                                        text.push_str(&run_xml(&run, piece));
                                    }
                                    text.push_str("<w:r>");
                                    text.push_str(&properties(&run, None));
                                    text.push_str("<w:t xml:space=\"preserve\">");
                                }
                            }
                        }
                        text.push_str("</w:t>");
                        writer
                            .get_mut()
                            .write_all(text.as_bytes())
                            .map_err(|_| Refusal::Damaged)?;
                        continue;
                    }
                    _ => {}
                }
                if let (Some(plan), Some((_, writer))) = (plan, &mut out)
                    && name.as_slice() == b"w:sectPr"
                    && append(&name, parent)
                    && plan.edits.bibliography.at.is_none()
                    && !appended
                {
                    appended = true;
                    entries(writer, plan, &[], &[])?;
                }
                stack.push(name);
            }
            Event::Empty(element) => match element.name().into_inner().as_bytes() {
                b"w:tab" | b"w:br" | b"w:cr" if parent == Some(b"w:r") => {
                    if let Some(paragraph) = frames.current() {
                        let at = frames.push(
                            paragraph,
                            if element.name().into_inner().as_bytes() == b"w:tab" {
                                "\t"
                            } else {
                                "\n"
                            },
                        )?;
                        if let Some(plan) = plan {
                            write = !plan.removes(paragraph, at, 1)?;
                        }
                    }
                }
                b"w:p" => {
                    frames.open(if boxes > 0 { Part::TextBox } else { part })?;
                    frames.close();
                }
                name => {
                    if let (Some(plan), Some((_, writer))) = (plan, &mut out)
                        && append(name, parent)
                        && plan.edits.bibliography.at.is_none()
                        && !appended
                    {
                        appended = true;
                        entries(writer, plan, &[], &[])?;
                    }
                }
            },
            Event::End(element) => {
                stack.pop();
                match element.name().into_inner().as_bytes() {
                    b"w:p" => {
                        frames.close();
                        if let Some(replaced) = swallow.take_if(|s| s.depth == stack.len()) {
                            if let (Some(plan), Some((_, writer))) = (plan, &mut out) {
                                entries(
                                    writer,
                                    plan,
                                    &replaced.paragraph,
                                    replaced.run.as_deref().unwrap_or(&[]),
                                )?;
                            }
                            continue;
                        }
                    }
                    b"w:txbxContent" => boxes = boxes.saturating_sub(1),
                    b"mc:Fallback" => frames.opaque(false),
                    b"w:body" => {
                        if let (Some(plan), Some((_, writer))) = (plan, &mut out)
                            && plan.edits.bibliography.at.is_none()
                            && !appended
                        {
                            appended = true;
                            entries(writer, plan, &[], &[])?;
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
        if let Some((_, writer)) = &mut out {
            writer.write_event(event).map_err(|_| Refusal::Damaged)?;
        }
    }
    if plan.is_some()
        && (swallow.is_some() || !appended && plan.is_some_and(|plan| plan.edits.bibliography.at.is_none()))
    {
        return Err(Refusal::Damaged);
    }
    Ok(inserted)
}

/// `w:rPr` from `base` with `run`'s formatting added.
fn properties(base: &[Child], run: Option<&DisplayRun>) -> String {
    let mut children: Vec<Child> = base.to_vec();
    if let Some(run) = run {
        let mut set = |name: &str, xml: String| {
            children.retain(|child| child.name != name.as_bytes());
            children.push(Child {
                name: name.as_bytes().to_vec(),
                xml,
            });
        };
        if run.italic {
            set("w:i", "<w:i/>".into());
            set("w:iCs", "<w:iCs/>".into());
        }
        if run.bold {
            set("w:b", "<w:b/>".into());
            set("w:bCs", "<w:bCs/>".into());
        }
        if run.small_caps {
            set("w:smallCaps", "<w:smallCaps/>".into());
        }
        if run.underline {
            set("w:u", "<w:u w:val=\"single\"/>".into());
        }
        match run.script {
            Script::Superscript => set("w:vertAlign", "<w:vertAlign w:val=\"superscript\"/>".into()),
            Script::Subscript => set("w:vertAlign", "<w:vertAlign w:val=\"subscript\"/>".into()),
            Script::Normal => {}
        }
    }
    if children.is_empty() {
        return String::new();
    }
    format!("<w:rPr>{}</w:rPr>", ordered(children, RUN_ORDER, "w:rPrChange"))
}

fn run_xml(base: &[Child], run: &DisplayRun) -> String {
    format!(
        "<w:r>{}<w:t xml:space=\"preserve\">{}</w:t></w:r>",
        properties(base, Some(run)),
        escape(&run.text)
    )
}

/// The reference list as paragraphs, in the replaced paragraph's properties
/// (`paragraph`) and its first run's (`run`).
fn entries(writer: &mut Writer<Vec<u8>>, plan: &Plan<'_>, paragraph: &[Child], run: &[Child]) -> Result<(), Refusal> {
    let bibliography = &plan.edits.bibliography;
    let mut xml = String::new();
    for entry in &bibliography.entries {
        // A copied paragraph never repeats a section break, list number or revision.
        let mut children: Vec<Child> = paragraph
            .iter()
            .filter(|child| {
                !matches!(child.name.as_slice(), b"w:sectPr" | b"w:pPrChange" | b"w:numPr")
                    && (child.name != b"w:ind" || !(bibliography.hanging_indent || !entry.margin.is_empty()))
            })
            .cloned()
            .collect();
        if bibliography.hanging_indent || !entry.margin.is_empty() {
            children.push(Child {
                name: b"w:ind".to_vec(),
                xml: "<w:ind w:left=\"720\" w:hanging=\"720\"/>".into(),
            });
        }
        xml.push_str("<w:p>");
        if !children.is_empty() {
            xml.push_str(&format!(
                "<w:pPr>{}</w:pPr>",
                ordered(children, PARAGRAPH_ORDER, "w:pPrChange")
            ));
        }
        for piece in &entry.margin {
            xml.push_str(&run_xml(run, piece));
        }
        if !entry.margin.is_empty() {
            xml.push_str(&format!("<w:r>{}<w:tab/></w:r>", properties(run, None)));
        }
        for piece in &entry.content {
            xml.push_str(&run_xml(run, piece));
        }
        xml.push_str("</w:p>");
    }
    writer.get_mut().write_all(xml.as_bytes()).map_err(|_| Refusal::Damaged)
}
