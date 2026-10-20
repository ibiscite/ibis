#![no_main]

#[path = "support/bibtex_inheritance.rs"]
mod target;

libfuzzer_sys::fuzz_target!(|data: &[u8]| target::check(data));
