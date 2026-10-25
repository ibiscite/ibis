//! Bounded structured requests, shared by front ends without I/O policy of their own.

use std::fmt;
use std::io::Read;

use serde::Deserializer;
use serde::de::{Error, SeqAccess, Visitor};
use thiserror::Error;

use super::CiteItem;

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
pub const MAX_INPUT_ITEMS: usize = 1000;

#[derive(Debug, Error)]
pub enum InputError {
    #[error("citation input could not be read")]
    Unreadable,
    #[error("citation input is larger than 1 MiB")]
    TooLarge,
    #[error("citation input contains more than 1000 items")]
    TooManyItems,
    // Never echo request contents or attach serde's potentially sensitive error.
    #[error("invalid citation input at line {line}, column {column}; expected a JSON array of citation items")]
    Invalid { line: usize, column: usize },
}

/// Read one JSON array. Formatting separately validates keys and citation text.
/// Reading stops at the byte limit even for an endless stream; deserialization
/// stops at the item limit without building the rest of an oversized array.
pub fn read_items(reader: impl Read) -> Result<Vec<CiteItem>, InputError> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_INPUT_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| InputError::Unreadable)?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(InputError::TooLarge);
    }
    let mut too_many = false;
    let mut decoder = serde_json::Deserializer::from_slice(&bytes);
    let result = decoder
        .deserialize_seq(Items(&mut too_many))
        .and_then(|items| decoder.end().map(|()| items));
    result.map_err(|error| {
        if too_many {
            InputError::TooManyItems
        } else {
            InputError::Invalid {
                line: error.line(),
                column: error.column(),
            }
        }
    })
}

struct Items<'a>(&'a mut bool);

impl<'de> Visitor<'de> for Items<'_> {
    type Value = Vec<CiteItem>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of citation items")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = sequence.next_element()? {
            if items.len() == MAX_INPUT_ITEMS {
                *self.0 = true;
                return Err(A::Error::custom("too many citation items"));
            }
            items.push(item);
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_limit_stops_an_endless_reader_and_accepts_the_exact_boundary() {
        assert!(matches!(read_items(std::io::repeat(b' ')), Err(InputError::TooLarge)));
        let mut input = br#"[{"key":"example"}]"#.to_vec();
        input.resize(MAX_INPUT_BYTES, b' ');
        assert_eq!(read_items(input.as_slice()).unwrap_or_else(|e| panic!("{e}")).len(), 1);
    }

    #[test]
    fn item_count_is_bounded_without_discarding_extra_requests() {
        let one = r#"{"key":"example"}"#;
        let input = format!("[{}]", vec![one; MAX_INPUT_ITEMS].join(","));
        assert_eq!(
            read_items(input.as_bytes()).unwrap_or_else(|e| panic!("{e}")).len(),
            MAX_INPUT_ITEMS
        );
        let extra = format!("[{},{}]", &input[1..input.len() - 1], one);
        assert!(matches!(read_items(extra.as_bytes()), Err(InputError::TooManyItems)));
    }

    #[test]
    fn malformed_shapes_unknown_fields_duplicates_and_trailing_data_are_rejected() {
        for input in [
            "",
            "{}",
            "null",
            "[1]",
            "[{}]",
            "[] []",
            "[",
            r#"[{"key":"a","prefixx":"private synthetic text"}]"#,
            r#"[{"key":"a","key":"b"}]"#,
            r#"[{"key":"a","locator":{"label":"unknown","value":"3"}}]"#,
            r#"[{"key":"a","locator":{"label":"page","value":"3","extra":true}}]"#,
        ] {
            let error = read_items(input.as_bytes())
                .err()
                .unwrap_or_else(|| panic!("invalid request accepted"));
            assert!(matches!(error, InputError::Invalid { .. }), "{input}");
            assert!(!error.to_string().contains("private synthetic text"));
        }
    }

    #[test]
    fn unreadable_input_has_a_fixed_diagnostic() {
        struct Broken;
        impl Read for Broken {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("private synthetic diagnostic"))
            }
        }
        assert_eq!(
            read_items(Broken)
                .err()
                .unwrap_or_else(|| panic!("broken reader accepted"))
                .to_string(),
            "citation input could not be read"
        );
    }
}
