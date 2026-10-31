//! Bibliographic meaning the BibTeX reader must keep: casing, names, macros,
//! inheritance, containers and import fidelity. Synthetic data only.
//!
//! Each case once lost or changed meaning in the earlier loader; the same
//! rules are asserted here on the CSL that the shared file loader publishes.

#[path = "support/bib.rs"]
mod bib;

use bib::{csl, date, display, keys, load, month, name, plain, record, text, warned_at, year};
use ibis_sources::WarningCode;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

// --- language tags and stored title case (#331, #1530) --------------------

#[test]
fn english_language_tag_case_does_not_change_title_conversion() {
    let loaded =
        load("@book{a,title={The Second Title},langid={EN-US}}\n@book{b,title={The Second Title},langid={en-US}}");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(keys(&loaded), ["a", "b"]);
    assert_eq!(plain(&loaded, "a", "title").as_deref(), Some("The Second Title"));
    assert_eq!(plain(&loaded, "b", "title").as_deref(), Some("The Second Title"));
}

#[test]
fn mixed_case_primary_tags_keep_language_spelling_and_non_english_titles() {
    for (tag, expected) in [
        ("EN", "The Second Title"),
        ("En-Latn-US", "The Second Title"),
        ("eN-gB", "The Second Title"),
        ("DE-de", "The Second Title"),
        ("enm", "The Second Title"),
    ] {
        let loaded = load(&format!("@book{{a,title={{The Second Title}},language={{{tag}}}}}"));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(keys(&loaded), ["a"]);
        assert_eq!(plain(&loaded, "a", "title").as_deref(), Some(expected), "{tag}");
        assert_eq!(text(&loaded, "a", "language"), Some(tag));
    }
}

#[test]
fn language_names_become_bcp47_tags() {
    let r = load(
        "@book{us, title={T}, langid={american}}\n@book{de, title={T}, langid={ngerman}}\n@book{tag, title={T}, langid={pt-BR}}\n@book{odd, title={T}, langid={klingonese}}",
    );
    assert_eq!(text(&r, "us", "language"), Some("en-US"));
    assert_eq!(text(&r, "de", "language"), Some("de-DE"));
    assert_eq!(text(&r, "tag", "language"), Some("pt-BR"));
    assert_eq!(text(&r, "odd", "language"), Some("und"));
    assert_eq!(warned_at(&r, WarningCode::Field, 4), 1, "{:?}", r.warnings);
}

#[test]
fn protected_capitalization_survives_into_csl() {
    let r = load("@article{p, title={Using {iPhone} Apps for {NASA} Missions}, langid={american}}");
    assert_eq!(
        text(&r, "p", "title"),
        Some("Using <span class=\"nocase\">iPhone</span> Apps for <span class=\"nocase\">NASA</span> Missions")
    );
    assert_eq!(
        plain(&r, "p", "title").as_deref(),
        Some("Using iPhone Apps for NASA Missions")
    );
}

#[test]
fn non_english_titles_keep_their_capitalization() {
    let r = load("@article{g, title={Schlaf und Gedächtnis}, langid={ngerman}}");
    assert_eq!(text(&r, "g", "title"), Some("Schlaf und Gedächtnis"));
}

#[test]
fn legacy_hyphenation_is_the_langid_fallback_and_unknown_languages_are_undetermined() {
    for (fields, language) in [
        ("hyphenation={german}", "de-DE"),
        ("hyphenation={german},language={english}", "de-DE"),
        ("langid={english},hyphenation={german}", "en-US"),
        ("language={Deutsch}", "de-DE"),
        ("langid={},hyphenation={german}", "de-DE"),
        ("language={unreadable-language}", "und"),
    ] {
        let loaded = load(&format!("@book{{a,title={{Invented German Nouns}},{fields}}}"));
        assert_eq!(text(&loaded, "a", "language"), Some(language), "{fields}");
        assert_eq!(text(&loaded, "a", "title"), Some("Invented German Nouns"));
    }
}

// --- names -----------------------------------------------------------------

#[test]
fn explicit_useprefix_preserves_non_dropping_particles_in_every_name_role() {
    let loaded = load(
        "@book{explicit, title={Synthetic Work},
         author={family=Beethoven, given=Ludwig, prefix=van, useprefix=true},
         editor={family=Gogh, given=Vincent, prefix=van, useprefix=true},
         translator={family=Meer, given=Jan, prefix=van der, useprefix=true}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(keys(&loaded), ["explicit"]);
    let csl = csl(&loaded, "explicit");
    for (role, particle) in [("author", "van"), ("editor", "van"), ("translator", "van der")] {
        assert_eq!(csl[role][0]["non-dropping-particle"], particle, "{role}: {csl:?}");
        assert!(csl[role][0].get("dropping-particle").is_none(), "{role}: {csl:?}");
    }
    assert_eq!(display(&csl["author"][0]), "Ludwig van Beethoven");
}

#[test]
fn false_and_unspecified_useprefix_keep_existing_dropping_particle_behavior() {
    let loaded = load(
        "@book{explicit, title={Synthetic Work},
         author={family=Beethoven, given=Ludwig, prefix=van, useprefix=false}}
         @book{unspecified, title={Synthetic Work},
         author={family=Beethoven, given=Ludwig, prefix=van}}
         @book{traditional, title={Synthetic Work}, author={van Beethoven, Ludwig}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(keys(&loaded), ["explicit", "traditional", "unspecified"]);
    for key in ["explicit", "unspecified", "traditional"] {
        let csl = csl(&loaded, key);
        assert_eq!(csl["author"][0]["dropping-particle"], "van", "{csl:?}");
        assert!(csl["author"][0].get("non-dropping-particle").is_none(), "{csl:?}");
    }
}

#[test]
fn decomposed_korean_names_keep_family_first_without_rewriting_name_text() {
    for (family, given) in [
        ("김", "민"),
        ("\u{1100}\u{1175}\u{11b7}", "\u{1106}\u{1175}\u{11ab}"),
        ("김", "\u{1106}\u{1175}\u{11ab}"),
        ("\u{1100}\u{1175}\u{11b7}", "민"),
    ] {
        let displayed = display(&json!({"family": family, "given": given}));
        assert_eq!(displayed, format!("{family}{given}"));
        assert_eq!(displayed.nfc().collect::<String>(), "김민");
    }
}

#[test]
fn korean_family_names_with_latin_given_names_keep_existing_reading_order() {
    for family in ["김", "\u{1100}\u{1175}\u{11b7}"] {
        assert_eq!(
            display(&json!({"family": family, "given": "Alex"})),
            format!("Alex {family}")
        );
    }
}

#[test]
fn bibtex_decomposed_korean_names_preserve_their_reading_order() {
    let loaded = load(
        "@book{composed, title={Synthetic Work}, author={김, 민}}
         @book{decomposed, title={Synthetic Work}, author={\u{1100}\u{1175}\u{11b7}, \u{1106}\u{1175}\u{11ab}}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(keys(&loaded), ["composed", "decomposed"]);
    for key in ["composed", "decomposed"] {
        let authors = csl(&loaded, key)["author"].as_array().cloned().unwrap_or_default();
        assert_eq!(authors.len(), 1);
        let shown = display(&authors[0]).nfc().collect::<String>();
        assert_eq!(shown, "김민", "{key}");
    }
}

#[test]
fn and_others_marks_a_truncated_list_instead_of_an_author() {
    let r = load("@article{o, author={Smith, Alex and others}, title={T}}");
    let authors = csl(&r, "o")["author"].as_array().cloned().unwrap_or_default();
    assert_eq!(authors.len(), 2);
    assert_eq!(authors[1], json!({"literal": "others"}));
    assert!(name(&authors[1]).is_others());
    assert_eq!(display(&authors[1]), "et al.");
}

#[test]
fn fixture_names_keep_family_first_particles_and_suffixes_in_display() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bibtex/real-world.bib");
    let loaded = ibis_sources::load_file(&path).unwrap_or_else(|e| panic!("{e}"));
    for (key, expected) in [
        ("chen2022", vec!["陈伟", "山田太郎"]),
        (
            "vanderberg2019",
            vec!["Jan van der Berg", "John Smith Jr.", "Mary-Kate O'Neil"],
        ),
    ] {
        let authors = csl(&loaded, key)["author"]
            .as_array()
            .unwrap_or_else(|| panic!("{key}: no authors"));
        assert_eq!(authors.iter().map(display).collect::<Vec<_>>(), expected, "{key}");
    }
}

#[test]
fn organizations_are_literal_names_and_particles_are_kept() {
    let r = load("@book{n, author={{World Health Organization} and van der Berg, Jan and Plato}, title={T}}");
    let authors = &csl(&r, "n")["author"];
    assert_eq!(authors[0]["literal"], "World Health Organization");
    assert_eq!(authors[1]["dropping-particle"], "van der");
    assert_eq!(authors[2]["family"], "Plato");
}

#[test]
fn translators_and_other_roles_reach_csl() {
    let r = load(
        "@book{t, author={Tolstoy, Leo}, translator={Garnett, Constance}, bookauthor={Editor, Book}, title={War and Peace}}",
    );
    let csl = csl(&r, "t");
    assert_eq!(csl["translator"][0]["family"], "Garnett");
    assert_eq!(csl["container-author"][0]["family"], "Editor");
}

#[test]
fn chapter_authors_are_both_author_and_book_author() {
    let r = load("@book{p,title={Book},author={Doe, Jane}}\n@inbook{c,title={Chapter},crossref={p}}");
    let c = csl(&r, "c");
    assert_eq!(c["author"][0]["family"], "Doe");
    assert_eq!(c["container-author"][0]["family"], "Doe");
}

// --- positional macros (#349) ---------------------------------------------

#[test]
fn string_redefinitions_do_not_change_earlier_records() {
    let loaded = load(
        "@string{j = \"First\"}\n@article{a,title={A},journal=j}\n@string{j = \"Second\"}\n@article{b,title={B},journal=j}\n",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(keys(&loaded), ["a", "b"]);
    assert_eq!(text(&loaded, "a", "container-title"), Some("First"));
    assert_eq!(text(&loaded, "b", "container-title"), Some("Second"));
}

#[test]
fn aliases_capture_versions_without_false_cycles_or_cache_reuse() {
    let loaded = load(
        "@string{j={First}} @string{alias=j}\n\
         @article{a,journal=alias}\n\
         @string{j=alias # {Second}}\n\
         @article{b,journal=j}\n\
         @article{c,journal=alias}\n\
         @string{j={Third}} @article{d,journal=j}\n\
         @article{e,journal=alias}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    let journals: Vec<_> = ["a", "b", "c", "d", "e"]
        .into_iter()
        .map(|key| text(&loaded, key, "container-title"))
        .collect();
    assert_eq!(
        journals,
        [
            Some("First"),
            Some("FirstSecond"),
            Some("First"),
            Some("Third"),
            Some("First")
        ]
    );
}

#[test]
fn inherited_fields_use_the_parents_definition_epoch() {
    let loaded = load(
        "@string{j={First}}\n\
         @article{a,journal=j,crossref={parent}}\n\
         @string{j={Second}}\n\
         @proceedings{parent,publisher=j}\n\
         @string{j={Third}}\n\
         @article{b,journal=j,crossref={parent}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    for (key, journal) in [("a", "First"), ("b", "Third")] {
        assert_eq!(text(&loaded, key, "container-title"), Some(journal));
        assert_eq!(text(&loaded, key, "publisher"), Some("Second"));
    }
}

#[test]
fn captured_months_keep_field_context_after_redefinition() {
    let loaded = load(
        "@string{season=mar}\n\
         @article{a,title=season,month=season,year=2024}\n\
         @string{mar={May}}\n\
         @article{b,title=season,month=season,year=2024}\n\
         @article{c,title=mar,month=mar,year=2024}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    for (key, (title, expected)) in ["a", "b", "c"]
        .into_iter()
        .zip([("March", 3), ("March", 3), ("May", 5)])
    {
        assert_eq!(text(&loaded, key, "title"), Some(title));
        assert_eq!(month(&loaded, key), Some(expected));
    }
}

#[test]
fn future_definitions_and_direct_self_redefinitions_still_warn() {
    let loaded = load(
        "@string{alias=later # {Suffix}}\n\
         @string{later={Prefix}}\n\
         @article{a,journal=alias}\n\
         @string{j={First}}\n\
         @string{j=j # {Second}}\n\
         @article{b,journal=j}",
    );
    assert_eq!(keys(&loaded), ["a", "b"]);
    assert_eq!(text(&loaded, "a", "container-title"), Some("Suffix"));
    assert_eq!(text(&loaded, "b", "container-title"), Some("Second"));
    assert_eq!(loaded.warnings.len(), 2, "{:?}", loaded.warnings);
    // The undefined `later` is reported on `a`, the self-reference on `b`.
    assert_eq!(warned_at(&loaded, WarningCode::Field, 3), 1);
    assert_eq!(warned_at(&loaded, WarningCode::Field, 6), 1);
}

#[test]
fn chained_macros_keep_every_part() {
    let r = load("@string{a = \"Alpha\"}\n@string{b = a # \" Beta\"}\n@article{m, title={T}, journal = b}");
    assert_eq!(text(&r, "m", "container-title"), Some("Alpha Beta"));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

// --- inheritance (#344) ------------------------------------------------------

#[test]
fn case_distinct_xdata_keys_both_supply_their_fields() {
    let loaded = load("@xdata{A,publisher={First}}\n@xdata{a,note={Second}}\n@book{b,xdata={A,a},title={T}}");
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(keys(&loaded), ["b"]);
    assert_eq!(text(&loaded, "b", "publisher"), Some("First"));
    assert_eq!(text(&loaded, "b", "note"), Some("Second"));
}

#[test]
fn exact_duplicate_parents_keep_last_occurrence_precedence() {
    for (parents, expected) in [("A,a", "Second"), ("a,A", "First"), ("A,a,A", "First")] {
        let loaded = load(&format!(
            "@xdata{{A,publisher={{First}}}}\n@xdata{{a,publisher={{Second}}}}\n@book{{b,xdata={{{parents}}},title={{T}}}}"
        ));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(keys(&loaded), ["b"]);
        assert_eq!(text(&loaded, "b", "publisher"), Some(expected), "{parents}");
    }
}

#[test]
fn nested_inheritance_reaches_the_grandparent() {
    let r = load(
        "@proceedings{c, title={Grand}, year={2019}, publisher={P}}\n@proceedings{b, title={Mid}, crossref={c}}\n@inproceedings{a, title={Leaf}, crossref={b}}",
    );
    assert_eq!(year(&r, "a"), Some(2019));
    assert_eq!(text(&r, "a", "publisher"), Some("P"));
}

#[test]
fn missing_cross_reference_target_is_reported() {
    let r = load("@inproceedings{a, title={T}, crossref={nowhere}}");
    record(&r, "a");
    assert_eq!(warned_at(&r, WarningCode::Field, 1), 1, "{:?}", r.warnings);
}

#[test]
fn shared_ancestors_are_not_cycles() {
    // a inherits from b and c (via crossref and xdata); both inherit from d.
    let r = load(
        "@book{d, title={D}, publisher={Shared}, year={2001}}\n@book{b, title={B}, crossref={d}}\n@book{c, title={C}, xdata={d}}\n@book{a, title={A}, crossref={b}, xdata={c}}",
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(text(&r, "a", "publisher"), Some("Shared"));
    assert_eq!(year(&r, "a"), Some(2001));
}

#[test]
fn xdata_helpers_supply_fields_but_are_not_references() {
    let r = load(
        "@xdata{shared, publisher={Synthetic Press}, date={2024}}\n@book{work, author={Example, Ada}, title={Synthetic Book}, xdata={shared}}",
    );
    assert_eq!(keys(&r), ["work"]);
    assert_eq!(text(&r, "work", "publisher"), Some("Synthetic Press"));
    assert_eq!(year(&r, "work"), Some(2024));
}

#[test]
fn a_short_path_still_reaches_ancestors_cut_off_on_a_long_path() {
    let mut src = String::from("@book{a,crossref={c1},xdata={shared}}\n");
    for i in 1..15 {
        src.push_str(&format!("@book{{c{i},crossref={{c{}}}}}\n", i + 1));
    }
    src.push_str("@book{c15,crossref={shared}}\n@book{shared,crossref={grand}}\n@book{grand,year={2024}}\n");
    assert_eq!(year(&load(&src), "a"), Some(2024));
}

#[test]
fn errors_in_inherited_xdata_fields_are_reported_on_the_consumer() {
    let r = load("@xdata{common,publisher=missing}\n@book{a,title={T},xdata={common}}");
    record(&r, "a");
    assert_eq!(warned_at(&r, WarningCode::Field, 2), 1, "{:?}", r.warnings);
}

#[test]
fn original_spacing_marker_characters_are_literal_in_inheritance_keys() {
    for marker in ['\u{e005}', '\u{e006}'] {
        let loaded = load(&format!(
            "@book{{parent{marker},title={{Invented parent}},publisher={{Invented Press}}}}\n@xdata{{shared{marker},year={{2026}}}}\n@inbook{{child,title={{Invented child}},crossref={{parent{marker}}},xdata={{shared{marker}}}}}"
        ));
        assert_eq!(text(&loaded, "child", "publisher"), Some("Invented Press"));
        assert_eq!(year(&loaded, "child"), Some(2026));
    }
}

#[test]
fn nonbreaking_spaces_remain_semantic_in_names_and_inheritance_keys() {
    let loaded = load(
        "@book{parent,title={Invented parent},publisher={Invented Press}}\n@xdata{shared,year={2026}}\n@inbook{child,title={Invented child},author={Ada\u{a0}Okafor},crossref={\u{a0}parent\u{a0}},xdata={\u{a0}shared\u{a0}}}",
    );
    let author = &csl(&loaded, "child")["author"][0];
    assert_eq!(author["family"], "Okafor");
    assert_eq!(author["given"], "Ada");
    assert_eq!(text(&loaded, "child", "publisher"), Some("Invented Press"));
    assert_eq!(year(&loaded, "child"), Some(2026));
}

#[test]
fn nonbreaking_date_whitespace_retains_era_and_day_meaning() {
    let loaded = load(
        "@book{era,title={Invented era},year={2026\u{a0}BC}}\n@book{day,title={Invented day},year={2026},month={March\u{a0}2}}",
    );
    assert_eq!(year(&loaded, "era"), Some(-2025));
    assert_eq!(date(&loaded, "day", "issued"), Some((2026, Some(3), Some(2))));
}

#[test]
fn explicit_child_dates_win_over_inherited_dates() {
    let r = load("@xdata{shared,year=2000}\n@book{a,title={T},date={2024},xdata={shared}}");
    assert_eq!(year(&r, "a"), Some(2024));
}

#[test]
fn later_xdata_parents_override_earlier_ones() {
    let r = load(
        "@xdata{first,publisher={First},year={2000}}\n@xdata{second,publisher={Second},year={2024}}\n@book{a,title={Synthetic},xdata={first,second}}",
    );
    assert_eq!(text(&r, "a", "publisher"), Some("Second"));
    assert_eq!(year(&r, "a"), Some(2024));
}

#[test]
fn a_repeated_xdata_parent_counts_at_its_last_position() {
    let r = load(
        "@xdata{first,publisher={First}}\n@xdata{second,publisher={Second}}\n@book{a,title={T},xdata={first,second,first}}",
    );
    assert_eq!(text(&r, "a", "publisher"), Some("First"));
}

#[test]
fn inherited_dates_merge_component_by_component() {
    let r =
        load("@book{parent,title={Parent},date={2024-02-05}}\n@book{child,title={Child},month=mar,crossref={parent}}");
    assert_eq!(date(&r, "child", "issued"), Some((2024, Some(3), Some(5))));
    let month_only = load("@xdata{p,year=2024}\n@book{a,title={T},month=mar,xdata={p}}");
    assert_eq!(date(&month_only, "a", "issued"), Some((2024, Some(3), None)));
}

#[test]
fn intermediate_entries_keep_their_own_dates() {
    let r = load("@book{g,year=2000}\n@book{p,date={2024},crossref={g}}\n@book{a,title={T},crossref={p}}");
    assert_eq!(year(&r, "p"), Some(2024));
    assert_eq!(year(&r, "a"), Some(2024));
}

#[test]
fn an_own_date_is_never_given_inherited_precision() {
    let r =
        load("@book{p,date={2000-02-05}}\n@book{c,date={2024},crossref={p}}\n@book{d,title={T},date={2024},month=mar}");
    for key in ["c", "d"] {
        assert_eq!(date(&r, key, "issued"), Some((2024, None, None)), "{key}");
    }
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
    assert_eq!(text(&load(&src), "r", "publisher"), Some("G"));
}

#[test]
fn legacy_conference_alias_inherits_its_proceedings_title() {
    let r = load(
        "@proceedings{volume,title={Synthetic Proceedings},year={2024}}\n@conference{paper,title={Synthetic Paper},crossref={volume}}",
    );
    assert_eq!(text(&r, "paper", "type"), Some("paper-conference"));
    assert_eq!(text(&r, "paper", "container-title"), Some("Synthetic Proceedings"));
}

#[test]
fn periodical_title_addon_stays_with_the_journal() {
    let r = load("@periodical{p,title={Journal},titleaddon={Annex}}\n@article{c,title={Paper},crossref={p}}");
    assert_eq!(plain(&r, "c", "title").as_deref(), Some("Paper"));
    assert_eq!(text(&r, "c", "container-title"), Some("Journal. Annex"));
}

#[test]
fn multivolume_mappings_apply_only_to_their_entry_types() {
    let r = load("@mvbook{p,title={Whole}}\n@article{c,title={Paper},crossref={p}}");
    assert_eq!(plain(&r, "c", "title").as_deref(), Some("Paper"));
    assert_eq!(text(&r, "c", "volume-title"), None);
}

// --- containers ---------------------------------------------------------------

/// The container shown under a result: journal, book or proceedings title,
/// else the publisher (which also carries a thesis school).
fn container(bib: &str) -> Option<String> {
    let loaded = load(bib);
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    [plain(&loaded, "a", "container-title"), plain(&loaded, "a", "publisher")]
        .into_iter()
        .flatten()
        .map(|text| text.trim().to_owned())
        .find(|text| !text.is_empty())
}

#[test]
fn a_journal_article_shows_its_journal_without_markup() {
    let bib = r"@article{a, title={T}, journal={Journal of \emph{Synthetic} Studies}, publisher={Press}}";
    assert_eq!(container(bib).as_deref(), Some("Journal of Synthetic Studies"));
}

#[test]
fn a_chapter_shows_its_book_title() {
    let bib = "@incollection{a, title={T}, booktitle={Collected Synthetic Essays}, publisher={Press}}";
    assert_eq!(container(bib).as_deref(), Some("Collected Synthetic Essays"));
}

#[test]
fn a_book_without_a_container_shows_its_publisher() {
    assert_eq!(
        container("@book{a, title={T}, publisher={Example Press}}").as_deref(),
        Some("Example Press")
    );
    assert_eq!(
        container("@phdthesis{a, title={T}, school={Example University}}").as_deref(),
        Some("Example University")
    );
}

#[test]
fn missing_containers_show_nothing() {
    assert_eq!(container("@misc{a, title={T}}"), None);
}

// --- dates, titles and text fidelity ----------------------------------------

#[test]
fn quoted_parentheses_keep_the_record() {
    let r = load("@book(q, title = \"Notes (draft) on sleep\", year = 2020)\n@book{after, title={After}}");
    assert_eq!(plain(&r, "q", "title").as_deref(), Some("Notes (draft) on sleep"));
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn indented_records_after_damage_are_kept() {
    let r = load("@article{broken, title={Unclosed}\n  @article{indented, title={Indented}}\n");
    record(&r, "indented");
    assert_eq!(r.warnings.len(), 1, "{:?}", r.warnings);
}

#[test]
fn equivalent_dates_in_any_script_give_the_same_result() {
    let latin = load("@article{d, title={T}, year={2020}, month=mar}");
    let bengali = load("@article{d, title={T}, year={২০২০}, month=mar}");
    assert_eq!(csl(&latin, "d").get("issued"), csl(&bengali, "d").get("issued"));
    assert_eq!(month(&bengali, "d"), Some(3));
}

#[test]
fn invalid_or_unreadable_dates_are_reported_not_silently_changed() {
    let r = load(
        "@book{feb, title={T}, date={2021-02-31}}\n@book{junk, title={T}, date={2020-13-45x}}\n@book{none, title={T}, date={sometime}}",
    );
    assert_eq!(
        date(&r, "feb", "issued").map(|(_, month, day)| (month, day)),
        Some((Some(2), None))
    );
    assert_eq!(year(&r, "junk"), Some(2020));
    assert_eq!(csl(&r, "none")["issued"], json!({"literal":"sometime"}));
    assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
}

#[test]
fn ranges_and_approximate_dates_are_kept() {
    let r = load("@book{range, title={T}, date={2019/2021}}\n@book{circa, title={T}, date={2020-03~}}");
    assert_eq!(date_parts_end_year(&r, "range"), Some(2021));
    assert_eq!(csl(&r, "circa")["issued"]["circa"], json!(true));
}

fn date_parts_end_year(loaded: &ibis_sources::LoadedLibrary, key: &str) -> Option<i64> {
    bib::date_parts(loaded, key, "issued")
        .get(1)
        .and_then(|end| end.get(0))
        .and_then(Value::as_i64)
}

#[test]
fn uncertain_and_open_ended_dates_are_reported() {
    let r = load(
        "@book{u, title={T}, date={2020?}}\n@book{before, title={T}, date={../2020}}\n@book{after, title={T}, date={2020/..}}",
    );
    for line in 1..=3 {
        assert!(
            r.warnings.iter().any(|w| w.line == Some(line)),
            "line {line}: {:?}",
            r.warnings
        );
    }
}

#[test]
fn escaped_quotes_inside_quoted_values_keep_the_record() {
    let r = load("@book(q, title = \"G\\\"odel (test)\", year = 2020)\n@book{after, title={After}}");
    assert!(plain(&r, "q", "title").is_some_and(|t| t.contains("del (test)")));
    record(&r, "after");
}

#[test]
fn tex_symbols_and_math_are_kept_not_dropped() {
    let r = load(
        "@article{m, title={The \\ensuremath{\\alpha}-Helix and $\\beta$ Sheets at 37\\,\\textdegree C}, langid={american}}",
    );
    let title = plain(&r, "m", "title").unwrap_or_default();
    assert!(
        title.contains('α') && title.contains('β') && title.contains('°'),
        "{title}"
    );
}

#[test]
fn unknown_tex_commands_are_reported_not_silently_dropped() {
    let r = load("@article{u, title={A \\frobnicate Title}}");
    assert!(plain(&r, "u", "title").is_some_and(|t| t.contains("frobnicate")));
    assert_eq!(warned_at(&r, WarningCode::Field, 1), 1, "{:?}", r.warnings);
}

#[test]
fn subtitles_are_part_of_the_title() {
    let r = load(
        "@book{s, title={Sleep}, subtitle={A Very Short Introduction}, langid={american}}\n@article{j, title={T}, journaltitle={Memory}, journalsubtitle={Theory and Practice}}",
    );
    assert_eq!(
        plain(&r, "s", "title").as_deref(),
        Some("Sleep: A Very Short Introduction")
    );
    assert_eq!(text(&r, "j", "container-title"), Some("Memory: Theory and Practice"));
}

#[test]
fn math_superscripts_and_subscripts_are_kept() {
    let r = load("@article{m, title={Energy as $mc^2$ and $H_{2}O$}, langid={american}}");
    let title = text(&r, "m", "title").unwrap_or_default();
    assert!(title.contains("mc<sup>2</sup>"), "{title}");
    assert!(title.contains("H<sub>2</sub>O"), "{title}");
}

#[test]
fn access_and_original_dates_reach_csl() {
    let r = load("@online{web,title={Live Status},urldate={2024-05-06},origdate={1999},url={https://example.org/}}");
    let csl = csl(&r, "web");
    assert_eq!(csl["accessed"]["date-parts"][0], json!([2024, 5, 6]));
    assert_eq!(csl["original-date"]["date-parts"][0], json!([1999]));
}

#[test]
fn newspaper_and_magazine_articles_keep_their_type() {
    let r = load(
        "@article{news,entrysubtype={newspaper},title={H},journaltitle={Example News},date={2020-03-10}}\n@article{mag,entrysubtype={magazine},title={M},journaltitle={Example Monthly},date={2020-04}}",
    );
    assert_eq!(text(&r, "news", "type"), Some("article-newspaper"));
    assert_eq!(text(&r, "mag", "type"), Some("article-magazine"));
}

#[test]
fn comments_between_fields_do_not_lose_the_record() {
    let r = load("@book{a,title={T}, % a comment\n  year=2024}");
    assert_eq!(year(&r, "a"), Some(2024));
}

#[test]
fn escaped_braces_are_literal_text() {
    let r = load("@book{a,title={The Set \\{x,y\\}}, publisher={A \\{B\\} Press}, langid={american}}");
    assert_eq!(plain(&r, "a", "title").as_deref(), Some("The Set {x,y}"));
    assert_eq!(text(&r, "a", "publisher"), Some("A {B} Press"));
}

#[test]
fn unknown_commands_with_arguments_stay_visible() {
    let r = load("@book{a,title={The Ratio $\\frac{1}{2}$}}");
    let title = plain(&r, "a", "title").unwrap_or_default();
    assert!(title.contains("\\frac{1}{2}"), "{title}");
    assert_eq!(warned_at(&r, WarningCode::Field, 1), 1, "{:?}", r.warnings);
}

#[test]
fn multi_volume_titles_keep_the_whole_work() {
    let r = load(
        "@book{volume,title={Volume One},maintitle={Collected Works},volume={2},year={2020},langid={american}}\n@inbook{part,title={A Letter},booktitle={Early Letters},maintitle={Collected Works},volume={2},year={2020}}",
    );
    assert_eq!(plain(&r, "volume", "title").as_deref(), Some("Collected Works"));
    assert_eq!(text(&r, "volume", "volume-title"), Some("Volume One"));
    assert_eq!(text(&r, "part", "container-title"), Some("Collected Works"));
    assert_eq!(text(&r, "part", "volume-title"), Some("Early Letters"));
}

#[test]
fn series_numbers_and_report_numbers_keep_their_meaning() {
    let r = load(
        "@book{b,title={Work},series={Example Series},number={12},year={2020}}\n@report{rep,title={R},number={TR-7},institution={I},series={Tech Series},year={2020}}",
    );
    assert_eq!(text(&r, "b", "collection-number"), Some("12"));
    assert_eq!(text(&r, "b", "number"), None);
    assert_eq!(text(&r, "rep", "number"), Some("TR-7"));
}

#[test]
fn private_use_characters_in_text_are_not_mistaken_for_braces() {
    let r = load(
        "@book{pua,title={A \u{E000} and \u{E001} \u{E004} \u{E005} \u{E006} test \\{x\\}},url={https://example.org/\u{E004}a--b},year={2024}}",
    );
    let title = text(&r, "pua", "title").unwrap_or_default();
    assert!(
        title.contains('\u{E000}')
            && title.contains('\u{E001}')
            && title.contains('\u{E004}')
            && title.contains('\u{E005}')
            && title.contains('\u{E006}')
            && title.contains("{x}"),
        "{title:?}"
    );
    assert_eq!(text(&r, "pua", "URL"), Some("https://example.org/\u{E004}a--b"));
}

#[test]
fn comment_delimiters_and_protected_escaped_braces() {
    let r = load("@book{a,title={T}, % }\n  year=2024}\n@book{b,title={{\\{B\\}}}}");
    assert_eq!(year(&r, "a"), Some(2024));
    assert_eq!(plain(&r, "b", "title").as_deref(), Some("{B}"));
}

#[test]
fn unknown_commands_across_chunks_are_kept_exactly() {
    let r = load("@book{a,title={\\frac{1}{2}}}\n@book{b,title={$\\frac{1}{2^3}$}}");
    assert_eq!(plain(&r, "a", "title").as_deref(), Some("\\frac{1}{2}"));
    assert_eq!(plain(&r, "b", "title").as_deref(), Some("$\\frac{1}{2^3}$"));
}

#[test]
fn escaped_backslash_before_a_group_keeps_the_record() {
    let r = load("@book{c,title={A \\\\{B} C}}");
    record(&r, "c");
}

#[test]
fn private_use_characters_survive_the_unknown_command_fallback() {
    let r = load("@book{c,title={\\foo \u{E000}x\u{E001}\u{E002}}}");
    let title = text(&r, "c", "title").unwrap_or_default();
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
    assert_eq!(plain(&r, "c", "title").as_deref(), Some("Second"));
    assert_eq!(warned_at(&r, WarningCode::Field, 1), 1, "{:?}", r.warnings);
}

#[test]
fn short_titles_reach_csl() {
    let r = load("@book{a,title={A Very Long Title},shorttitle={Short},year={2020}}");
    assert_eq!(text(&r, "a", "title-short"), Some("Short"));
}

#[test]
fn textbackslash_is_literal_in_titles_and_plain_fields() {
    let r = load(
        r"@book{k,title={{A \textbackslash{} B}},subtitle={C \textbackslash{} D},
        shorttitle={{E \textbackslash{} F}},journaltitle={G \textbackslash{} H},
        publisher={I \textbackslash{} J},abstract={K \textbackslash{} L},
        author={{M \textbackslash{} N}},langid={ja}}",
    );
    assert_eq!(plain(&r, "k", "title").as_deref(), Some(r"A \ B: C \ D"));
    assert_eq!(
        text(&r, "k", "title-short"),
        Some(r#"<span class="nocase">E \ F</span>"#)
    );
    assert_eq!(text(&r, "k", "container-title"), Some(r"G \ H"));
    assert_eq!(text(&r, "k", "publisher"), Some(r"I \ J"));
    assert_eq!(text(&r, "k", "abstract"), Some(r"K \ L"));
    assert_eq!(csl(&r, "k")["author"][0]["literal"], r"M \ N");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn backslashes_in_urls_are_withheld_with_a_warning() {
    let r = load(r"@book{k,title={T},url={https://example.org/\textbackslash}}");
    assert_eq!(keys(&r), ["k"]);
    assert_eq!(text(&r, "k", "title"), Some("T"));
    assert_eq!(text(&r, "k", "URL"), None);
    assert_eq!(warned_at(&r, WarningCode::Field, 1), 1);
    assert_eq!(r.warnings.len(), 1);
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
    assert_eq!(
        plain(&r, "k", "title").as_deref(),
        Some("A \\alpha \\unknown \\\\ {B} \u{E003}")
    );
    assert_eq!(text(&r, "k", "publisher"), Some(r"\alpha"));
    assert_eq!(
        plain(&r, "u", "title").as_deref(),
        Some("\\textbackslash{}alpha \\unknown \u{E003}")
    );
    assert_eq!(plain(&r, "long", "title").as_deref(), Some(r"\textbackslashsuffix"));
    assert_eq!(plain(&r, "space", "title").as_deref(), Some(r"A \B"));
    assert_eq!(r.warnings.len(), 2, "{:?}", r.warnings);
}
