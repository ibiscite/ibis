#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = ibis_core::bibtex::load_str("fuzz.bib", &String::from_utf8_lossy(data));
});
