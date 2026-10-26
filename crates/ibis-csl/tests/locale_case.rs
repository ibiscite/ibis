//! Synthetic locale transformations citeproc uses before collation/title casing.
use rquickjs::{Context, Runtime};
use serde::Deserialize;

#[derive(Deserialize)]
struct Case {
    locale: String,
    input: String,
    lower: String,
    upper: String,
}

#[test]
fn locale_case_matches_pinned_node_output() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("locale-cases.json")).unwrap();
    let runtime = Runtime::new().unwrap();
    let context = Context::full(&runtime).unwrap();
    context.with(|ctx| {
        for case in cases {
            ctx.globals().set("input", case.input).unwrap();
            ctx.globals().set("locale", case.locale).unwrap();
            let lower: String = ctx.eval("input.toLocaleLowerCase(locale)").unwrap();
            let upper: String = ctx.eval("input.toLocaleUpperCase(locale)").unwrap();
            assert_eq!(lower, case.lower);
            assert_eq!(upper, case.upper);
        }
    });
}
