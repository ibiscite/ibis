//! Bounded LSP-style framing. No request payload is included in diagnostics.
use serde_json::Value;
use std::io::{self, BufRead, Write};
use thiserror::Error;

pub const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_HEADER_BYTES: usize = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum FrameError {
    #[error("invalid message header")]
    InvalidHeader,
    #[error("message exceeds the size limit")]
    TooLarge,
    #[error("incomplete message")]
    UnexpectedEof,
    #[error("invalid JSON message")]
    InvalidJson,
    #[error("message transport failed")]
    Io,
}

pub fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>, FrameError> {
    let mut remaining = MAX_HEADER_BYTES;
    let mut length = None;
    let mut headers = 0;
    loop {
        let Some(line) = header_line(reader, &mut remaining)? else {
            return if headers == 0 {
                Ok(None)
            } else {
                Err(FrameError::UnexpectedEof)
            };
        };
        if line == b"\r\n" {
            break;
        }
        headers += 1;
        if headers > 64 {
            return Err(FrameError::TooLarge);
        }
        let line = line.strip_suffix(b"\r\n").ok_or(FrameError::InvalidHeader)?;
        if line
            .iter()
            .any(|byte| !byte.is_ascii() || (byte.is_ascii_control() && *byte != b'\t'))
        {
            return Err(FrameError::InvalidHeader);
        }
        let line = std::str::from_utf8(line).map_err(|_| FrameError::InvalidHeader)?;
        let (name, value) = line.split_once(':').ok_or(FrameError::InvalidHeader)?;
        if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-') {
            return Err(FrameError::InvalidHeader);
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("Content-Length") {
            if length.is_some() || value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(FrameError::InvalidHeader);
            }
            let parsed = value.parse::<usize>().map_err(|_| FrameError::TooLarge)?;
            if parsed > MAX_FRAME_BYTES {
                return Err(FrameError::TooLarge);
            }
            length = Some(parsed);
        } else if name.eq_ignore_ascii_case("Content-Type") {
            let value = value.to_ascii_lowercase();
            let mut parts = value.split(';').map(str::trim);
            if !matches!(parts.next(), Some("application/vscode-jsonrpc" | "application/json")) {
                return Err(FrameError::InvalidHeader);
            }
            for parameter in parts {
                let (name, value) = parameter.split_once('=').ok_or(FrameError::InvalidHeader)?;
                if name.trim() == "charset" && !matches!(value.trim().trim_matches('"'), "utf-8" | "utf8") {
                    return Err(FrameError::InvalidHeader);
                }
            }
        }
    }
    let mut body = vec![0; length.ok_or(FrameError::InvalidHeader)?];
    reader.read_exact(&mut body).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            FrameError::UnexpectedEof
        } else {
            FrameError::Io
        }
    })?;
    parse_json(&body).map(Some)
}

/// Shared bounded JSON reader for stdio and command-line request input.
pub fn parse_json(bytes: &[u8]) -> Result<Value, FrameError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(FrameError::TooLarge);
    }
    let value = serde_json::from_slice::<UniqueValue>(bytes)
        .map_err(|_| FrameError::InvalidJson)?
        .0;
    validate_shape(&value)?;
    Ok(value)
}

fn header_line(reader: &mut impl BufRead, remaining: &mut usize) -> Result<Option<Vec<u8>>, FrameError> {
    let mut line = vec![];
    loop {
        let available = reader.fill_buf().map_err(|_| FrameError::Io)?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(FrameError::UnexpectedEof)
            };
        }
        let scan = &available[..available.len().min(remaining.saturating_add(1))];
        let count = scan
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(scan.len(), |index| index + 1);
        if count > *remaining {
            return Err(FrameError::TooLarge);
        }
        let complete = available[count - 1] == b'\n';
        line.extend_from_slice(&available[..count]);
        reader.consume(count);
        *remaining -= count;
        if complete {
            return Ok(Some(line));
        }
    }
}

pub fn write_frame(writer: &mut impl Write, message: &Value) -> Result<(), FrameError> {
    let body = crate::wire::encode(message, MAX_FRAME_BYTES)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len()).map_err(|_| FrameError::Io)?;
    writer
        .write_all(&body)
        .and_then(|()| writer.flush())
        .map_err(|_| FrameError::Io)
}

pub(crate) fn validate_shape(message: &Value) -> Result<(), FrameError> {
    let mut pending = vec![(message, 0)];
    let mut visited = 0;
    while let Some((value, depth)) = pending.pop() {
        visited += 1;
        if depth > 128 || visited > MAX_FRAME_BYTES {
            return Err(FrameError::TooLarge);
        }
        match value {
            Value::String(text) if text.len() > MAX_FRAME_BYTES => return Err(FrameError::TooLarge),
            Value::Array(values) => {
                if values.len() > MAX_FRAME_BYTES.saturating_sub(pending.len()) {
                    return Err(FrameError::TooLarge);
                }
                pending.extend(values.iter().map(|value| (value, depth + 1)));
            }
            Value::Object(values) => {
                if values.len() > MAX_FRAME_BYTES.saturating_sub(pending.len())
                    || values.keys().any(|key| key.len() > MAX_FRAME_BYTES)
                {
                    return Err(FrameError::TooLarge);
                }
                pending.extend(values.values().map(|value| (value, depth + 1)));
            }
            _ => {}
        }
    }
    Ok(())
}

/// Reject ambiguous duplicate members before converting method parameters to
/// Value; otherwise a later typed reader can no longer observe the duplicate.
struct UniqueValue(Value);
impl<'de> serde::Deserialize<'de> for UniqueValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("unambiguous JSON")
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Null))
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::Bool(value)))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::from(value)))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::from(value)))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|value| UniqueValue(Value::Number(value)))
                    .ok_or_else(|| E::custom("invalid JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value.into())))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(UniqueValue(Value::String(value)))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<UniqueValue>()? {
                    values.push(value.0);
                }
                Ok(UniqueValue(Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON object member"));
                    }
                    let value = map.next_value::<UniqueValue>()?;
                    values.insert(key, value.0);
                }
                Ok(UniqueValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}
