#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    ibis_fuzz::pdf::check_damaged_pdf(data);
});
