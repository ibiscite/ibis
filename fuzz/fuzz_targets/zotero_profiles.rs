#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    ibis_fuzz::files::check_zotero_profiles(data);
});
