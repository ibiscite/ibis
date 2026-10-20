#[path = "../fuzz_targets/support/output_escaping.rs"]
mod target;

#[test]
fn synthetic_output_seeds() {
    for seed in [
        include_bytes!("../seeds/cite_render/simple.txt").as_slice(),
        include_bytes!("../seeds/cite_render/specials.txt").as_slice(),
        include_bytes!("../seeds/cite_render/unicode-typst.txt").as_slice(),
        include_bytes!("../seeds/cite_render/rejected.txt").as_slice(),
        include_bytes!("../seeds/cite_render/two-items.txt").as_slice(),
        include_bytes!("../seeds/output_escaping/markup.txt").as_slice(),
        include_bytes!("../seeds/output_escaping/unicode.txt").as_slice(),
        include_bytes!("../seeds/output_escaping/tex.txt").as_slice(),
        b"",
        b"\xff\0\t\n",
        "<script>alert('synthetic')</script>&amp;<i>italic</i>".as_bytes(),
    ] {
        target::check(seed);
    }
}

#[test]
fn controls_and_markup_boundaries() {
    for byte in 0..=255 {
        target::check(&[byte]);
    }
    for text in [
        "<",
        ">",
        "&",
        "&amp;",
        "&lt;script&gt;",
        "<i>",
        "</span>",
        "<IMG src=x onerror=run()>",
        "<b title=\"x\">",
        "引用\u{2028}= heading\u{2029}1. list",
    ] {
        target::check(text.as_bytes());
    }
}
