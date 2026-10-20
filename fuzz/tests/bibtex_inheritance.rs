#[path = "../fuzz_targets/support/bibtex_inheritance.rs"]
mod target;

#[test]
fn synthetic_graphs_and_hostile_values() {
    for seed in [
        include_bytes!("../seeds/bibtex_inheritance/chain.bib").as_slice(),
        include_bytes!("../seeds/bibtex_inheritance/cycle.bib").as_slice(),
        include_bytes!("../seeds/bibtex_inheritance/case.bib").as_slice(),
        include_bytes!("../seeds/bibtex_inheritance/hostile.bib").as_slice(),
        b"",
        b"\xff\0{}\\",
    ] {
        target::check(seed);
    }
    // Every generated depth, cycle mode, own-field choice and parent order.
    for depth in 0..32 {
        for flags in 0..16 {
            target::check(&[depth, flags]);
        }
    }
}
