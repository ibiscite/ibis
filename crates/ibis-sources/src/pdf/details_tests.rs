use super::*;

fn said(info: Stated, xmp: Stated, first_page: &str) -> Said {
    Said {
        pages: 3,
        info,
        xmp,
        text: vec![first_page.to_owned(), "Invented second page.".to_owned()],
    }
}

fn titled(title: &str) -> Stated {
    Stated {
        title: Some(title.to_owned()),
        ..Stated::default()
    }
}

#[test]
fn the_xmp_wins_over_the_info_dictionary_and_noise_is_dropped() {
    let details = checked_details(said(
        Stated {
            title: Some("Invented  Info\ttitle".into()),
            creators: vec!["Ada Quillfeather; Bo Venn-Harrow".into()],
            ..Stated::default()
        },
        Stated {
            title: Some("Microsoft Word - tides_final_v3.docx".into()),
            creators: vec!["jsmith".into(), "Adobe".into()],
            container: Some("Journal of Invented Oceans".into()),
            first_page: Some("101".into()),
            last_page: Some("119".into()),
            date: Some("2031-04-02T00:00:00Z".into()),
            ..Stated::default()
        },
        "",
    ))
    .unwrap();
    // The XMP's title and authors are noise, so the Info dictionary's count.
    assert_eq!(details.title.as_deref(), Some("Invented Info title"));
    assert_eq!(details.authors, ["Ada Quillfeather", "Bo Venn-Harrow"]);
    assert_eq!(details.container.as_deref(), Some("Journal of Invented Oceans"));
    assert_eq!(details.page.as_deref(), Some("101-119"));
    assert_eq!(details.issued.as_deref(), Some("2031-04-02"));
    assert_eq!(details.doi, None);
}

#[test]
fn titles_that_name_a_file_or_a_template_are_unknown() {
    for noise in [
        "tides.pdf",
        "Microsoft Word - Invented draft",
        "untitled",
        "Title",
        "C:\\Invented\\draft",
        "/Users/invented/tides",
        "  \u{7}  ",
        "\u{fffd}\u{fffd}\u{fffd}\u{fffd}",
    ] {
        let details = checked_details(said(titled(noise), Stated::default(), "")).unwrap();
        assert_eq!(details.title, None, "{noise:?}");
    }
    let long = "w".repeat(MAX_VALUE_CHARS + 1);
    assert_eq!(
        checked_details(said(titled(&long), Stated::default(), ""))
            .unwrap()
            .title,
        None
    );
    let details = checked_details(said(titled("Ce\u{301}tacés inventés"), Stated::default(), "")).unwrap();
    assert_eq!(details.title.as_deref(), Some("Cétacés inventés"));
    // Short is not noise: an invented work may be called that.
    assert_eq!(
        checked_details(said(titled("Up"), Stated::default(), ""))
            .unwrap()
            .title
            .as_deref(),
        Some("Up")
    );
}

#[test]
fn authors_are_people_or_bodies_never_programs_or_accounts() {
    let info = Stated {
        creators: vec!["Quillfeather, Ada and Bo Venn-Harrow & Invented Ocean Society".into()],
        ..Stated::default()
    };
    let details = checked_details(said(info, Stated::default(), "")).unwrap();
    assert_eq!(
        details.authors,
        ["Quillfeather, Ada", "Bo Venn-Harrow", "Invented Ocean Society"]
    );
    let one = Stated {
        creators: vec!["Sol".into()],
        ..Stated::default()
    };
    assert_eq!(
        checked_details(said(one, Stated::default(), "")).unwrap().authors,
        ["Sol"]
    );
    for noise in [
        "admin",
        "Microsoft Office User",
        "LaTeX with hyperref",
        "owner",
        "jsmith",
        "user01",
    ] {
        let info = Stated {
            creators: vec![noise.into()],
            ..Stated::default()
        };
        assert!(
            checked_details(said(info, Stated::default(), ""))
                .unwrap()
                .authors
                .is_empty(),
            "{noise}"
        );
    }
}

#[test]
fn the_doi_is_one_the_metadata_names_and_a_printed_one_is_only_a_candidate() {
    let both = |xmp: Option<&str>, info: Option<&str>, ids: &[&str], page: &str| {
        let details = checked_details(said(
            Stated {
                doi: info.map(Into::into),
                ..Stated::default()
            },
            Stated {
                doi: xmp.map(Into::into),
                identifiers: ids.iter().map(|id| (*id).to_owned()).collect(),
                ..Stated::default()
            },
            page,
        ))
        .unwrap();
        (details.doi, details.page_doi)
    };
    let named = |xmp, info, ids: &[&str], page| both(xmp, info, ids, page).0;
    let printed = |page| both(None, None, &[], page);
    let page = "Invented header doi:10.5555/page.one\nReferences 10.5555/cited.two";
    assert_eq!(
        named(Some("10.5555/XMP"), Some("10.5555/info"), &[], page).as_deref(),
        Some("10.5555/xmp")
    );
    assert_eq!(
        named(Some("not a doi"), Some("https://doi.org/10.5555/Info"), &[], page).as_deref(),
        Some("10.5555/info")
    );
    assert_eq!(
        named(None, None, &["urn:isbn:invented", "doi:10.5555/id"], page).as_deref(),
        Some("10.5555/id")
    );
    // A named DOI leaves no candidate.
    assert_eq!(both(Some("10.5555/xmp"), None, &[], page).1, None);
    // Two distinct DOIs on the first page: one is probably cited, so neither.
    assert_eq!(printed(page), (None, None));
    let once = "Invented header https://doi.org/10.5555/Page.One\nAgain: doi:10.5555/page.one.";
    assert_eq!(printed(once), (None, Some("10.5555/page.one".to_owned())));
    assert_eq!(printed(""), (None, None));
}

#[test]
fn dates_are_publication_dates_with_plausible_parts() {
    let dated = |date: &str| {
        checked_details(said(
            Stated::default(),
            Stated {
                date: Some(date.into()),
                ..Stated::default()
            },
            "",
        ))
        .unwrap()
        .issued
    };
    assert_eq!(dated("2031").as_deref(), Some("2031"));
    assert_eq!(dated("2031-04").as_deref(), Some("2031-04"));
    assert_eq!(dated("2031-13-40").as_deref(), Some("2031"));
    assert_eq!(dated("D:20310402"), None);
    assert_eq!(dated("0001-01-01"), None);
    assert_eq!(dated("tomorrow"), None);
}

#[test]
fn an_answer_past_a_bound_is_refused() {
    let mut answer = said(Stated::default(), Stated::default(), "");
    answer.text.push("Invented third page.".into());
    assert_eq!(checked_details(answer), Err(Refusal::Damaged));
    let mut answer = said(Stated::default(), Stated::default(), "");
    answer.pages = 0;
    assert_eq!(checked_details(answer), Err(Refusal::Damaged));
    let answer = said(
        Stated::default(),
        Stated::default(),
        &"w".repeat(MAX_PAGE_TEXT_BYTES + 1),
    );
    assert_eq!(checked_details(answer), Err(Refusal::TooLarge));
    let answer = said(
        Stated {
            creators: vec!["Invented Person".into(); MAX_LIST + 1],
            ..Stated::default()
        },
        Stated::default(),
        "",
    );
    assert_eq!(checked_details(answer), Err(Refusal::TooLarge));
}

#[test]
fn a_numeric_title_is_a_title() {
    let details = checked_details(said(titled("2049"), Stated::default(), "")).unwrap();
    assert_eq!(details.title.as_deref(), Some("2049"));
}

#[test]
fn a_value_longer_than_the_child_sends_refuses_the_answer() {
    let info = Stated {
        creators: vec!["A;".repeat(MAX_STATED_BYTES)],
        ..Stated::default()
    };
    assert_eq!(
        checked_details(said(info, Stated::default(), "")),
        Err(Refusal::TooLarge)
    );
    let info = Stated {
        creators: vec!["Invented Person;".repeat(300)[..MAX_STATED_BYTES].to_owned()],
        ..Stated::default()
    };
    assert!(
        checked_details(said(info, Stated::default(), ""))
            .unwrap()
            .authors
            .len()
            <= MAX_LIST
    );
}

#[test]
fn a_page_with_more_dois_than_one_scan_reports_gives_no_candidate() {
    let page = format!(
        "{}10.5555/b",
        "10.5555/a ".repeat(ibis_core::identifier::MAX_DOIS_FOUND)
    );
    assert_eq!(
        checked_details(said(Stated::default(), Stated::default(), &page))
            .unwrap()
            .page_doi,
        None
    );
}
