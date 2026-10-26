//! Name particles and suffixes, split the way Zotero's CSL export splits
//! them (`Zotero.Utilities.Item.parseParticles`, which replicates
//! citeproc-js), so a name read from Zotero's database matches the one its
//! local API sends: "van Gogh" has the non-dropping particle "van", and
//! "Jean de" the dropping particle "de".

use serde_json::{Map, Value};

/// Separators that end a family-name particle: `d'`, `al-`, `van `.
const FAMILY_SEPARATORS: [char; 5] = ['-', '\u{02BB}', '\u{2019}', ' ', '\''];
/// Characters before a particle's first letter that do not decide its case.
const LEADING: [char; 4] = ['-', '\'', '\u{02BB}', '\u{2019}'];
/// Longer names are left as they are: splitting is quadratic in the length,
/// and no real name part comes near this.
const MAX_CHARS: usize = 512;

/// Split particles and a suffix out of a CSL name with both a family and a
/// given name, in place.
pub(super) fn parse(name: &mut Map<String, Value>) {
    let text = |name: &Map<String, Value>, key: &str| name.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let (family, particles) = split(&text(name, "family"), false);
    name.insert("family".into(), family.into());
    let particle = trim_last(&particles.concat());
    if !particle.is_empty() {
        name.insert("non-dropping-particle".into(), particle.into());
    }
    parse_suffix(name);
    let (given, particles) = split(&text(name, "given"), true);
    name.insert("given".into(), given.into());
    let particle = particles.concat().trim().to_string();
    if !particle.is_empty() {
        name.insert("dropping-particle".into(), particle.into());
    }
}

/// Leading particles of a family name, or trailing ones of a given name.
fn split(value: &str, given: bool) -> (String, Vec<String>) {
    if value.chars().count() > MAX_CHARS {
        return (value.to_string(), Vec::new());
    }
    let mut rest: Vec<char> = value.chars().collect();
    let mut scan = rest.clone();
    if given {
        scan.reverse();
    }
    let mut particles = Vec::new();
    while let Some(length) = particle_match(&scan, given) {
        let mut matched: Vec<char> = scan[..length].to_vec();
        if given {
            matched.reverse();
        }
        let first = matched
            .iter()
            .find(|c| !(LEADING.contains(c) || c.is_whitespace()))
            .copied();
        if !first.is_some_and(|c| c.to_uppercase().ne(std::iter::once(c))) {
            break;
        }
        let particle: String = if given {
            rest.split_off(rest.len() - length).into_iter().collect()
        } else {
            let tail = rest.split_off(length);
            std::mem::replace(&mut rest, tail).into_iter().collect()
        };
        particles.push(particle);
        scan.drain(..length);
    }
    if given {
        particles.reverse();
        for i in 1..particles.len() {
            if particles[i].starts_with(' ') {
                particles[i - 1].push(' ');
            }
        }
        for particle in &mut particles {
            if let Some(stripped) = particle.strip_prefix(' ') {
                *particle = stripped.to_string();
            }
        }
    }
    (rest.into_iter().collect(), particles)
}

/// The length of the particle at the start of `s` with the separator and
/// spaces after it, as `^([^ ]+(?:-|ʻ|’| |') *)(.+)$` matches it for a family
/// name, or `^([^ ]+(?:ʻ |’ | |' ) *)(.+)$` for a reversed given name.
fn particle_match(s: &[char], given: bool) -> Option<usize> {
    let line_end = |c: &char| matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}');
    let token = s.iter().position(|&c| c == ' ').unwrap_or(s.len());
    for end in (1..=token).rev() {
        let separator = match s.get(end) {
            Some(' ') => 1,
            Some(c) if !given && FAMILY_SEPARATORS.contains(c) => 1,
            Some(c) if given && LEADING[1..].contains(c) && s.get(end + 1) == Some(&' ') => 2,
            _ => continue,
        };
        let after = end + separator;
        let mut spaces = after;
        while s.get(spaces) == Some(&' ') {
            spaces += 1;
        }
        // `.+$` needs one more character and no line break to the end; the
        // spaces give one back if nothing else follows.
        if s[spaces..].iter().any(line_end) {
            continue;
        }
        if spaces < s.len() {
            return Some(spaces);
        }
        if spaces > after {
            return Some(spaces - 1);
        }
    }
    None
}

fn trim_last(text: &str) -> String {
    let trimmed = text.trim();
    let mut out = trimmed.to_string();
    if text.ends_with(' ') && (trimmed.ends_with('\'') || trimmed.ends_with('\u{2019}')) {
        out.push(' ');
    }
    out
}

/// A suffix after a comma in the given name ("John, Jr."), or "et al."
/// written into it.
fn parse_suffix(name: &mut Map<String, Value>) {
    if name.contains_key("suffix") {
        return;
    }
    let Some(given) = name.get("given").and_then(Value::as_str).filter(|g| !g.is_empty()) else {
        return;
    };
    let Some(comma) = given.find(',') else {
        return;
    };
    let start = given[..comma].trim_end().len();
    let mut end = comma + 1;
    end += given[end..].len() - given[end..].trim_start_matches('!').len();
    let marks = end - comma;
    end += given[end..].len() - given[end..].trim_start().len();
    let suffix = given[end..].to_string();
    let rest = given[..start].to_string();
    if suffix.replace('.', "") == "et al" && !name.contains_key("dropping-particle") {
        name.insert("dropping-particle".into(), suffix.into());
        name.insert("comma-dropping-particle".into(), ",".into());
    } else {
        if marks == 2 {
            name.insert("comma-suffix".into(), true.into());
        }
        name.insert("suffix".into(), suffix.into());
    }
    name.insert("given".into(), rest.into());
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn parsed(family: &str, given: &str) -> Value {
        let mut name = Map::new();
        name.insert("family".into(), family.into());
        name.insert("given".into(), given.into());
        parse(&mut name);
        Value::Object(name)
    }

    #[test]
    fn particles_are_split_as_zotero_splits_them() {
        assert_eq!(
            parsed("van Gogh", "Vincent"),
            json!({ "family": "Gogh", "given": "Vincent", "non-dropping-particle": "van" })
        );
        assert_eq!(
            parsed("de la Fontaine", "Jean"),
            json!({ "family": "Fontaine", "given": "Jean", "non-dropping-particle": "de la" })
        );
        assert_eq!(
            parsed("d'Alembert", "Jean"),
            json!({ "family": "Alembert", "given": "Jean", "non-dropping-particle": "d'" })
        );
        assert_eq!(
            parsed("al-Khwarizmi", "Muhammad"),
            json!({ "family": "Khwarizmi", "given": "Muhammad", "non-dropping-particle": "al-" })
        );
        assert_eq!(
            parsed("Humboldt", "Alexander von"),
            json!({ "family": "Humboldt", "given": "Alexander", "dropping-particle": "von" })
        );
        // Capitalised words, other scripts and single words are no particles.
        assert_eq!(
            parsed("Van Gogh", "Vincent"),
            json!({ "family": "Van Gogh", "given": "Vincent" })
        );
        assert_eq!(
            parsed("山田 太郎", "花子"),
            json!({ "family": "山田 太郎", "given": "花子" })
        );
        assert_eq!(
            parsed("García", "Lucía"),
            json!({ "family": "García", "given": "Lucía" })
        );
    }

    #[test]
    fn suffixes_and_et_al_come_out_of_the_given_name() {
        assert_eq!(
            parsed("King", "Martin Luther, Jr."),
            json!({ "family": "King", "given": "Martin Luther", "suffix": "Jr." })
        );
        assert_eq!(
            parsed("Doe", "John ,! III"),
            json!({ "family": "Doe", "given": "John", "suffix": "III", "comma-suffix": true })
        );
        assert_eq!(
            parsed("Doe", "Jane, et al."),
            json!({ "family": "Doe", "given": "Jane", "dropping-particle": "et al.", "comma-dropping-particle": "," })
        );
    }

    #[test]
    fn odd_names_end_without_a_crash() {
        for (family, given) in [
            (" ", " "),
            ("- -", "' '"),
            ("a", "b"),
            ("van ", "x "),
            ("van\nGogh", "x\ny"),
            ("ß ß ß", "ß ß ß"),
            ("de ", ","),
        ] {
            let name = parsed(family, given);
            assert!(name.get("family").is_some(), "{family:?}");
        }
        let long = "a ".repeat(100_000);
        let started = std::time::Instant::now();
        for (family, given) in [
            (long.as_str(), "x"),
            ("x", long.as_str()),
            (&"b".repeat(200_000), "y z"),
        ] {
            assert_eq!(parsed(family, given)["family"], family);
        }
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
    }
}
