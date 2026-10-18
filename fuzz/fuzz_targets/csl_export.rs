#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let loaded = ibis_core::bibtex::load_str("fuzz.bib", &String::from_utf8_lossy(data));
    let items = ibis_core::library::csl_items(&loaded.references)
        .unwrap_or_else(|error| panic!("one source must have unique CSL IDs: {error}"));
    assert!(serde_json::to_vec(&items).is_ok());
});
