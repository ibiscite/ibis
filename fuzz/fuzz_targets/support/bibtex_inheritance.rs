use std::collections::HashSet;

use ibis_core::{bibtex::load_str, model::LoadResult};

fn load(text: &str) -> LoadResult {
    let result = load_str("inheritance.bib", text);
    let mut ids = HashSet::new();
    for reference in &result.references {
        assert!(ids.insert(&reference.id));
        assert_eq!(reference.id.source, "inheritance.bib");
        assert_eq!(reference.citekey.as_deref(), Some(reference.id.native_key.as_str()));
        assert_eq!(reference.to_csl_json()["id"], reference.id.native_key);
    }
    assert!(
        result
            .warnings
            .iter()
            .all(|w| { w.message != "entry could not be read safely (parser error); skipped" }),
        "caught parser panic: {text}"
    );
    result
}

/// Exercise arbitrary source and inherited hostile values, then generate valid
/// graphs so mutations cannot spend the entire run failing at the lexer.
pub fn check(data: &[u8]) {
    let input = String::from_utf8_lossy(data);
    load(&input);
    load(&format!(
        "@xdata{{hostile,note={{{input}}}}}\n@book{{probe,title={{probe}},xdata={{hostile}}}}"
    ));

    let depth = 1 + usize::from(data.first().copied().unwrap_or(0) % 32);
    let flags = data.get(1).copied().unwrap_or(0);
    let cycle = flags & 1 != 0;
    let own = flags & 2 != 0;
    let reverse = flags & 4 != 0;
    let link = if flags & 8 == 0 { "xdata" } else { "crossref" };
    let mut graph = String::new();
    for i in 0..depth {
        let fields = if i + 1 < depth {
            format!("{link}={{node{}}}", i + 1)
        } else if cycle {
            format!("{link}={{NODE0}}")
        } else {
            "publisher={chain press}".to_string()
        };
        graph.push_str(&format!("@xdata{{Node{i},{fields}}}\n"));
    }
    graph.push_str(&format!(
        "@book{{chain,title={{child}},{link}={{node0}}{}}}\n",
        if own { ",publisher={own press}" } else { "" }
    ));

    // Exact case beats folded lookup even when the other spelling came first.
    let parents = [
        "@xdata{Parent,publisher={upper}}\n",
        "@xdata{parent,publisher={lower}}\n",
    ];
    for i in if reverse { [1, 0] } else { [0, 1] } {
        graph.push_str(parents[i]);
    }
    graph.push_str("@proceedings{container,title={container},publisher={cross press}}\n");
    for (key, targets) in [
        ("upper", "Parent"),
        ("lower", "parent"),
        ("folded", "PARENT"),
        ("last", "Parent,parent,Parent"),
    ] {
        graph.push_str(&format!(
            "@inproceedings{{{key},title={{child}},xdata={{{targets}}},crossref={{container}}}}\n"
        ));
    }
    let loaded = load(&graph);
    assert_eq!(loaded.references.len(), 6, "xdata helpers must stay hidden: {graph}");
    let find = |key: &str| {
        loaded
            .references
            .iter()
            .find(|r| r.id.native_key == key)
            .unwrap_or_else(|| panic!("lost {key}: {graph}"))
    };
    let publisher = |key: &str| find(key).variables.get("publisher").map(String::as_str);
    assert_eq!(find("chain").title_plain().as_deref(), Some("child"));
    assert_eq!(
        publisher("chain"),
        if own {
            Some("own press")
        } else if !cycle && depth <= 16 {
            Some("chain press")
        } else {
            None
        },
        "depth/cycle/own-field precedence: {graph}"
    );
    assert_eq!(publisher("upper"), Some("upper"));
    assert_eq!(publisher("lower"), Some("lower"));
    assert_eq!(publisher("folded"), Some(if reverse { "lower" } else { "upper" }));
    assert_eq!(publisher("last"), Some("upper"));
    for key in ["upper", "lower", "folded", "last"] {
        assert_eq!(find(key).title_plain().as_deref(), Some("child"));
        assert_eq!(find(key).container_title.as_deref(), Some("container"));
    }
    if cycle || depth > 16 {
        let expected = if depth <= 16 { "cycle" } else { "deeper than 16" };
        assert!(
            loaded
                .warnings
                .iter()
                .any(|w| w.key.as_deref() == Some("chain") && w.message.contains(expected)),
            "missing graph diagnostic: {graph}"
        );
    } else {
        assert!(loaded.warnings.is_empty(), "unexpected warnings: {:?}", loaded.warnings);
    }
    // Repeat a load with a fresh resolver/cache and randomized hash state.
    let again = load(&graph);
    assert_eq!(loaded.references, again.references);
    assert_eq!(loaded.warnings, again.warnings);
}
