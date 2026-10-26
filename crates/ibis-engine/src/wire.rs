//! Deterministic, bounded JSON independent of serde_json's map feature flags.
use crate::protocol::{FrameError, validate_shape_with_limit};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::Value;
use std::io::{self, Write};

struct Canonical<'a>(&'a Value);
impl Serialize for Canonical<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.0 {
            Value::Object(object) => {
                let mut keys: Vec<_> = object.keys().collect();
                keys.sort_unstable();
                let mut map = serializer.serialize_map(Some(keys.len()))?;
                for key in keys {
                    map.serialize_entry(key, &Canonical(&object[key]))?;
                }
                map.end()
            }
            Value::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&Canonical(value))?;
                }
                sequence.end()
            }
            value => value.serialize(serializer),
        }
    }
}

/// Stage the complete message before any transport bytes are written.
pub fn encode(value: &Value, limit: usize) -> Result<Vec<u8>, FrameError> {
    validate_shape_with_limit(value, limit)?;
    struct Bounded {
        bytes: Vec<u8>,
        limit: usize,
    }
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
                return Err(io::Error::other("message exceeds the size limit"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded { bytes: vec![], limit };
    serde_json::to_writer(&mut output, &Canonical(value)).map_err(|_| FrameError::TooLarge)?;
    Ok(output.bytes)
}

/// Count serialized bytes without copying a library object or building JSON.
pub(crate) fn serialized_size(value: &impl Serialize, limit: usize) -> Result<usize, FrameError> {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes) {
                return Err(io::Error::other("result exceeds the size limit"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter { bytes: 0, limit };
    serde_json::to_writer(&mut counter, value).map_err(|_| FrameError::TooLarge)?;
    Ok(counter.bytes)
}
