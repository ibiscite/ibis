//! A small lossless JSON/JSONC editor. Only owned values and their separators
//! change; other tokens, whitespace and comments stay in their original order.
use serde_json::{Value, json};
use std::{collections::BTreeSet, ops::Range};

#[derive(Debug)]
struct Node {
    range: Range<usize>,
    value: Value,
    children: Vec<(String, usize, Node, Option<usize>)>,
    array: Vec<(Node, Option<usize>)>,
}

struct Parser<'a> {
    text: &'a str,
    at: usize,
    comments: bool,
}

impl Parser<'_> {
    fn skip(&mut self) -> Result<(), String> {
        loop {
            while self.text.as_bytes().get(self.at).is_some_and(u8::is_ascii_whitespace) {
                self.at += 1;
            }
            if self.comments && self.text[self.at..].starts_with("//") {
                self.at += self.text[self.at..].find('\n').unwrap_or(self.text.len() - self.at);
            } else if self.comments && self.text[self.at..].starts_with("/*") {
                self.at += self.text[self.at + 2..].find("*/").ok_or("unclosed comment")? + 4;
            } else {
                return Ok(());
            }
        }
    }
    fn take(&mut self, byte: u8) -> Result<(), String> {
        self.skip()?;
        if self.text.as_bytes().get(self.at) != Some(&byte) {
            return Err("invalid JSON".into());
        }
        self.at += 1;
        Ok(())
    }
    fn string(&mut self) -> Result<String, String> {
        self.skip()?;
        let start = self.at;
        self.take(b'"')?;
        loop {
            match self.text.as_bytes().get(self.at) {
                Some(b'"') => {
                    self.at += 1;
                    break;
                }
                Some(b'\\') => self.at += 2,
                Some(_) => self.at += 1,
                None => return Err("unclosed string".into()),
            }
        }
        serde_json::from_str(&self.text[start..self.at]).map_err(|_| "invalid JSON string".into())
    }
    fn node(&mut self, depth: usize) -> Result<Node, String> {
        if depth > 64 {
            return Err("JSON is nested too deeply".into());
        }
        self.skip()?;
        let start = self.at;
        let mut children = vec![];
        let mut array = vec![];
        let value = match self.text.as_bytes().get(self.at) {
            Some(b'{') => {
                self.at += 1;
                let mut value = serde_json::Map::new();
                let mut names = BTreeSet::new();
                self.skip()?;
                while self.text.as_bytes().get(self.at) != Some(&b'}') {
                    let key_start = self.at;
                    let key = self.string()?;
                    if !names.insert(key.clone()) {
                        return Err(format!("duplicate JSON key {key:?}"));
                    }
                    self.take(b':')?;
                    let node = self.node(depth + 1)?;
                    value.insert(key.clone(), node.value.clone());
                    self.skip()?;
                    let comma = if self.text.as_bytes().get(self.at) == Some(&b',') {
                        let comma = self.at;
                        self.at += 1;
                        self.skip()?;
                        if !self.comments && self.text.as_bytes().get(self.at) == Some(&b'}') {
                            return Err("trailing comma".into());
                        }
                        Some(comma)
                    } else {
                        None
                    };
                    children.push((key, key_start, node, comma));
                    if comma.is_none() {
                        break;
                    }
                }
                self.take(b'}')?;
                Value::Object(value)
            }
            Some(b'[') => {
                self.at += 1;
                self.skip()?;
                while self.text.as_bytes().get(self.at) != Some(&b']') {
                    let node = self.node(depth + 1)?;
                    self.skip()?;
                    let comma = if self.text.as_bytes().get(self.at) == Some(&b',') {
                        let comma = self.at;
                        self.at += 1;
                        self.skip()?;
                        if !self.comments && self.text.as_bytes().get(self.at) == Some(&b']') {
                            return Err("trailing comma".into());
                        }
                        Some(comma)
                    } else {
                        None
                    };
                    array.push((node, comma));
                    if comma.is_none() {
                        break;
                    }
                }
                self.take(b']')?;
                Value::Array(array.iter().map(|(n, _)| n.value.clone()).collect())
            }
            Some(b'"') => Value::String(self.string()?),
            Some(_) => {
                while self
                    .text
                    .as_bytes()
                    .get(self.at)
                    .is_some_and(|b| !b.is_ascii_whitespace() && !b",]} /".contains(b))
                {
                    self.at += 1;
                }
                serde_json::from_str(&self.text[start..self.at]).map_err(|_| "invalid JSON value")?
            }
            None => return Err("incomplete JSON".into()),
        };
        Ok(Node {
            range: start..self.at,
            value,
            children,
            array,
        })
    }
}

fn parse(text: &str, comments: bool) -> Result<Node, String> {
    let mut parser = Parser { text, at: 0, comments };
    let node = parser.node(0)?;
    parser.skip()?;
    if parser.at != text.len() || !node.value.is_object() {
        return Err("expected one JSON object".into());
    }
    Ok(node)
}

fn child<'a>(node: &'a Node, key: &str) -> Result<Option<&'a Node>, String> {
    if let Some(name) = key.strip_prefix('@') {
        if !node.value.is_array() {
            return Err("plugins must be an array".into());
        }
        let found: Vec<_> = node.array.iter().filter(|(n, _)| n.value["name"] == name).collect();
        if found.len() > 1 {
            return Err(format!("more than one {name} plugin entry"));
        }
        Ok(found.first().map(|(n, _)| n))
    } else {
        if !node.value.is_object() {
            return Err(format!("{key} must be inside an object"));
        }
        Ok(node.children.iter().find(|(k, _, _, _)| k == key).map(|(_, _, n, _)| n))
    }
}

pub(super) fn get(text: &str, path: &[String], comments: bool) -> Result<Option<Value>, String> {
    let root = parse(text, comments)?;
    let mut node = &root;
    for key in path {
        let Some(next) = child(node, key)? else {
            return Ok(None);
        };
        node = next;
    }
    Ok(Some(node.value.clone()))
}

/// Missing containers belong to Ibis only while they remain empty on removal.
pub(super) fn missing(text: &str, path: &[String], comments: bool) -> Result<Vec<Vec<String>>, String> {
    let mut out = vec![];
    for len in 1..path.len() {
        if get(text, &path[..len], comments)?.is_none() {
            out.push(path[..len].to_vec());
        }
    }
    Ok(out)
}

pub(super) fn removable_empty(text: &str, path: &[String], comments: bool) -> Result<bool, String> {
    let root = parse(text, comments)?;
    let mut node = &root;
    for key in path {
        let Some(next) = child(node, key)? else {
            return Ok(false);
        };
        node = next;
    }
    Ok(
        (node.value.as_object().is_some_and(|o| o.is_empty()) || node.value.as_array().is_some_and(|a| a.is_empty()))
            && text[node.range.start + 1..node.range.end - 1].trim().is_empty(),
    )
}

pub(super) fn set(text: &str, path: &[String], value: Option<&Value>, comments: bool) -> Result<String, String> {
    fn edit(text: &str, node: &Node, path: &[String], value: Option<&Value>) -> Result<String, String> {
        let key = path.first().ok_or("empty entry path")?;
        let existing = child(node, key)?;
        if path.len() > 1 {
            if let Some(next) = existing {
                return edit(text, next, &path[1..], value);
            }
            if value.is_none() {
                return Ok(text.into());
            }
            let mut nested = value.cloned().ok_or("missing entry")?;
            for part in path[1..].iter().rev() {
                nested = if part.starts_with('@') {
                    json!([nested])
                } else {
                    json!({part: nested})
                };
            }
            return edit(text, node, &path[..1], Some(&nested));
        }
        if let Some(existing) = existing {
            if let Some(value) = value {
                let mut out = text.to_owned();
                out.replace_range(existing.range.clone(), &value.to_string());
                return Ok(out);
            }
            let mut out = text.to_owned();
            let (range, separator) = if key.starts_with('@') {
                let pos = node
                    .array
                    .iter()
                    .position(|(n, _)| n.range == existing.range)
                    .ok_or("missing plugin")?;
                let separator = node.array[pos]
                    .1
                    .or_else(|| pos.checked_sub(1).and_then(|p| node.array[p].1));
                (existing.range.clone(), separator)
            } else {
                let pos = node
                    .children
                    .iter()
                    .position(|(k, _, _, _)| k == key)
                    .ok_or("missing entry")?;
                let (_, start, n, comma) = &node.children[pos];
                let separator = comma.or_else(|| pos.checked_sub(1).and_then(|p| node.children[p].3));
                (*start..n.range.end, separator)
            };
            if let Some(comma) = separator.filter(|&p| p >= range.end) {
                out.remove(comma);
            }
            out.replace_range(range.clone(), "");
            if let Some(comma) = separator.filter(|&p| p < range.start) {
                out.remove(comma);
            }
            return Ok(out);
        }
        let Some(value) = value else {
            return Ok(text.into());
        };
        let (at, separator, entry) = if key.starts_with('@') {
            let last = node.array.last();
            (
                last.map_or(node.range.start + 1, |(n, _)| n.range.end),
                last.is_some_and(|(_, c)| c.is_none()),
                value.to_string(),
            )
        } else {
            let last = node.children.last();
            (
                last.map_or(node.range.start + 1, |(_, _, n, _)| n.range.end),
                last.is_some_and(|(_, _, _, c)| c.is_none()),
                format!("{}:{value}", Value::String(key.clone())),
            )
        };
        let mut out = text.to_owned();
        // If a trailing comma exists, insert after it instead, preserving it as
        // the separator before this entry; JSONC keeps a new trailing comma.
        let trailing = if key.starts_with('@') {
            node.array.last().and_then(|(_, c)| *c)
        } else {
            node.children.last().and_then(|(_, _, _, c)| *c)
        };
        if let Some(comma) = trailing {
            out.insert_str(comma + 1, &format!("{entry},"));
        } else {
            out.insert_str(at, &format!("{}{entry}", if separator { "," } else { "" }));
        }
        Ok(out)
    }
    let root = parse(text, comments)?;
    let out = edit(text, &root, path, value)?;
    parse(&out, comments)?;
    Ok(out)
}
