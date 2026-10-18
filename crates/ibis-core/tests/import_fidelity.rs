//! Regression tests for import correctness: each case once lost or changed
//! bibliographic meaning.

use ibis_core::bibtex::load_str;
use ibis_core::model::{LoadResult, Reference};

fn load(src: &str) -> LoadResult {
    load_str("t", src)
}

fn only(result: &LoadResult, key: &str) -> Reference {
    result
        .references
        .iter()
        .find(|r| r.id.native_key == key)
        .cloned()
        .unwrap_or_else(|| panic!("`{key}` not loaded; warnings: {:?}", result.warnings))
}

fn warned(result: &LoadResult, needle: &str) -> bool {
    result.warnings.iter().any(|w| w.message.contains(needle))
}

#[test]
fn quoted_parentheses_keep_the_record() {
    let r = load("@book(q, title = \"Notes (draft) on sleep\", year = 2020)\n@book{after, title={After}}");
    assert_eq!(only(&r, "q").title_plain().as_deref(), Some("Notes (draft) on sleep"));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn indented_records_after_damage_are_kept() {
    let r = load("@article{broken, title={Unclosed}\n  @article{indented, title={Indented}}\n");
    only(&r, "indented");
    assert_eq!(r.warnings.len(), 1);
}

#[test]
fn chained_macros_keep_every_part() {
    let r = load("@string{a = \"Alpha\"}\n@string{b = a # \" Beta\"}\n@article{m, title={T}, journal = b}");
    assert_eq!(only(&r, "m").container_title.as_deref(), Some("Alpha Beta"));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn nested_inheritance_reaches_the_grandparent() {
    let r = load(
        "@proceedings{c, title={Grand}, year={2019}, publisher={P}}\n@proceedings{b, title={Mid}, crossref={c}}\n@inproceedings{a, title={Leaf}, crossref={b}}",
    );
    let a = only(&r, "a");
    assert_eq!(a.year(), Some(2019));
    assert_eq!(a.variables.get("publisher").map(String::as_str), Some("P"));
}

#[test]
fn missing_cross_reference_target_is_reported() {
    let r = load("@inproceedings{a, title={T}, crossref={nowhere}}");
    only(&r, "a");
    assert!(warned(&r, "nowhere"));
}

#[test]
fn equivalent_dates_in_any_script_give_the_same_result() {
    let latin = load("@article{d, title={T}, year={2020}, month=mar}");
    let bengali = load("@article{d, title={T}, year={২০২০}, month=mar}");
    assert_eq!(only(&latin, "d").issued, only(&bengali, "d").issued);
    assert_eq!(only(&bengali, "d").issued.and_then(|i| i.start.month), Some(3));
}

#[test]
fn invalid_or_unreadable_dates_are_reported_not_silently_changed() {
    let r = load(
        "@book{feb, title={T}, date={2021-02-31}}\n@book{junk, title={T}, date={2020-13-45x}}\n@book{none, title={T}, date={sometime}}",
    );
    assert_eq!(
        only(&r, "feb").issued.map(|i| (i.start.month, i.start.day)),
        Some((Some(2), None))
    );
    assert_eq!(only(&r, "junk").year(), Some(2020));
    assert_eq!(only(&r, "none").issued, None);
    assert_eq!(r.warnings.len(), 3, "{:?}", r.warnings);
}

#[test]
fn ranges_and_approximate_dates_are_kept() {
    let r = load("@book{range, title={T}, date={2019/2021}}\n@book{circa, title={T}, date={2020-03~}}");
    let range = only(&r, "range").issued.unwrap_or_else(|| panic!("no date"));
    assert_eq!(range.end.map(|e| e.year), Some(2021));
    assert!(only(&r, "circa").issued.is_some_and(|i| i.circa));
}

#[test]
fn and_others_marks_a_truncated_list_instead_of_an_author() {
    let r = only(&load("@article{o, author={Smith, Alex and others}, title={T}}"), "o");
    assert_eq!(r.authors.len(), 2);
    assert!(r.authors[1].is_others());
    assert_eq!(r.authors[1].display(), "et al.");
    let csl = r.to_csl_json();
    assert_eq!(csl["author"][1]["literal"], "others");
}

#[test]
fn organizations_are_literal_names_and_particles_are_kept() {
    let r = only(
        &load("@book{n, author={{World Health Organization} and van der Berg, Jan and Plato}, title={T}}"),
        "n",
    );
    assert_eq!(r.authors[0].literal.as_deref(), Some("World Health Organization"));
    assert_eq!(r.authors[1].dropping_particle.as_deref(), Some("van der"));
    assert_eq!(r.authors[2].family.as_deref(), Some("Plato"));
}

#[test]
fn language_names_become_bcp47_tags() {
    let r = load(
        "@book{us, title={T}, langid={american}}\n@book{de, title={T}, langid={ngerman}}\n@book{tag, title={T}, langid={pt-BR}}\n@book{odd, title={T}, langid={klingonese}}",
    );
    assert_eq!(only(&r, "us").language.as_deref(), Some("en-US"));
    assert_eq!(only(&r, "de").language.as_deref(), Some("de-DE"));
    assert_eq!(only(&r, "tag").language.as_deref(), Some("pt-BR"));
    assert_eq!(only(&r, "odd").language, None);
    assert!(warned(&r, "klingonese"));
}

#[test]
fn protected_capitalization_survives_into_csl() {
    let r = only(
        &load("@article{p, title={Using {iPhone} Apps for {NASA} Missions}, langid={american}}"),
        "p",
    );
    assert_eq!(
        r.title.as_deref(),
        Some("Using <span class=\"nocase\">iPhone</span> apps for <span class=\"nocase\">NASA</span> missions")
    );
    assert_eq!(r.title_plain().as_deref(), Some("Using iPhone apps for NASA missions"));
}

#[test]
fn non_english_titles_keep_their_capitalization() {
    let r = only(
        &load("@article{g, title={Schlaf und Gedächtnis}, langid={ngerman}}"),
        "g",
    );
    assert_eq!(r.title.as_deref(), Some("Schlaf und Gedächtnis"));
}

#[test]
fn large_libraries_load_in_linear_time() {
    let mut src = String::from("@string{j = \"Journal\"}\n@proceedings{conf, title={Conf}, year={2020}}\n");
    for i in 0..5_000 {
        src.push_str(&format!(
            "@inproceedings{{k{i}, title={{Paper {i}}}, crossref={{conf}}, booktitle = j}}\n"
        ));
    }
    let start = std::time::Instant::now();
    let r = load(&src);
    assert_eq!(r.references.len(), 5_001);
    assert!(start.elapsed().as_secs() < 20, "took {:?}", start.elapsed());
}

#[test]
fn shared_ancestors_are_not_cycles() {
    // a inherits from b and c (via crossref and xdata); both inherit from d.
    let r = load(
        "@book{d, title={D}, publisher={Shared}, year={2001}}\n@book{b, title={B}, crossref={d}}\n@book{c, title={C}, xdata={d}}\n@book{a, title={A}, crossref={b}, xdata={c}}",
    );
    assert!(!warned(&r, "cycle"), "{:?}", r.warnings);
    let a = only(&r, "a");
    assert_eq!(a.variables.get("publisher").map(String::as_str), Some("Shared"));
    assert_eq!(a.year(), Some(2001));
}

#[test]
fn escaped_quotes_inside_quoted_values_keep_the_record() {
    let r = load("@book(q, title = \"G\\\"odel (test)\", year = 2020)\n@book{after, title={After}}");
    assert!(only(&r, "q").title_plain().is_some_and(|t| t.contains("del (test)")));
    only(&r, "after");
}

#[test]
fn tex_symbols_and_math_are_kept_not_dropped() {
    let r = load(
        "@article{m, title={The \\ensuremath{\\alpha}-Helix and $\\beta$ Sheets at 37\\,\\textdegree C}, langid={american}}",
    );
    let title = only(&r, "m").title_plain().unwrap_or_default();
    assert!(
        title.contains('α') && title.contains('β') && title.contains('°'),
        "{title}"
    );
}

#[test]
fn unknown_tex_commands_are_reported_not_silently_dropped() {
    let r = load("@article{u, title={A \\frobnicate Title}}");
    assert!(only(&r, "u").title_plain().is_some_and(|t| t.contains("frobnicate")));
    assert!(warned(&r, "frobnicate"));
}

#[test]
fn subtitles_are_part_of_the_title() {
    let r = load(
        "@book{s, title={Sleep}, subtitle={A Very Short Introduction}, langid={american}}\n@article{j, title={T}, journaltitle={Memory}, journalsubtitle={Theory and Practice}}",
    );
    assert_eq!(
        only(&r, "s").title_plain().as_deref(),
        Some("Sleep: A very short introduction")
    );
    assert_eq!(
        only(&r, "j").container_title.as_deref(),
        Some("Memory: Theory and Practice")
    );
}

#[test]
fn uncertain_and_open_ended_dates_are_reported() {
    let r = load(
        "@book{u, title={T}, date={2020?}}\n@book{before, title={T}, date={../2020}}\n@book{after, title={T}, date={2020/..}}",
    );
    for key in ["u", "before", "after"] {
        assert!(
            r.warnings.iter().any(|w| w.key.as_deref() == Some(key)),
            "{key}: {:?}",
            r.warnings
        );
    }
}

#[test]
fn exponential_macro_expansion_is_bounded() {
    let mut src = String::from("@string{m0 = \"xxxxxxxxxxxxxxxx\"}\n");
    for i in 1..=24 {
        src.push_str(&format!("@string{{m{i} = m{} # m{}}}\n", i - 1, i - 1));
    }
    src.push_str("@article{boom, title = m24}\n");
    let start = std::time::Instant::now();
    let r = load(&src);
    assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    let title_len = only(&r, "boom").title.map_or(0, |t| t.len());
    assert!(title_len < 1_000_000, "expanded to {title_len} bytes");
    assert!(warned(&r, "too large"), "{:?}", r.warnings);
}

#[test]
fn block_splitting_scales_linearly() {
    let build = |n: usize| {
        (0..n)
            .map(|i| format!("@misc{{k{i}, title={{T{i}}}}}\n"))
            .collect::<String>()
    };
    let time = |src: &str| {
        (0..3)
            .map(|_| {
                let start = std::time::Instant::now();
                let _ = load(src);
                start.elapsed()
            })
            .min()
            .unwrap_or_default()
    };
    let small = time(&build(2_000));
    let large = time(&build(20_000));
    // Ten times the input may take at most ~25x the time; quadratic growth would be ~100x.
    assert!(large < small * 25, "2k: {small:?}, 20k: {large:?}");
}

#[test]
fn xdata_helpers_supply_fields_but_are_not_references() {
    let r = load(
        "@xdata{shared, publisher={Synthetic Press}, date={2024}}\n@book{work, author={Example, Ada}, title={Synthetic Book}, xdata={shared}}",
    );
    assert_eq!(
        r.references.len(),
        1,
        "{:?}",
        r.references.iter().map(|x| &x.id.native_key).collect::<Vec<_>>()
    );
    let work = only(&r, "work");
    assert_eq!(
        work.variables.get("publisher").map(String::as_str),
        Some("Synthetic Press")
    );
    assert_eq!(work.year(), Some(2024));
}

#[test]
fn a_short_path_still_reaches_ancestors_cut_off_on_a_long_path() {
    let mut src = String::from("@book{a,crossref={c1},xdata={shared}}\n");
    for i in 1..15 {
        src.push_str(&format!("@book{{c{i},crossref={{c{}}}}}\n", i + 1));
    }
    src.push_str("@book{c15,crossref={shared}}\n@book{shared,crossref={grand}}\n@book{grand,year={2024}}\n");
    assert_eq!(only(&load(&src), "a").year(), Some(2024));
}

#[test]
fn translators_and_other_roles_reach_csl() {
    let r = only(
        &load(
            "@book{t, author={Tolstoy, Leo}, translator={Garnett, Constance}, bookauthor={Editor, Book}, title={War and Peace}}",
        ),
        "t",
    );
    let csl = r.to_csl_json();
    assert_eq!(csl["translator"][0]["family"], "Garnett");
    assert_eq!(csl["container-author"][0]["family"], "Editor");
}

#[test]
fn math_superscripts_and_subscripts_are_kept() {
    let r = only(
        &load("@article{m, title={Energy as $mc^2$ and $H_{2}O$}, langid={american}}"),
        "m",
    );
    let title = r.title.unwrap_or_default();
    assert!(title.contains("mc<sup>2</sup>"), "{title}");
    assert!(title.contains("H<sub>2</sub>O"), "{title}");
}

#[test]
fn errors_in_inherited_xdata_fields_are_reported_on_the_consumer() {
    let r = load("@xdata{common,publisher=missing}\n@book{a,title={T},xdata={common}}");
    only(&r, "a");
    assert!(
        r.warnings
            .iter()
            .any(|w| w.key.as_deref() == Some("a") && w.message.contains("missing")),
        "{:?}",
        r.warnings
    );
}

#[test]
fn access_and_original_dates_reach_csl() {
    let r = only(
        &load("@online{web,title={Live Status},urldate={2024-05-06},origdate={1999},url={https://example.org/}}"),
        "web",
    );
    let csl = r.to_csl_json();
    assert_eq!(csl["accessed"]["date-parts"][0], serde_json::json!([2024, 5, 6]));
    assert_eq!(csl["original-date"]["date-parts"][0], serde_json::json!([1999]));
}

#[test]
fn newspaper_and_magazine_articles_keep_their_type() {
    let r = load(
        "@article{news,entrysubtype={newspaper},title={H},journaltitle={Example News},date={2020-03-10}}\n@article{mag,entrysubtype={magazine},title={M},journaltitle={Example Monthly},date={2020-04}}",
    );
    assert_eq!(only(&r, "news").item_type, "article-newspaper");
    assert_eq!(only(&r, "mag").item_type, "article-magazine");
}

#[test]
fn comments_between_fields_do_not_lose_the_record() {
    let r = load("@book{a,title={T}, % a comment\n  year=2024}");
    assert_eq!(only(&r, "a").year(), Some(2024));
}

#[test]
fn explicit_child_dates_win_over_inherited_dates() {
    let r = load("@xdata{shared,year=2000}\n@book{a,title={T},date={2024},xdata={shared}}");
    assert_eq!(only(&r, "a").year(), Some(2024));
}

#[test]
fn escaped_braces_are_literal_text() {
    let r = load("@book{a,title={The Set \\{x,y\\}}, publisher={A \\{B\\} Press}, langid={american}}");
    let a = only(&r, "a");
    assert_eq!(a.title_plain().as_deref(), Some("The set {x,y}"));
    assert_eq!(a.variables.get("publisher").map(String::as_str), Some("A {B} Press"));
}

#[test]
fn unknown_commands_with_arguments_stay_visible() {
    let r = load("@book{a,title={The Ratio $\\frac{1}{2}$}}");
    let title = only(&r, "a").title_plain().unwrap_or_default();
    assert!(title.contains("\\frac{1}{2}"), "{title}");
    assert!(warned(&r, "frac"));
}

#[test]
fn repeated_and_fanned_out_inheritance_is_bounded() {
    let mut src = String::from("@book{r,title={T},xdata={a}}\n@xdata{a,xdata={b,b,b,b,b,b,b,b}}\n");
    for (x, y) in [("b", "c"), ("c", "d"), ("d", "e"), ("e", "f"), ("f", "g"), ("g", "h")] {
        src.push_str(&format!("@xdata{{{x},xdata={{{y},{y},{y},{y},{y},{y},{y},{y}}}}}\n"));
    }
    src.push_str("@xdata{h,year=2024}\n");
    // Distinct parents fanning out at every level (8 x 8 x 8 paths).
    src.push_str("@book{wide,title={W},xdata={w0,w1,w2,w3,w4,w5,w6,w7}}\n");
    for i in 0..8 {
        src.push_str(&format!("@xdata{{w{i},xdata={{v0,v1,v2,v3,v4,v5,v6,v7}}}}\n"));
        src.push_str(&format!("@xdata{{v{i},xdata={{u0,u1,u2,u3,u4,u5,u6,u7}}}}\n"));
        src.push_str(&format!("@xdata{{u{i},publisher={{P}}}}\n"));
    }
    let start = std::time::Instant::now();
    let r = load(&src);
    assert!(start.elapsed().as_secs() < 5, "took {:?}", start.elapsed());
    assert_eq!(only(&r, "r").year(), Some(2024));
    assert_eq!(
        only(&r, "wide").variables.get("publisher").map(String::as_str),
        Some("P")
    );
}

#[test]
fn later_xdata_parents_override_earlier_ones() {
    let r = load(
        "@xdata{first,publisher={First},year={2000}}\n@xdata{second,publisher={Second},year={2024}}\n@book{a,title={Synthetic},xdata={first,second}}",
    );
    let a = only(&r, "a");
    assert_eq!(a.variables.get("publisher").map(String::as_str), Some("Second"));
    assert_eq!(a.year(), Some(2024));
}

#[test]
fn inherited_dates_merge_component_by_component() {
    let r =
        load("@book{parent,title={Parent},date={2024-02-05}}\n@book{child,title={Child},month=mar,crossref={parent}}");
    let issued = only(&r, "child")
        .issued
        .unwrap_or_else(|| panic!("no date: {:?}", r.warnings));
    assert_eq!(
        (issued.start.year, issued.start.month, issued.start.day),
        (2024, Some(3), Some(5))
    );
    let month_only = load("@xdata{p,year=2024}\n@book{a,title={T},month=mar,xdata={p}}");
    let issued = only(&month_only, "a").issued.unwrap_or_else(|| panic!("no date"));
    assert_eq!((issued.start.year, issued.start.month), (2024, Some(3)));
}

#[test]
fn intermediate_entries_keep_their_own_dates() {
    let r = load("@book{g,year=2000}\n@book{p,date={2024},crossref={g}}\n@book{a,title={T},crossref={p}}");
    assert_eq!(only(&r, "p").year(), Some(2024));
    assert_eq!(only(&r, "a").year(), Some(2024));
}

#[test]
fn multi_volume_titles_keep_the_whole_work() {
    let r = load(
        "@book{volume,title={Volume One},maintitle={Collected Works},volume={2},year={2020},langid={american}}\n@inbook{part,title={A Letter},booktitle={Early Letters},maintitle={Collected Works},volume={2},year={2020}}",
    );
    let v = only(&r, "volume");
    assert_eq!(v.title_plain().as_deref(), Some("Collected works"));
    assert_eq!(v.variables.get("volume-title").map(String::as_str), Some("Volume one"));
    let part = only(&r, "part");
    assert_eq!(part.container_title.as_deref(), Some("Collected Works"));
    assert_eq!(
        part.variables.get("volume-title").map(String::as_str),
        Some("Early Letters")
    );
}

#[test]
fn series_numbers_and_report_numbers_keep_their_meaning() {
    let r = load(
        "@book{b,title={Work},series={Example Series},number={12},year={2020}}\n@report{rep,title={R},number={TR-7},institution={I},series={Tech Series},year={2020}}",
    );
    assert_eq!(
        only(&r, "b").variables.get("collection-number").map(String::as_str),
        Some("12")
    );
    assert_eq!(only(&r, "b").variables.get("number"), None);
    assert_eq!(
        only(&r, "rep").variables.get("number").map(String::as_str),
        Some("TR-7")
    );
}

#[test]
fn legacy_conference_alias_inherits_its_proceedings_title() {
    let r = load(
        "@proceedings{volume,title={Synthetic Proceedings},year={2024}}\n@conference{paper,title={Synthetic Paper},crossref={volume}}",
    );
    let paper = only(&r, "paper");
    assert_eq!(paper.item_type, "paper-conference");
    assert_eq!(paper.container_title.as_deref(), Some("Synthetic Proceedings"));
}

#[test]
fn private_use_characters_in_text_are_not_mistaken_for_braces() {
    let r = load("@book{pua,title={A \u{E000} and \u{E001} test \\{x\\}},year={2024}}");
    let title = only(&r, "pua").title.unwrap_or_default();
    assert!(
        title.contains('\u{E000}') && title.contains('\u{E001}') && title.contains("{x}"),
        "{title:?}"
    );
}

#[test]
fn comment_delimiters_and_protected_escaped_braces() {
    let r = load("@book{a,title={T}, % }\n  year=2024}\n@book{b,title={{\\{B\\}}}}");
    assert_eq!(only(&r, "a").year(), Some(2024));
    assert_eq!(only(&r, "b").title_plain().as_deref(), Some("{B}"));
}

#[test]
fn unknown_commands_across_chunks_are_kept_exactly() {
    let r = load("@book{a,title={\\frac{1}{2}}}\n@book{b,title={$\\frac{1}{2^3}$}}");
    assert_eq!(only(&r, "a").title_plain().as_deref(), Some("\\frac{1}{2}"));
    assert_eq!(only(&r, "b").title_plain().as_deref(), Some("$\\frac{1}{2^3}$"));
}

#[test]
fn inheritance_precedence_survives_large_graphs() {
    let mut src =
        String::from("@book{r,xdata={n0,n1,n2,n3,n4,n5,n6,n7,n8}}\n@xdata{n0,xdata={n2,n1,n3,n4,n5,n6,n7,n8}}\n");
    src.push_str(
        "@xdata{n1,publisher={B},xdata={n2,n3,n4,n5,n6,n7,n8}}\n@xdata{n2,publisher={G},xdata={n3,n4,n5,n6,n7,n8}}\n",
    );
    for i in 3..8 {
        let rest: Vec<String> = ((i + 1)..9).map(|j| format!("n{j}")).collect();
        src.push_str(&format!("@xdata{{n{i},xdata={{{}}}}}\n", rest.join(",")));
    }
    src.push_str("@xdata{n8}\n");
    // Later xdata parents win: n8..n2 are listed after n1, and n2 sets G.
    let r = only(&load(&src), "r");
    assert_eq!(r.variables.get("publisher").map(String::as_str), Some("G"));
}

#[test]
fn chapter_authors_are_both_author_and_book_author() {
    let r = load("@book{p,title={Book},author={Doe, Jane}}\n@inbook{c,title={Chapter},crossref={p}}");
    let c = only(&r, "c").to_csl_json();
    assert_eq!(c["author"][0]["family"], "Doe");
    assert_eq!(c["container-author"][0]["family"], "Doe");
}

#[test]
fn an_own_date_is_never_given_inherited_precision() {
    let r =
        load("@book{p,date={2000-02-05}}\n@book{c,date={2024},crossref={p}}\n@book{d,title={T},date={2024},month=mar}");
    for key in ["c", "d"] {
        let issued = only(&r, key).issued.unwrap_or_else(|| panic!("{key}: no date"));
        assert_eq!(
            (issued.start.year, issued.start.month, issued.start.day),
            (2024, None, None),
            "{key}"
        );
    }
}

#[test]
fn escaped_backslash_before_a_group_keeps_the_record() {
    let r = load("@book{c,title={A \\\\{B} C}}");
    only(&r, "c");
}

#[test]
fn periodical_title_addon_stays_with_the_journal() {
    let r = load("@periodical{p,title={Journal},titleaddon={Annex}}\n@article{c,title={Paper},crossref={p}}");
    let c = only(&r, "c");
    assert_eq!(c.title_plain().as_deref(), Some("Paper"));
    assert_eq!(c.container_title.as_deref(), Some("Journal. Annex"));
}

#[test]
fn multivolume_mappings_apply_only_to_their_entry_types() {
    let r = load("@mvbook{p,title={Whole}}\n@article{c,title={Paper},crossref={p}}");
    let c = only(&r, "c");
    assert_eq!(c.title_plain().as_deref(), Some("Paper"));
    assert_eq!(c.variables.get("volume-title"), None);
}

#[test]
fn private_use_characters_survive_the_unknown_command_fallback() {
    let r = load("@book{c,title={\\foo \u{E000}x\u{E001}\u{E002}}}");
    let title = only(&r, "c").title.unwrap_or_default();
    assert!(
        title.contains("\\foo")
            && title.contains('\u{E000}')
            && title.contains('\u{E001}')
            && title.contains('\u{E002}'),
        "{title:?}"
    );
}

#[test]
fn duplicate_fields_keep_the_last_value_and_warn() {
    let r = load("@book{c,title={First},title={Second}}");
    assert_eq!(only(&r, "c").title_plain().as_deref(), Some("Second"));
    assert!(warned(&r, "twice"), "{:?}", r.warnings);
}

#[test]
fn a_repeated_xdata_parent_counts_at_its_last_position() {
    let r = load(
        "@xdata{first,publisher={First}}\n@xdata{second,publisher={Second}}\n@book{a,title={T},xdata={first,second,first}}",
    );
    assert_eq!(
        only(&r, "a").variables.get("publisher").map(String::as_str),
        Some("First")
    );
}

#[test]
fn short_titles_reach_csl() {
    let r = only(
        &load("@book{a,title={A Very Long Title},shorttitle={Short},year={2020}}"),
        "a",
    );
    assert_eq!(r.to_csl_json()["title-short"], "Short");
}

#[test]
fn textbackslash_is_literal_in_titles_and_plain_fields() {
    let r = load(
        r"@book{k,title={{A \textbackslash{} B}},subtitle={C \textbackslash{} D},
        shorttitle={{E \textbackslash{} F}},journaltitle={G \textbackslash{} H},
        publisher={I \textbackslash{} J},abstract={K \textbackslash{} L},
        author={{M \textbackslash{} N}},url={https://example.org/\textbackslash},langid={ja}}",
    );
    let a = only(&r, "k");
    assert_eq!(a.title_plain().as_deref(), Some(r"A \ B: C \ D"));
    assert_eq!(a.to_csl_json()["title-short"], r#"<span class="nocase">E \ F</span>"#);
    assert_eq!(a.container_title.as_deref(), Some(r"G \ H"));
    assert_eq!(a.variables.get("publisher").map(String::as_str), Some(r"I \ J"));
    assert_eq!(a.abstract_text.as_deref(), Some(r"K \ L"));
    assert_eq!(a.authors[0].literal.as_deref(), Some(r"M \ N"));
    assert_eq!(a.url.as_deref(), Some(r"https://example.org/\textbackslash"));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn literal_backslashes_are_not_commands_or_placeholder_collisions() {
    let r = load(
        "@string{slash = {\\textbackslash{}}}\n@xdata{p,publisher=slash # {alpha}}\n\
         @book{k,title={A \\textbackslash{}alpha \\textbackslash{}unknown \\textbackslash{}\\textbackslash{} \\{B\\} \u{E003}},xdata={p},langid={ja}}\n\
         @book{u,title={\\textbackslash{}alpha \\unknown \u{E003}},langid={ja}}\n\
         @book{space,title={A \\textbackslash B},langid={ja}}\n\
         @book{long,title={\\textbackslashsuffix},langid={ja}}",
    );
    let k = only(&r, "k");
    assert_eq!(
        k.title_plain().as_deref(),
        Some("A \\alpha \\unknown \\\\ {B} \u{E003}")
    );
    assert_eq!(k.variables.get("publisher").map(String::as_str), Some(r"\alpha"));
    assert_eq!(
        only(&r, "u").title_plain().as_deref(),
        Some("\\textbackslash{}alpha \\unknown \u{E003}")
    );
    assert_eq!(only(&r, "long").title_plain().as_deref(), Some(r"\textbackslashsuffix"));
    assert_eq!(only(&r, "space").title_plain().as_deref(), Some(r"A \B"));
    assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
}
