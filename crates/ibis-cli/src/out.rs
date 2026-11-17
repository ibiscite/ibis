//! Terminal presentation for search and libraries. Plain streams never use it.
use icu_properties::{
    CodePointMapData, CodePointSetData,
    props::{DefaultIgnorableCodePoint, EastAsianWidth, Emoji, EmojiPresentation, GeneralCategory, HangulSyllableType},
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub(super) struct Out {
    pub width: Option<usize>,
    pub color: bool,
}

impl Out {
    pub fn new(mode: super::OutputMode) -> Self {
        let terminal = mode.stdout_terminal();
        let columns = std::env::var("COLUMNS").ok().and_then(|s| s.parse().ok());
        Self {
            width: terminal.then(|| native_width().or(columns).filter(|w| *w > 0).unwrap_or(80)),
            color: colors(
                terminal,
                std::env::var_os("NO_COLOR").is_some(),
                std::env::var("TERM").ok().as_deref(),
            ),
        }
    }

    pub fn configured(mode: super::OutputMode, matches: &clap::ArgMatches) -> Self {
        let mut out = Self::new(mode);
        match matches.get_one::<String>("color").map(String::as_str) {
            Some("never") => out.color = false,
            Some("always") => {
                out.color = true;
                // Explicit colour asks for human presentation even through a pipe.
                out.width = out.width.or(Some(80));
            }
            _ => {}
        }
        if out.width.is_some()
            && let Some(width) = matches.get_one::<u16>("width")
        {
            out.width = Some(usize::from(*width));
        }
        out
    }

    pub fn bold(self, text: &str) -> String {
        self.paint(text, 1)
    }

    pub fn dim(self, text: &str) -> String {
        self.paint(text, 2)
    }

    fn paint(self, text: &str, code: u8) -> String {
        if self.color && !text.is_empty() {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.into()
        }
    }
}

fn colors(terminal: bool, no_color: bool, term: Option<&str>) -> bool {
    terminal && !no_color && term != Some("dumb")
}

fn native_width() -> Option<usize> {
    console::Term::stdout()
        .size_checked()
        .map(|(_, columns)| usize::from(columns))
        .filter(|w| *w > 0)
}

// ICU's Unicode tables are already part of the engine build. Measure complete
// graphemes: combining accents occupy no extra cell, CJK and emoji occupy two.
fn cells(grapheme: &str) -> usize {
    let east = CodePointMapData::<EastAsianWidth>::new();
    let category = CodePointMapData::<GeneralCategory>::new();
    let ignored = CodePointSetData::new::<DefaultIgnorableCodePoint>();
    let emoji = CodePointSetData::new::<EmojiPresentation>();
    let emoji_base = CodePointSetData::new::<Emoji>();
    let hangul = CodePointMapData::<HangulSyllableType>::new();
    if (grapheme.contains('\u{fe0f}') && grapheme.chars().any(|c| emoji_base.contains(c)))
        || grapheme.chars().any(|c| emoji.contains(c))
    {
        return 2;
    }
    grapheme
        .chars()
        .map(|c| {
            if ignored.contains(c)
                || matches!(
                    hangul.get(c),
                    HangulSyllableType::VowelJamo | HangulSyllableType::TrailingJamo
                )
                || matches!(
                    category.get(c),
                    GeneralCategory::NonspacingMark
                        | GeneralCategory::EnclosingMark
                        | GeneralCategory::SpacingMark
                        | GeneralCategory::Control
                )
            {
                0
            } else if matches!(east.get(c), EastAsianWidth::Wide | EastAsianWidth::Fullwidth) {
                2
            } else {
                1
            }
        })
        .sum()
}

pub(super) fn width(text: &str) -> usize {
    text.graphemes(true).map(cells).sum()
}

pub(super) fn truncate(text: &str, columns: usize) -> String {
    if width(text) <= columns {
        return text.into();
    }
    if columns == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for grapheme in text.graphemes(true) {
        let next = cells(grapheme);
        if used + next >= columns {
            break;
        }
        result.push_str(grapheme);
        used += next;
    }
    result.push('…');
    result
}
