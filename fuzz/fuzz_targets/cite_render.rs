#![no_main]

#[path = "support/cite_render.rs"]
mod target;

libfuzzer_sys::fuzz_target!(|data: &[u8]| target::check(data));
