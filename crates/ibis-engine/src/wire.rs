//! Deterministic, bounded JSON independent of serde_json's map feature flags.
use crate::protocol::{FrameError, validate_shape_with_limit};
use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq},
};
use serde_json::Value;
use std::io::{self, Write};

/// Complete, bounded JSON. Transports can borrow the accepted bytes, never mutate them.
#[derive(Debug)]
pub(crate) struct EncodedReply(Box<[u8]>);

impl EncodedReply {
    pub(crate) fn new(value: &Value, limit: usize) -> Result<Self, FrameError> {
        encode(value, limit).map(|bytes| Self(bytes.into_boxed_slice()))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
pub(crate) mod retained_reply_tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub fn payload() -> Value {
        serde_json::json!([
            {"kind":"note", "item":{"library":"invented-a", "key":"note-1"},
             "text":"Invented note: \"first\"\n第二"},
            {"kind":"annotation", "item":{"library":"invented-b", "key":"annotation-2"},
             "text":"Invented passage", "comment":"Invented comment", "page":7}
        ])
    }

    #[derive(Default)]
    pub struct RecordingWriter {
        pub bytes: Vec<u8>,
        pub events: Arc<Mutex<Vec<&'static str>>>,
    }
    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            self.events.lock().unwrap().push("emitted");
            Ok(())
        }
    }

    #[test]
    fn retained_reply_encoding_bounds_escaped_bytes_before_any_emission() {
        let mut value = payload();
        let expected = encode(&value, 4096).unwrap();
        let mut writer = RecordingWriter::default();
        assert_eq!(
            EncodedReply::new(&value, expected.len() - 1).unwrap_err(),
            FrameError::TooLarge
        );
        assert!(writer.bytes.is_empty());
        let reply = EncodedReply::new(&value, expected.len()).unwrap();
        value[0]["text"] = Value::String("changed after encoding".into());
        writer.write_all(reply.as_bytes()).unwrap();
        assert_eq!(writer.bytes, expected);
        let mut deep = payload();
        for _ in 0..130 {
            deep = Value::Array(vec![deep]);
        }
        assert_eq!(EncodedReply::new(&deep, 4096).unwrap_err(), FrameError::TooLarge);
    }
}
