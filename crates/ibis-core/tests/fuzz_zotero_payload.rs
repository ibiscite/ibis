#[path = "../../../fuzz/fuzz_targets/support/zotero_payload.rs"]
mod target;

#[test]
fn synthetic_zotero_payload_seeds() {
    for seed in [
        include_bytes!("../../../fuzz/seeds/zotero_payload/mixed.json").as_slice(),
        include_bytes!("../../../fuzz/seeds/zotero_payload/export-text.json").as_slice(),
        include_bytes!("../../../fuzz/seeds/zotero_payload/neighbors.json").as_slice(),
        b"null",
        b"[]",
        b"{}",
        b"[false,0,\"bad\",[]]",
    ] {
        target::exercise(seed);
    }
}
