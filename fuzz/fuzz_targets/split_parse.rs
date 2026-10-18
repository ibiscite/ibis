#![no_main]

// Exercise the exact private implementation without adding a public fuzz API.
#[allow(dead_code)]
#[path = "../../crates/ibis-core/src/bibtex/lexer.rs"]
mod lexer;

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    for block in lexer::split_blocks(&text) {
        let _ = lexer::parse_entry(&block);
        let _ = lexer::parse_string(&block);
    }
});
