//! What a person feels, measured on the engine path the desktop calls: opening
//! a library, the first search after summoning the picker, each keystroke,
//! refresh, the first citation and warm formatting, idle CPU and resident
//! memory. Each case prints one `PERF {json}` line and asserts correctness
//! only; `scripts/perf.mjs` runs the cases in fresh processes and owns every
//! limit (docs/architecture.md › Performance checks). Window paint and the
//! webview are outside this harness.
#![cfg(not(debug_assertions))]
use ibis_core::contract::*;
use ibis_engine::{Cancellation, Engine, Formatting, Method, request::KeyCitationRequest, request::SearchRequest};
use serde_json::json;
use std::{
    sync::{
        Barrier,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

fn library(records: usize, edited: usize) -> String {
    (0..records)
        .map(|index| {
            let title = if index == edited { "Edited synthetic signals" } else { "Synthetic signals" };
            format!(
                "@article{{synthetic{index},title={{{title} {index}}},author={{Researcher{}, Avery and Morgan{}, Ann}},year={{{}}},journal={{Invented Letters}},pages={{1--20}},doi={{10.1000/synthetic{index}}},volume={{{}}},number={{{}}}}}\n",
                index % 5_000,
                index % 701,
                1950 + index % 75,
                index % 12 + 1,
                index % 4 + 1,
            )
        })
        .collect()
}

fn percentile(mut samples: Vec<Duration>, percent: usize) -> Duration {
    samples.sort_unstable();
    samples[(samples.len() * percent / 100).min(samples.len() - 1)]
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

// Captured from the pre-optimization scorer on this exact generated 100k
// library. Each digest covers ordered (native key, score, matched, typo, partial)
// tuples, excluding the temporary file's library ID. Never regenerate these
// from the optimized implementation to make a mismatch pass.
fn assert_original_ranking(engine: &Engine, cancel: &Cancellation) {
    let cases = [
        (
            "researcher42 2001",
            "b566fbfb106d01991f6cadb6e155b6cb363aa2d3a2ad1602c435acf546be2c12",
            34,
            false,
        ),
        (
            "synthetic signals 99999",
            "0609a507dbaf9318f6f5e7f85d894cb903674a6f059ee2291c8a6e2f2e490425",
            1,
            false,
        ),
        (
            "morgan7",
            "5f0870a0915f0df82cf5ddd936c767094e566dfd295a112bae3b737fc757bb45",
            50,
            false,
        ),
        (
            "invented letters",
            "073a5546d87f167b5c592401ed177f4ddad5a2e999455c9ea5921a844a6dc061",
            50,
            false,
        ),
        (
            "avery",
            "85fd5e6c7f758903399aa5a3e6798575a3e64ccdaa01dbcd59b3a25ba368af20",
            50,
            false,
        ),
        (
            "synthetic quasar",
            "6caa73cb874196c899b9cb163cb3b0ffae98d53897097ce6e109a5e438c18480",
            50,
            true,
        ),
        (
            "synthetci signals",
            "508c7c0422fb46be536552859bded63a985fa4fd9ad955208af4b5879bf0ef01",
            50,
            false,
        ),
        (
            "SYNTHETIC42",
            "127cf44a7f7b513456242978c16e46c752662077c97f54bf59a8cee827f914a7",
            50,
            false,
        ),
        (
            "a:researcher42",
            "c2cf137df0fbc484fabbceded0b7f20d2c75dbef5eaf83f97fb6ca532e432e47",
            50,
            false,
        ),
        (
            "j:invented y:2001",
            "ef4416e58861455d1633e3edee41257b7ad597692528275d15a9e53f0622a27f",
            50,
            false,
        ),
        (
            "10.1000/synthetic42",
            "9059acff4fa2c7752593197b8f8fe21006ea0488a222af66e09006cac66bd252",
            1,
            false,
        ),
    ];
    for (query, expected, count, partial) in cases {
        let result = engine
            .search(
                &SearchRequest {
                    query: query.into(),
                    limit: 50,
                    libraries: None,
                    interpret_locator: false,
                },
                cancel,
            )
            .unwrap();
        let ordered: Vec<_> = result
            .hits
            .iter()
            .map(|hit| json!([hit.item.object.id().key, hit.score, hit.matched, hit.typo, hit.partial]))
            .collect();
        assert_eq!(result.hits.len(), count, "{query}");
        assert_eq!(result.hits.iter().any(|hit| hit.partial), partial, "{query}");
        assert_eq!(
            Fingerprint::of_bytes(&serde_json::to_vec(&ordered).unwrap()).as_str(),
            expected,
            "{query}: result order or match metadata changed"
        );
    }
}

/// One search and how long it took; a prefix may match nothing.
fn search(engine: &Engine, query: &str, cancel: &Cancellation) -> (Duration, usize) {
    let start = Instant::now();
    let result = engine
        .search(
            &SearchRequest {
                query: query.into(),
                limit: 50,
                libraries: None,
                interpret_locator: false,
            },
            cancel,
        )
        .unwrap();
    (start.elapsed(), result.hits.len())
}

fn cite(engine: &Engine, keys: &[&str], cancel: &Cancellation) -> Duration {
    let request = KeyCitationRequest {
        keys: keys.iter().map(|key| (*key).into()).collect(),
        output: OutputSpec::Csl { style: "apa".into() },
        form: CitationForm::Normal,
        keep_order: false,
        strict: false,
        output_locale: None,
    };
    let start = Instant::now();
    let result = engine.cite_keys(&request, cancel).unwrap();
    let elapsed = start.elapsed();
    assert!(result.copy_content(keys.len()).is_some());
    elapsed
}

/// This process's CPU time and resident memory, read from outside the
/// allocator so the numbers match what an activity monitor shows.
fn process() -> (Duration, u64) {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string("/proc/self/stat").unwrap();
        // Fields after the parenthesised name: utime and stime are 12 and 13.
        let fields: Vec<&str> = stat[stat.rfind(')').unwrap() + 2..].split(' ').collect();
        let ticks: u64 = fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap();
        let status = std::fs::read_to_string("/proc/self/status").unwrap();
        let rss = status.lines().find(|line| line.starts_with("VmRSS:")).unwrap();
        let kib: u64 = rss.split_whitespace().nth(1).unwrap().parse().unwrap();
        // USER_HZ is 100 on every supported Linux.
        (Duration::from_millis(ticks * 10), kib * 1024)
    }
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "cputime=,rss=", "-p", &std::process::id().to_string()])
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let mut parts = text.split_whitespace();
        let cpu = parts.next().unwrap();
        let (minutes, seconds) = cpu.rsplit_once(':').unwrap();
        let minutes: f64 = minutes
            .split(':')
            .fold(0.0, |total, part| total * 60.0 + part.parse::<f64>().unwrap());
        let cpu = Duration::from_secs_f64(minutes * 60.0 + seconds.parse::<f64>().unwrap());
        (cpu, parts.next().unwrap().parse::<u64>().unwrap() * 1024)
    }
    #[cfg(windows)]
    {
        let script = format!(
            "$p = Get-Process -Id {}; '{{0}} {{1}}' -f [int64]$p.TotalProcessorTime.TotalMilliseconds, $p.WorkingSet64",
            std::process::id()
        );
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let out = std::process::Command::new(format!(r"{root}\System32\WindowsPowerShell\v1.0\powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let mut parts = text.split_whitespace();
        let cpu = Duration::from_millis(parts.next().unwrap().parse().unwrap());
        (cpu, parts.next().unwrap().parse().unwrap())
    }
}

#[test]
#[ignore = "run through scripts/perf.mjs, release mode, one case per process"]
fn feel_10k() {
    measure(10_000);
}

#[test]
#[ignore = "run through scripts/perf.mjs, release mode, one case per process"]
fn feel_100k() {
    measure(100_000);
}

fn measure(records: usize) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.bib");
    std::fs::write(&path, library(records, usize::MAX)).unwrap();
    let cancel = Cancellation::default();
    let mut report = serde_json::Map::new();
    report.insert("records".into(), records.into());

    // Starting cold: a new engine reads the library, then the picker's first
    // query and the recently added rows it shows before any typing.
    let start = Instant::now();
    let engine = Engine::new().unwrap().without_file_watches();
    engine
        .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
        .unwrap();
    let open = start.elapsed();
    assert_eq!(engine.libraries().unwrap().libraries[0].objects, records);
    let (first_search, hits) = search(&engine, "avery", &cancel);
    assert_eq!(hits, 50);
    let start = Instant::now();
    let snapshot = engine.snapshot().unwrap();
    let recents = ibis_engine::recents::resolve(&snapshot, &[], true);
    let _ = snapshot.fingerprints().full.as_str();
    let summon = start.elapsed();
    drop(recents);
    report.insert("open_ms".into(), ms(open).into());
    report.insert("first_search_ms".into(), ms(first_search).into());
    report.insert("summon_ms".into(), ms(summon).into());

    // Typing: every prefix of each query is one keystroke's search.
    const QUERIES: [&str; 5] = [
        "researcher42 2001",
        "synthetic signals 9999",
        "morgan7",
        "invented letters",
        "avery",
    ];
    let mut typing = Vec::new();
    for _ in 0..4 {
        for query in QUERIES {
            for end in query
                .char_indices()
                .map(|(index, _)| index)
                .skip(1)
                .chain([query.len()])
            {
                let prefix = &query[..end];
                if !prefix.ends_with(' ') {
                    typing.push(search(&engine, prefix, &cancel).0);
                }
            }
        }
    }
    report.insert("typing_p50_ms".into(), ms(percentile(typing.clone(), 50)).into());
    report.insert("typing_p95_ms".into(), ms(percentile(typing.clone(), 95)).into());
    report.insert("typing_max_ms".into(), ms(percentile(typing.clone(), 100)).into());
    report.insert("typing_n".into(), typing.len().into());
    if records == 100_000 {
        assert_original_ranking(&engine, &cancel);
    }
    report.insert("rss_mib".into(), (process().1 as f64 / 1048576.0).into());

    // The first citation on a formatter nobody warmed, then the one the app
    // gives: warmed at startup, before the person presses Enter.
    let cold = Engine::new().unwrap().without_file_watches();
    let small = directory.path().join("small.bib");
    std::fs::write(&small, library(10, usize::MAX)).unwrap();
    cold.dispatch(Method::Libraries, &json!({"action":"open-file","path":small}), &cancel)
        .unwrap();
    report.insert(
        "first_cite_cold_ms".into(),
        ms(cite(&cold, &["synthetic1"], &cancel)).into(),
    );
    drop(cold);
    let formatting = Formatting::default();
    let start = Instant::now();
    formatting.warm("apa", None).unwrap();
    report.insert("format_warmup_ms".into(), ms(start.elapsed()).into());
    let engine = engine.with_formatting(formatting);
    report.insert(
        "first_cite_ms".into(),
        ms(cite(&engine, &["synthetic1"], &cancel)).into(),
    );
    let warm: Vec<_> = (0..30)
        .map(|_| cite(&engine, &["synthetic2", "synthetic3"], &cancel))
        .collect();
    report.insert("format_p50_ms".into(), ms(percentile(warm.clone(), 50)).into());
    report.insert("format_p95_ms".into(), ms(percentile(warm, 95)).into());
    if std::env::var_os("IBIS_PERF_TRACE").is_some() {
        for keys in [
            &["synthetic2"][..],
            &["synthetic2", "synthetic3"],
            &["synthetic4", "synthetic5"],
        ] {
            let samples: Vec<_> = (0..std::env::var("IBIS_PERF_TRACE").unwrap().parse::<usize>().unwrap())
                .map(|_| ms(cite(&engine, keys, &cancel)))
                .collect();
            eprintln!("trace {keys:?} {samples:?}");
        }
    }

    // Refresh after an edit while copies keep committing.
    let shown = engine
        .dispatch(
            Method::Show,
            &json!({"item":{"by":"key","value":"synthetic1"}}),
            &cancel,
        )
        .unwrap();
    let request: CitationRequest = serde_json::from_value(json!({
        "items": [{"item":shown.data["object"]["data"]["id"],"expected_cite":shown.data["fingerprints"]["cite"]}],
        "output": {"kind":"key","syntax":"pandoc","command":null},
        "keep_order": true,
    }))
    .unwrap();
    let prepared = engine.prepare_citation(&request, &cancel).unwrap();
    std::fs::write(&path, library(records, records / 2)).unwrap();
    let refreshing = AtomicBool::new(true);
    let ready = Barrier::new(2);
    let (refresh, copies) = std::thread::scope(|scope| {
        let copier = scope.spawn(|| {
            let mut samples = vec![];
            ready.wait();
            while refreshing.load(Ordering::Relaxed) {
                let start = Instant::now();
                engine.recite(&prepared, &cancel).unwrap();
                samples.push(start.elapsed());
                std::thread::sleep(Duration::from_millis(1));
            }
            samples
        });
        ready.wait();
        let start = Instant::now();
        let result = engine.dispatch(Method::Libraries, &json!({"action":"reload"}), &cancel);
        let refresh = start.elapsed();
        refreshing.store(false, Ordering::Relaxed);
        result.unwrap();
        (refresh, copier.join().unwrap())
    });
    assert_eq!(search(&engine, "edited synthetic", &cancel).1, 1);
    report.insert("refresh_ms".into(), ms(refresh).into());
    report.insert(
        "copy_during_refresh_max_ms".into(),
        ms(copies.iter().max().copied().unwrap_or_default()).into(),
    );
    report.insert("rss_after_refresh_mib".into(), (process().1 as f64 / 1048576.0).into());

    // Idle: the library open and watched, nothing asked of it.
    let idle_seconds: u64 = std::env::var("IBIS_PERF_IDLE_SECONDS")
        .ok()
        .map_or(10, |value| value.parse().unwrap());
    if idle_seconds > 0 {
        drop(engine);
        let watched = Engine::new().unwrap();
        watched
            .dispatch(Method::Libraries, &json!({"action":"open-file","path":path}), &cancel)
            .unwrap();
        std::thread::sleep(Duration::from_secs(2));
        let (before, _) = process();
        let start = Instant::now();
        std::thread::sleep(Duration::from_secs(idle_seconds));
        let (after, _) = process();
        let share = (after.saturating_sub(before)).as_secs_f64() / start.elapsed().as_secs_f64() * 100.0;
        report.insert("idle_cpu_percent".into(), share.into());
        drop(watched);
    }
    println!("PERF {}", serde_json::Value::Object(report));
}
