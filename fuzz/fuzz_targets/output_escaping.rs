#![no_main]

#[path = "support/output_escaping.rs"]
mod target;

libfuzzer_sys::fuzz_target!(|data: &[u8]| target::check(data));
