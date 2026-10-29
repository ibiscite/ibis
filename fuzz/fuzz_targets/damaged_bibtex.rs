#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    ibis_fuzz::check_damaged_bibtex(data);
});
