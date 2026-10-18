//! Distinct hostile aliases must share one load budget.

use std::process::Command;
use std::time::{Duration, Instant};

use ibis_core::{bibtex::load_str, model::WarningKind};

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
