//! PDF extractor oracle: the parsing entry point of `ibis __extract-pdf`.
//!
//! Any bytes give a refusal or an answer the parent accepts: one text and
//! label slot per page, within the page and text bounds, the same page count
//! with and without text, and no labels from a file without a label tree.
//! A panic or a stall here would be contained by the child process in Ibis,
//! but it is still a finding.

use std::time::{Duration, Instant};

pub const MAX_INPUT_BYTES: usize = 1024 * 1024;
const INPUT_TIMEOUT: Duration = Duration::from_secs(5);

pub fn check_damaged_pdf(data: &[u8]) {
    if data.len() > MAX_INPUT_BYTES {
        return;
    }
    let started = Instant::now();
    let full = ibis_sources::pdf::read::extract(data, false);
    let labels = ibis_sources::pdf::read::extract(data, true);
    match (full, labels) {
        (Ok(full), Ok(labels)) => {
            assert_eq!(full.pages, labels.pages, "page counts differ with and without text");
            assert!(full.labels == labels.labels, "labels differ with and without text");
            assert!(labels.text.is_none(), "text where only labels were asked for");
            let pages = full.pages;
            let checked = ibis_sources::pdf::checked(full, false)
                .unwrap_or_else(|refusal| panic!("the parent refuses the child's answer: {refusal:?}"));
            assert_eq!(checked.pages, pages);
            ibis_sources::pdf::checked(labels, true)
                .unwrap_or_else(|refusal| panic!("the parent refuses the child's labels: {refusal:?}"));
        }
        (Err(full), Err(labels)) => assert_eq!(full, labels, "refusals differ with and without text"),
        // Text past its bound is refused only when text is asked for.
        (Err(ibis_sources::pdf::Refusal::TooLarge), Ok(_)) => (),
        _ => panic!("the same bytes were read once and refused once"),
    }
    assert!(started.elapsed() < INPUT_TIMEOUT, "input exceeded five seconds");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed seeds run in the crate's `seed_corpus`; these are made here.
    #[test]
    fn invented_pdfs_and_their_cuts() {
        let invented = ibis_sources::pdf::read::invented_pdf(
            &["Invented first page.", "Invented second page."],
            Some(&[(0, Some("r"), "", 1), (1, Some("D"), "B-", 1)]),
        );
        check_damaged_pdf(&invented);
        for cut in [0, 5, 64, invented.len() / 2] {
            check_damaged_pdf(&invented[..cut]);
        }
        check_damaged_pdf(b"%PDF-");
    }
}
