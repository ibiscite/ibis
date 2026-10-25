//! Synthetic oracles come directly from pinned citeproc using semantic work IDs.
use super::{Occurrence, Output, Work, format, intl::Failure};
use crate::{cite::CiteItem, model::RecordId, settings::CitationStyle};
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

#[derive(Deserialize)]
struct Corpus {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    works: Vec<WorkInput>,
    occurrences: Vec<CiteInput>,
    expected: Vec<Expected>,
}
#[derive(Deserialize)]
struct WorkInput {
    identity: RecordId,
    csl: Value,
}
#[derive(Deserialize)]
struct CiteInput {
    identity: RecordId,
    intent: CiteItem,
}
#[derive(Deserialize)]
struct Expected {
    style: CitationStyle,
    output: String,
    bibliography: bool,
    value: String,
}

fn corpus() -> Vec<Case> {
    serde_json::from_str::<Corpus>(include_str!("../../../../fixtures/csl/headless-parity.json"))
        .expect("synthetic corpus is valid")
        .cases
}
fn input(case: &Case) -> (Vec<Work>, Vec<Occurrence>) {
    let works = case
        .works
        .iter()
        .map(|w| Work {
            identity: w.identity.clone(),
            csl: w.csl.clone(),
        })
        .collect();
    let cites = case
        .occurrences
        .iter()
        .map(|c| Occurrence {
            identity: c.identity.clone(),
            intent: c.intent.clone(),
        })
        .collect();
    (works, cites)
}
fn cancel(value: bool) -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(value))
}

#[test]
fn pinned_semantic_oracles_cover_five_styles_and_all_output_formats() {
    let mut checked = 0;
    for case in corpus() {
        let (works, cites) = input(&case);
        for expected in case.expected {
            let output = match expected.output.as_str() {
                "text" => Output::Text,
                "html" => Output::Html,
                "rtf" => Output::Rtf,
                _ => panic!("unknown synthetic output format"),
            };
            let actual = format(
                &works,
                &cites,
                expected.style,
                output,
                expected.bibliography,
                cancel(false),
            )
            .expect("synthetic reference formats");
            assert_eq!(
                actual, expected.value,
                "{} / {:?} / {} / bibliography={}",
                case.name, expected.style, expected.output, expected.bibliography
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 58);
}

fn rejects(works: &[Work], cites: &[Occurrence], bibliography: bool, cancelled: bool, expected: Failure) {
    let result = format(
        works,
        cites,
        CitationStyle::Apa,
        Output::Text,
        bibliography,
        cancel(cancelled),
    );
    assert!(
        result.is_err_and(|error| std::mem::discriminant(&error) == std::mem::discriminant(&expected)),
        "failure category changed"
    );
    // A rejected request cannot poison the next request's identity or engine state.
    let healthy = corpus().remove(1);
    let (works, cites) = input(&healthy);
    assert_eq!(
        format(&works, &cites, CitationStyle::Apa, Output::Text, false, cancel(false))
            .expect("healthy request after rejection"),
        healthy.expected[0].value
    );
}

#[test]
fn invalid_identity_shape_and_cancelled_work_fail_without_poisoning_recovery() {
    let case = corpus().remove(1);
    let (works, mut cites) = input(&case);
    cites[0].identity.native_key = "missing".into();
    rejects(&works, &cites, false, false, Failure::Invalid);
    let (mut works, cites) = input(&case);
    works[0].csl = Value::Null;
    rejects(&works, &cites, false, false, Failure::Invalid);
    let (mut works, cites) = input(&case);
    works.push(Work {
        identity: works[0].identity.clone(),
        csl: json!({"type":"book", "title":"Conflicting synthetic work"}),
    });
    rejects(&works, &cites, false, false, Failure::Invalid);
    let (works, cites) = input(&case);
    rejects(&works, &cites, false, true, Failure::Cancelled);
}

#[test]
fn records_occurrences_and_serialized_bytes_have_independent_ceilings() {
    let case = corpus().remove(1);
    let (works, cites) = input(&case);
    let repeated = (0..1001)
        .map(|_| Occurrence {
            identity: cites[0].identity.clone(),
            intent: cites[0].intent.clone(),
        })
        .collect::<Vec<_>>();
    rejects(&works, &repeated, false, false, Failure::Limit);
    let many = (0..1001)
        .map(|index| Work {
            identity: RecordId {
                source: "synthetic:bounded".into(),
                native_key: index.to_string(),
            },
            csl: json!({"type":"book", "title":"Synthetic work"}),
        })
        .collect::<Vec<_>>();
    rejects(&many, &[], true, false, Failure::Limit);
    let (mut works, cites) = input(&case);
    works[0].csl["title"] = Value::String("x".repeat(1024 * 1024));
    rejects(&works, &cites, false, false, Failure::Limit);
}

// This suite uses the extended deadline; other suites retain the two-second wrapper.
#[allow(dead_code)]
#[path = "../../tests/support/subprocess_watchdog.rs"]
mod subprocess_watchdog;

fn execute_script(script: &str, timeout: std::time::Duration) -> Result<String, Failure> {
    let guard = std::rc::Rc::new(super::intl::Guard::new(
        std::time::Instant::now() + timeout,
        cancel(false),
    ));
    super::execute("{}", "", script, guard)
}

#[test]
fn native_intrinsics_preserve_default_sort_options_and_turkish_case() {
    let script = r#"
JSON.stringify([
    Math.sign('2'.localeCompare('10', 'en-US')),
    Math.sign('2'.localeCompare('10', 'en-US', {sensitivity:'base', numeric:true, ignorePunctuation:true})),
    Math.sign('a'.localeCompare('A', 'en-US')),
    Math.sign('a'.localeCompare('A', 'en-US', {sensitivity:'base', numeric:true, ignorePunctuation:true})),
    'istanbul'.toLocaleUpperCase(['tr']), 'I'.toLocaleLowerCase(['tr'])
])
"#;
    assert_eq!(
        execute_script(script, std::time::Duration::from_secs(5)).expect("qualified intrinsic calls"),
        "[1,-1,-1,0,\"İSTANBUL\",\"ı\"]"
    );
}

#[test]
fn swallowed_native_failures_and_invalid_output_still_reject_the_request() {
    let cases = [
        (
            "try { 'a'.localeCompare('b', 'unknown'); } catch (_) {} 'caught'",
            Failure::Unsupported,
        ),
        (
            "try { 'a'.localeCompare('b', 'en-US', {}); } catch (_) {} 'caught'",
            Failure::Unsupported,
        ),
        (
            r#"try { 'a'.localeCompare('b', 'en-US', {get sensitivity(){return 'base'}, numeric:true, ignorePunctuation:true}); } catch (_) {} 'caught'"#,
            Failure::Unsupported,
        ),
        (
            r#"const locales = ['en-US']; Object.defineProperty(locales, '0', {get(){return 'en-US'}}); try { 'a'.toLocaleUpperCase(locales); } catch (_) {} 'caught'"#,
            Failure::Unsupported,
        ),
        (
            r#"try { '\ud800'.toLocaleUpperCase('en-US'); } catch (_) {} 'caught'"#,
            Failure::Invalid,
        ),
        (r#"'\ud800'"#, Failure::Invalid),
        ("'x'.repeat(1048577)", Failure::Limit),
    ];
    for (script, expected) in cases {
        assert_eq!(execute_script(script, std::time::Duration::from_secs(5)), Err(expected));
        assert_eq!(
            execute_script("'healthy'", std::time::Duration::from_secs(5)),
            Ok("healthy".into())
        );
    }
}

#[test]
fn native_interrupt_returns_and_next_request_runs() {
    const CHILD: &str = "IBIS_TEST_HEADLESS_INTERRUPT";
    if std::env::var_os(CHILD).is_some() {
        // The guard includes processor initialization, not only script execution.
        assert_eq!(
            execute_script("while (true) {}", std::time::Duration::from_millis(100)),
            Err(Failure::TimedOut)
        );
        assert_eq!(
            execute_script("'healthy'", std::time::Duration::from_secs(5)),
            Ok("healthy".into())
        );
        return;
    }
    subprocess_watchdog::assert_finishes_with_timeout(
        "headless_csl::tests::native_interrupt_returns_and_next_request_runs",
        CHILD,
        "headless citation interrupt did not return",
        std::time::Duration::from_secs(10),
    );
}
