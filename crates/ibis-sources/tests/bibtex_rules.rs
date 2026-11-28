//! Bibliographic meaning the BibTeX reader must keep: casing, names, macros,
//! inheritance, containers and import fidelity. Synthetic data only.
//!
//! Each case once lost or changed meaning in the earlier loader; the same
//! rules are asserted here on the CSL that the shared file loader publishes.

#[path = "support/bib.rs"]
mod bib;

use bib::{csl, date, display, keys, load, name, plain, record, text, warned_at, year};
use ibis_sources::WarningCode;
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;

// --- language tags and stored title case (#331, #1530) --------------------

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
fn inherited_dates_merge_component_by_component() {
    let r =
        load("@book{parent,title={Parent},date={2024-02-05}}\n@book{child,title={Child},month=mar,crossref={parent}}");
    assert_eq!(date(&r, "child", "issued"), Some((2024, Some(3), Some(5))));
    let month_only = load("@xdata{p,year=2024}\n@book{a,title={T},month=mar,xdata={p}}");
    assert_eq!(date(&month_only, "a", "issued"), Some((2024, Some(3), None)));
}

#[test]
fn conference_alias_inherits_its_proceedings_title() {
    let r = load(
        "@proceedings{volume,title={Synthetic Proceedings},year={2024}}\n@conference{paper,title={Synthetic Paper},crossref={volume}}",
    );
    assert_eq!(text(&r, "paper", "type"), Some("paper-conference"));
    assert_eq!(text(&r, "paper", "container-title"), Some("Synthetic Proceedings"));
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

// --- dates, titles and text fidelity ----------------------------------------

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
fn years_before_the_common_era_count_as_csl_counts_them() {
    // biblatex reads ISO 8601 years, in which 0000 is 1 BC and -0043 is 44 BC.
    let r = load(
        "@book{era,title={T},year={45 BC}}\n@book{iso,title={T},date={-0043-03-15}}\n@book{zero,title={T},date={0000/0050}}",
    );
    assert_eq!(csl(&r, "era")["issued"]["date-parts"], json!([[-45]]));
    assert_eq!(csl(&r, "iso")["issued"]["date-parts"], json!([[-44, 3, 15]]));
    assert_eq!(csl(&r, "zero")["issued"]["date-parts"], json!([[-1], [50]]));
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
fn escaped_braces_are_literal_text() {
    let r = load("@book{a,title={The Set \\{x,y\\}}, publisher={A \\{B\\} Press}, langid={american}}");
    assert_eq!(plain(&r, "a", "title").as_deref(), Some("The Set {x,y}"));
    assert_eq!(text(&r, "a", "publisher"), Some("A {B} Press"));
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
