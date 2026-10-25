#[path = "../fuzz_targets/support/cite_render.rs"]
mod target;

#[test]
fn synthetic_cite_seeds() {
    for seed in [
        include_bytes!("../seeds/cite_render/simple.txt").as_slice(),
        include_bytes!("../seeds/cite_render/specials.txt").as_slice(),
        include_bytes!("../seeds/cite_render/unicode-typst.txt").as_slice(),
        include_bytes!("../seeds/cite_render/rejected.txt").as_slice(),
        include_bytes!("../seeds/cite_render/two-items.txt").as_slice(),
    ] {
        target::check(seed);
    }
}

/// Nightly fuzzing (#1096): a whitespace-only locator is rejected by `render`
/// (#1061); the checker must expect that rejection, before any later rule.
#[test]
fn whitespace_locators_are_expected_rejections() {
    target::check(include_bytes!("../seeds/cite_render/whitespace-locator.txt"));
    for locator in [" ", "  ", "\u{a0}", "\u{3000}", "\u{b}", "\r"] {
        for key in ["B", "a%b", "k,ey"] {
            target::check(format!("{key}\t\t{locator}").as_bytes());
            target::check(format!("{key}\tpre\t{locator}\tpost\t!").as_bytes());
        }
    }
}
