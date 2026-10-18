//! Bound hostile work without truncating ordinary large libraries.

use std::process::Command;
use std::time::{Duration, Instant};

use ibis_core::{bibtex::load_str, model::WarningKind};

fn assert_complete(text: &str, count: usize) {
    let start = Instant::now();
    let loaded = load_str("synthetic.bib", text);
    eprintln!(
        "ordinary library: bytes={}, references={}, warnings={}, elapsed={:?}",
        text.len(),
        loaded.references.len(),
        loaded.warnings.len(),
        start.elapsed()
    );
    assert_eq!(loaded.references.len(), count);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.references[count - 1].id.native_key, format!("k{}", count - 1));
}

#[test]
fn two_hundred_long_notes_load_completely() {
    let note = "x".repeat(65_535);
    let text: String = (0..200)
        .map(|i| format!("@article{{k{i},title={{Paper {i}}},year=2024,note={{{note}}}}}\n"))
        .collect();
    assert_complete(&text, 200);
}

#[test]
fn twenty_thousand_ordinary_ten_field_records_load_completely() {
    let fields = "title={Small synthetic record},author={Doe, Jane},journal={Sample Journal},year={2024},\
                  volume={12},number={3},pages={1--9},doi={10.0000/example},url={https://example.org/r},keywords={sample; test}";
    let text: String = (0..20_000).map(|i| format!("@article{{k{i},{fields}}}\n")).collect();
    assert_complete(&text, 20_000);
}

#[test]
fn distinct_aliases_share_one_load_budget() {
    const CHILD: &str = "IBIS_LOAD_BUDGET_TEST_CHILD";
    if std::env::var_os(CHILD).is_some() {
        let mut text = String::from("@article{kept,title={Retained}}\n@string{m0={}}\n");
        for i in 1..=17 {
            text.push_str(&format!("@string{{m{i}=m{} # m{}}}\n", i - 1, i - 1));
        }
        for i in 0..1_000 {
            text.push_str(&format!(
                "@string{{alias{i}=m17}}\n@article{{k{i},title=alias{i},year=2024}}\n"
            ));
        }
        text.push_str("@article{suffix,title={Skipped}}\n");
        let start = Instant::now();
        let loaded = load_str("synthetic.bib", &text);
        eprintln!("synthetic load: {:?}", start.elapsed());
        assert_eq!(loaded.references[0].id.native_key, "kept");
        assert!(loaded.references.len() < 1_001);
        assert!(!loaded.references.iter().any(|r| r.id.native_key == "suffix"));
        let warnings: Vec<_> = loaded
            .warnings
            .iter()
            .filter(|w| w.message.contains("load work limit"))
            .collect();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, WarningKind::Syntax);
        assert!(warnings[0].line.is_some() && warnings[0].key.is_some());
        return;
    }
    // Run this test's loader in a killable process, so a regression cannot hang CI.
    let mut child = Command::new(std::env::current_exe().unwrap_or_else(|e| panic!("{e}")))
        .args(["--exact", "distinct_aliases_share_one_load_budget", "--nocapture"])
        .env(CHILD, "1")
        .spawn()
        .unwrap_or_else(|e| panic!("{e}"));
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap_or_else(|e| panic!("{e}")) {
            assert!(status.success());
            break;
        }
        if start.elapsed() >= Duration::from_secs(2) {
            child.kill().unwrap_or_else(|e| panic!("{e}"));
            child.wait().unwrap_or_else(|e| panic!("{e}"));
            panic!("synthetic load exceeded the 2 second deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
