//! One bounded search alias per all-kana name; never a source-data conversion.
use unicode_normalization::UnicodeNormalization;

use crate::model::Name;

/// Orthographic Hepburn: explicit vowels, ー repeats a vowel, n' before a
/// vowel/y. No inferred Han readings, alternative spellings or dictionaries.
/// A name with unsupported characters or over 256 NFKC scalars has no alias.
pub(super) fn alias(name: &Name) -> Option<String> {
    let parts: Vec<&str> = if let Some(literal) = &name.literal {
        vec![literal]
    } else {
        // Keep family/given boundaries even when Name::display joins CJK names.
        [
            &name.non_dropping_particle,
            &name.family,
            &name.given,
            &name.dropping_particle,
            &name.suffix,
        ]
        .into_iter()
        .flatten()
        .map(String::as_str)
        .collect()
    };
    let chars: Vec<char> = parts
        .iter()
        .enumerate()
        .flat_map(|(i, part)| std::iter::once(' ').take(usize::from(i > 0)).chain(part.chars()))
        .nfkc()
        .map(|c| match c {
            '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .take(257)
        .collect();
    if chars.len() > 256 {
        return None;
    }
    let words = chars
        .split(|c| c.is_whitespace() || *c == '・')
        .filter(|word| !word.is_empty())
        .map(word)
        .collect::<Option<Vec<_>>>()?;
    let alias = super::normalize(&words.join(" "));
    (!alias.is_empty()).then_some(alias)
}

fn word(chars: &[char]) -> Option<String> {
    // U+3041..U+3096, including empty entries for unsupported small kana.
    const KANA: [&str; 86] = [
        "", "a", "", "i", "", "u", "", "e", "", "o", "ka", "ga", "ki", "gi", "ku", "gu", "ke", "ge", "ko", "go", "sa",
        "za", "shi", "ji", "su", "zu", "se", "ze", "so", "zo", "ta", "da", "chi", "ji", "", "tsu", "zu", "te", "de",
        "to", "do", "na", "ni", "nu", "ne", "no", "ha", "ba", "pa", "hi", "bi", "pi", "fu", "bu", "pu", "he", "be",
        "pe", "ho", "bo", "po", "ma", "mi", "mu", "me", "mo", "", "ya", "", "yu", "", "yo", "ra", "ri", "ru", "re",
        "ro", "", "wa", "wi", "we", "wo", "n", "vu", "", "",
    ];
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let doubled = chars[i] == 'っ';
        i += usize::from(doubled);
        let c = *chars.get(i)?;
        if c == 'ー' && !doubled {
            let vowel = out
                .chars()
                .last()
                .filter(|c| matches!(c, 'a' | 'i' | 'u' | 'e' | 'o'))?;
            out.push(vowel);
            i += 1;
            continue;
        }
        let mut syllable = *KANA.get((c as usize).checked_sub('ぁ' as usize)?)?;
        if syllable.is_empty() {
            return None;
        }
        let contracted = match chars.get(i + 1) {
            Some('ゃ') => Some('a'),
            Some('ゅ') => Some('u'),
            Some('ょ') => Some('o'),
            _ => None,
        };
        if contracted.is_some() {
            syllable = match syllable {
                "ki" => "ky",
                "gi" => "gy",
                "shi" => "sh",
                "ji" => "j",
                "chi" => "ch",
                "ni" => "ny",
                "hi" => "hy",
                "bi" => "by",
                "pi" => "py",
                "mi" => "my",
                "ri" => "ry",
                _ => return None,
            };
        }
        if doubled {
            out.push(match syllable.as_bytes()[0] {
                b'k' => 'k',
                b's' => 's',
                b't' | b'c' => 't',
                b'p' => 'p',
                _ => return None,
            });
        }
        out.push_str(syllable);
        if let Some(vowel) = contracted {
            out.push(vowel);
            i += 1;
        } else if c == 'ん'
            && matches!(
                chars.get(i + 1),
                Some('あ' | 'い' | 'う' | 'え' | 'お' | 'や' | 'ゆ' | 'よ')
            )
        {
            out.push('\'');
        }
        i += 1;
    }
    Some(out)
}
