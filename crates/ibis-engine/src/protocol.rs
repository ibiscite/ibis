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
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|_| FrameError::InvalidJson)
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
    validate_shape(message)?;
    struct Body(Vec<u8>);
    impl Write for Body {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_FRAME_BYTES - self.0.len() {
                return Err(io::Error::other("message exceeds the size limit"));
            }
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut body = Body(vec![]);
    serde_json::to_writer(&mut body, message).map_err(|_| FrameError::TooLarge)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.0.len()).map_err(|_| FrameError::Io)?;
    writer
        .write_all(&body.0)
        .and_then(|()| writer.flush())
        .map_err(|_| FrameError::Io)
}

fn validate_shape(message: &Value) -> Result<(), FrameError> {
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
