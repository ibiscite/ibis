//! What Word and OpenDocument share: bounded XML events, paragraphs in
//! document order, and the plan that maps replacements onto text pieces.
use super::{Edit, Edits, MAX_PARAGRAPHS, MAX_TEXT_BYTES, Paragraph, Part, Refusal};
use quick_xml::Reader;
use quick_xml::events::{BytesRef, BytesStart, Event};
use std::collections::BTreeMap;

/// The deepest element nesting read.
const MAX_DEPTH: usize = 256;

/// XML events within the bounds: no document type declaration, so no
/// entity can be declared or fetched, and a bounded depth.
pub struct Events<'a> {
    reader: Reader<&'a [u8]>,
    ancestors: Vec<BytesStart<'a>>,
}

impl<'a> Events<'a> {
    pub fn new(xml: &'a str) -> Self {
        let mut reader = Reader::from_str(xml);
        reader.config_mut().check_end_names = true;
        Self {
            reader,
            ancestors: Vec::new(),
        }
    }

    pub fn next(&mut self) -> Result<Event<'a>, Refusal> {
        let event = self.reader.read_event().map_err(|_| Refusal::Damaged)?;
        match &event {
            Event::DocType(_) => return Err(Refusal::Unsafe),
            Event::Start(element) => {
                if self.ancestors.len() >= MAX_DEPTH {
                    return Err(Refusal::TooLarge);
                }
                self.ancestors.push(element.clone());
            }
            Event::End(_) => {
                self.ancestors.pop();
            }
            Event::Eof if !self.ancestors.is_empty() => return Err(Refusal::Damaged),
            _ => {}
        }
        Ok(event)
    }

    /// Capture properties that will leave their current ancestors. Bindings
    /// declared below `retained_depth` must travel with each moved child;
    /// bindings above it still surround the replacement at its destination.
    pub fn capture(&self, retained_depth: usize) -> Result<Capture, Refusal> {
        let mut namespaces = BTreeMap::new();
        for ancestor in self.ancestors.iter().skip(retained_depth) {
            for attribute in ancestor.attributes() {
                let attribute = attribute.map_err(|_| Refusal::Damaged)?;
                let name = attribute.key.into_inner();
                if name == "xmlns" || name.starts_with("xmlns:") {
                    namespaces.insert(name.to_owned(), attribute.value.into_owned());
                }
            }
        }
        Ok(Capture {
            namespaces,
            ..Capture::default()
        })
    }
}

/// The character an entity reference stands for: only XML's own five and
/// character references; any other name is an undeclared entity.
pub fn reference(reference: &BytesRef<'_>) -> Result<char, Refusal> {
    if reference.is_char_ref() {
        return reference.resolve_char_ref().ok().flatten().ok_or(Refusal::Damaged);
    }
    match reference.as_bytes() {
        b"amp" => Ok('&'),
        b"lt" => Ok('<'),
        b"gt" => Ok('>'),
        b"quot" => Ok('"'),
        b"apos" => Ok('\''),
        _ => Err(Refusal::Damaged),
    }
}

/// The value of attribute `name` on an element, unescaped.
pub fn attribute(element: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Result<Option<String>, Refusal> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|_| Refusal::Damaged)?;
        if attribute.key.into_inner().as_bytes() == name {
            return attribute
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .map(|value| Some(value.into_owned()))
                .map_err(|_| Refusal::Damaged);
        }
    }
    Ok(None)
}

pub fn escape(text: &str) -> std::borrow::Cow<'_, str> {
    quick_xml::escape::escape(text)
}

/// Paragraphs numbered as they open, so a paragraph inside another (a text
/// box or note) comes after the text before it. Text counts for the
/// innermost open paragraph unless an opaque element (a shape's description,
/// a note's number) lies between.
#[derive(Default)]
pub struct Frames {
    pub paragraphs: Vec<Paragraph>,
    stack: Vec<(usize, usize)>,
    text: usize,
}

impl Frames {
    pub fn open(&mut self, part: Part) -> Result<usize, Refusal> {
        if self.paragraphs.len() >= MAX_PARAGRAPHS {
            return Err(Refusal::TooLarge);
        }
        self.paragraphs.push(Paragraph {
            part,
            text: String::new(),
        });
        self.stack.push((self.paragraphs.len() - 1, 0));
        Ok(self.paragraphs.len() - 1)
    }
    pub fn close(&mut self) {
        self.stack.pop();
    }
    pub fn opaque(&mut self, open: bool) {
        if let Some((_, opaque)) = self.stack.last_mut() {
            *opaque = if open { *opaque + 1 } else { opaque.saturating_sub(1) };
        }
    }
    /// The paragraph text now counts for, if any.
    pub fn current(&self) -> Option<usize> {
        self.stack
            .last()
            .filter(|(_, opaque)| *opaque == 0)
            .map(|(index, _)| *index)
    }
    /// Add `text` to paragraph `index`; where it starts in that paragraph.
    pub fn push(&mut self, index: usize, text: &str) -> Result<usize, Refusal> {
        self.text += text.len();
        if self.text > MAX_TEXT_BYTES {
            return Err(Refusal::TooLarge);
        }
        let paragraph = &mut self.paragraphs[index].text;
        let at = paragraph.len();
        paragraph.push_str(text);
        Ok(at)
    }
}

/// A piece of a paragraph's text as written: kept text, and the places
/// where an edit's runs go.
#[derive(Debug, PartialEq, Eq)]
pub enum Segment<'a> {
    Text(&'a str),
    Insert(usize),
}

/// The checked replacements, by paragraph.
pub struct Plan<'e> {
    pub edits: &'e Edits,
    /// Each paragraph's edits, as a range of `edits.citations`.
    by_paragraph: std::collections::BTreeMap<usize, std::ops::Range<usize>>,
}

impl<'e> Plan<'e> {
    /// Refuse replacements that do not fit `paragraphs`: out of order,
    /// overlapping, empty, off a character boundary or outside the body.
    pub fn new(paragraphs: &[Paragraph], edits: &'e Edits) -> Result<Self, Refusal> {
        let mut by_paragraph = std::collections::BTreeMap::<usize, std::ops::Range<usize>>::new();
        let mut last: Option<(usize, usize)> = None;
        for (index, edit) in edits.citations.iter().enumerate() {
            let text = &paragraphs.get(edit.paragraph).ok_or(Refusal::Mismatch)?;
            let fits = text.part == Part::Body
                && edit.start < edit.end
                && edit.end <= text.text.len()
                && text.text.is_char_boundary(edit.start)
                && text.text.is_char_boundary(edit.end)
                && last.is_none_or(|last| last <= (edit.paragraph, edit.start))
                && !edit.runs.is_empty();
            if !fits {
                return Err(Refusal::Mismatch);
            }
            last = Some((edit.paragraph, edit.end));
            by_paragraph.entry(edit.paragraph).or_insert(index..index).end = index + 1;
        }
        if let Some(at) = edits.bibliography.at {
            let place = paragraphs.get(at).ok_or(Refusal::Mismatch)?;
            if place.part != Part::Body || place.text.trim() != "[bibliography]" || by_paragraph.contains_key(&at) {
                return Err(Refusal::Mismatch);
            }
        }
        Ok(Self { edits, by_paragraph })
    }

    fn of(&self, paragraph: usize) -> &'e [Edit] {
        self.by_paragraph
            .get(&paragraph)
            .map_or(&[], |range| &self.edits.citations[range.clone()])
    }

    /// The index in `edits.citations` of the first edit of `paragraph`.
    fn first(&self, paragraph: usize) -> usize {
        self.by_paragraph.get(&paragraph).map_or(0, |range| range.start)
    }

    /// How text `text`, at byte `at` of paragraph `paragraph`, is written;
    /// `None` when no edit touches it.
    pub fn segments<'t>(&self, paragraph: usize, at: usize, text: &'t str) -> Option<Vec<Segment<'t>>> {
        let end = at + text.len();
        let edits = self.of(paragraph);
        if !edits.iter().any(|edit| edit.start < end && edit.end > at) {
            return None;
        }
        let first = self.first(paragraph);
        let mut segments = Vec::new();
        let mut cursor = at;
        for (offset, edit) in edits.iter().enumerate() {
            if edit.end <= at || edit.start >= end {
                continue;
            }
            if edit.start >= cursor {
                segments.push(Segment::Text(&text[cursor - at..edit.start - at]));
                segments.push(Segment::Insert(first + offset));
            }
            cursor = cursor.max(edit.end.min(end));
        }
        segments.push(Segment::Text(&text[cursor - at..]));
        segments.retain(|segment| *segment != Segment::Text(""));
        Some(segments)
    }

    /// Whether a piece that cannot be split, `len` bytes at `at`, lies inside
    /// an edit. One an edit starts on cannot carry its runs.
    pub fn removes(&self, paragraph: usize, at: usize, len: usize) -> Result<bool, Refusal> {
        for edit in self.of(paragraph) {
            if edit.start == at {
                return Err(Refusal::Mismatch);
            }
            if edit.start < at && at + len <= edit.end {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// One child element of a property list (`w:rPr`, `w:pPr`), as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    pub name: Vec<u8>,
    pub xml: String,
}

/// Collects the child elements of one element while the events pass.
#[derive(Default)]
pub struct Capture {
    pub children: Vec<Child>,
    namespaces: BTreeMap<String, String>,
    depth: usize,
    current: Option<(Vec<u8>, quick_xml::Writer<Vec<u8>>)>,
}

impl Capture {
    /// Record `event`, inside the captured element. `false` once it has closed.
    pub fn take(&mut self, event: &Event<'_>) -> Result<bool, Refusal> {
        let mut event = event.clone();
        if self.depth == 0
            && let Event::Start(element) | Event::Empty(element) = &mut event
        {
            for (name, value) in &self.namespaces {
                // A declaration on the child itself overrides its ancestors.
                if !element
                    .attributes()
                    .any(|attribute| attribute.is_ok_and(|attribute| attribute.key.into_inner() == name))
                {
                    element.push_attribute((name.as_str(), value.as_str()));
                }
            }
        }
        match &event {
            Event::End(_) if self.depth == 0 => return Ok(false),
            Event::Start(element) if self.depth == 0 => {
                self.current = Some((
                    element.name().into_inner().as_bytes().to_vec(),
                    quick_xml::Writer::new(Vec::new()),
                ));
            }
            Event::Empty(element) if self.depth == 0 => {
                let mut writer = quick_xml::Writer::new(Vec::new());
                writer.write_event(event.borrow()).map_err(|_| Refusal::Damaged)?;
                self.children.push(Child {
                    name: element.name().into_inner().as_bytes().to_vec(),
                    xml: String::from_utf8(writer.into_inner()).map_err(|_| Refusal::Damaged)?,
                });
                return Ok(true);
            }
            _ => {}
        }
        if let Some((_, writer)) = &mut self.current {
            writer.write_event(event.borrow()).map_err(|_| Refusal::Damaged)?;
        }
        match &event {
            Event::Start(_) => self.depth += 1,
            Event::End(_) => {
                self.depth -= 1;
                if self.depth == 0
                    && let Some((name, writer)) = self.current.take()
                {
                    self.children.push(Child {
                        name,
                        xml: String::from_utf8(writer.into_inner()).map_err(|_| Refusal::Damaged)?,
                    });
                }
            }
            _ => {}
        }
        Ok(true)
    }
}

/// `children` in the order a schema's sequence lists them; names it does not
/// list keep their place after the listed ones, and `last` goes last.
pub fn ordered(mut children: Vec<Child>, order: &[&str], last: &str) -> String {
    let rank = |child: &Child| {
        order
            .iter()
            .position(|name| name.as_bytes() == child.name)
            .unwrap_or(if child.name == last.as_bytes() {
                order.len() + 1
            } else {
                order.len()
            })
    };
    children.sort_by_key(rank);
    children.into_iter().map(|child| child.xml).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::contract::DisplayRun;

    fn edit(paragraph: usize, start: usize, end: usize) -> Edit {
        Edit {
            paragraph,
            start,
            end,
            runs: vec![DisplayRun {
                text: "X".into(),
                ..DisplayRun::default()
            }],
        }
    }

    #[test]
    fn a_replacement_split_across_pieces_is_written_once_at_its_start() {
        let paragraphs = vec![Paragraph {
            part: Part::Body,
            text: "See [@vale2024] now".into(),
        }];
        let edits = Edits {
            citations: vec![edit(0, 4, 15)],
            bibliography: super::super::Bibliography {
                at: None,
                entries: vec![],
                hanging_indent: false,
            },
        };
        let plan = Plan::new(&paragraphs, &edits).unwrap();
        // "See [@va" | "le20" | "24] now"
        assert_eq!(
            plan.segments(0, 0, "See [@va"),
            Some(vec![Segment::Text("See "), Segment::Insert(0)])
        );
        assert_eq!(plan.segments(0, 8, "le20"), Some(vec![]));
        assert_eq!(plan.segments(0, 12, "24] now"), Some(vec![Segment::Text(" now")]));
        assert_eq!(plan.removes(0, 9, 1), Ok(true));
        assert_eq!(plan.removes(0, 4, 1), Err(Refusal::Mismatch));

        let mut overlapping = edits.clone();
        overlapping.citations.push(edit(0, 10, 12));
        assert!(Plan::new(&paragraphs, &overlapping).is_err());
    }
}
