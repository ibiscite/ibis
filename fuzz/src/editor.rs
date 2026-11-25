//! The editor endpoint (decision 0008): an HTTP request head that any
//! program on this computer, or a web page through the browser, can send to
//! the loopback port. `ibis_engine::cayw::parse` decides what it means.

use std::time::Instant;

use ibis_engine::cayw::{self, CaywRequest, Refusal};

use crate::{assert_in_time, assert_no_contained_panic, watch_panics};

const PORT: u16 = 23_124;
const TOKEN: &str = "0123456789abcdef0123456789abcdef";

/// Any head gives a request or a fixed refusal; one that is accepted named
/// the token, came from this computer's own name, and fits the bound.
pub fn check_editor_request(head: &[u8]) {
    if head.len() > 2 * cayw::MAX_HEAD_BYTES {
        return;
    }
    watch_panics();
    let started = Instant::now();
    let result = cayw::parse(head, PORT, TOKEN);
    assert_no_contained_panic();
    match result {
        Ok(request) => {
            assert!(head.len() <= cayw::MAX_HEAD_BYTES, "an oversized head was accepted");
            let text = std::str::from_utf8(head).unwrap_or_else(|_| panic!("a request that is not text was accepted"));
            assert!(text.contains(TOKEN), "a request without the token was accepted");
            assert!(text.starts_with("GET "), "a method other than GET was accepted");
            let lower = text.to_ascii_lowercase();
            assert!(!lower.contains("\r\norigin:"), "a browser's Origin was accepted");
            if let CaywRequest::Pick(pick) = request {
                for path in [&pick.document, &pick.bibliography].into_iter().flatten() {
                    assert!(path.is_absolute(), "a relative document or bibliography was accepted");
                }
            }
        }
        Err(refusal) => {
            let (status, _) = refusal.status();
            assert!((400..500).contains(&status), "a refusal outside 4xx");
            if head.len() > cayw::MAX_HEAD_BYTES {
                assert_eq!(refusal, Refusal::TooLarge);
            }
        }
    }
    assert_in_time(started);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The seeds are requests the endpoint accepts, not early refusals.
    #[test]
    fn seeds_are_accepted_requests() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus/editor_request");
        for (name, probe) in [("probe.txt", true), ("pick.txt", false)] {
            let head = std::fs::read(root.join(name)).unwrap_or_else(|_| panic!("no seed"));
            let request = cayw::parse(&head, PORT, TOKEN).unwrap_or_else(|refusal| panic!("{name}: {refusal:?}"));
            assert_eq!(matches!(request, CaywRequest::Probe), probe, "{name}");
        }
    }
}
