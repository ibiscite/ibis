#![no_main]

mod support {
    pub mod zotero_payload;
}

libfuzzer_sys::fuzz_target!(|data: &[u8]| support::zotero_payload::exercise(data));
