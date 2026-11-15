use super::*;

fn labels(extracted: &Extracted) -> Vec<Option<&str>> {
    extracted
        .labels
        .as_ref()
        .unwrap_or_else(|| panic!("a label tree"))
        .iter()
        .map(Option::as_deref)
        .collect()
}

#[test]
fn pages_keep_their_own_text_in_order() {
    let pdf = invented_pdf(
        &["Invented first page.", "Invented second page\nholds a brass lantern."],
        None,
    );
    let extracted = extract(&pdf, false).unwrap();
    assert_eq!(extracted.pages, 2);
    assert_eq!(
        extracted.text,
        Some(vec![
            "Invented first page.".to_owned(),
            "Invented second page\nholds a brass lantern.".to_owned()
        ])
    );
    // No label tree: no labels, never the file's own page numbers.
    assert_eq!(extracted.labels, None);
    let labels_only = extract(&pdf, true).unwrap();
    assert_eq!((labels_only.pages, labels_only.text), (2, None));
}

#[test]
fn page_labels_follow_the_label_tree_with_styles_prefixes_and_starts() {
    let pages = ["Cover", "Preface", "Preface", "One", "Two", "Map", "Map", "Index"];
    let tree = [
        (0, None, "Cover", 1),
        (1, Some("r"), "", 1),
        (3, Some("D"), "", 1),
        (5, Some("A"), "Plate-", 26),
        (7, Some("R"), "", 4),
    ];
    let extracted = extract(&invented_pdf(&pages, Some(&tree)), true).unwrap();
    assert_eq!(
        labels(&extracted),
        [
            Some("Cover"),
            Some("i"),
            Some("ii"),
            Some("1"),
            Some("2"),
            Some("Plate-Z"),
            Some("Plate-AA"),
            Some("IV")
        ]
    );
}

#[test]
fn pages_before_the_first_range_have_no_label_and_numbers_continue_from_start() {
    let tree = [(1, Some("D"), "A-", 41), (3, Some("a"), "", 27)];
    let extracted = extract(&invented_pdf(&["", "", "", "", ""], Some(&tree)), true).unwrap();
    assert_eq!(
        labels(&extracted),
        [None, Some("A-41"), Some("A-42"), Some("aa"), Some("bb")]
    );
}

#[test]
fn a_damaged_label_tree_gives_no_labels() {
    // A start below one and an unknown style are damaged.
    for tree in [[(0, Some("D"), "", 0)], [(0, Some("X"), "", 1)]] {
        let extracted = extract(&invented_pdf(&["Invented"], Some(&tree)), true).unwrap();
        assert_eq!(extracted.labels, None);
    }
    let twice = [(0, Some("D"), "", 1), (0, Some("r"), "", 1)];
    assert_eq!(
        extract(&invented_pdf(&["", ""], Some(&twice)), true).unwrap().labels,
        None
    );
}

#[test]
fn page_label_prefixes_are_bounded_before_they_are_repeated() {
    for prefix in ["x".repeat(65), "é".repeat(65), "x".repeat(8 * 1024 * 1024)] {
        let pdf = invented_pdf(&["Invented", "Invented"], Some(&[(0, Some("D"), &prefix, 1)]));
        assert_eq!(extract(&pdf, true).unwrap().labels, None);
    }
    let prefix = "é".repeat(64);
    let pdf = invented_pdf(&["Invented"], Some(&[(0, None, &prefix, 1)]));
    assert_eq!(labels(&extract(&pdf, true).unwrap()), [Some(prefix.as_str())]);
}

#[test]
fn the_total_label_output_is_bounded_before_expansion() {
    let pdf = invented_pdf(&["Invented"], Some(&[(0, None, &"界".repeat(64), 1)]));
    let document = Document::load_mem(&pdf).unwrap();
    // Even a caller supplying an excessive count cannot trigger expansion.
    assert!(page_labels(&document, 10_000).is_none());
}

#[cfg(unix)]
#[test]
fn memory_limits_apply_in_an_isolated_child() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pdf_text::tests::resource_limits_helper",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(unix)]
#[test]
#[ignore = "subprocess helper; lowers limits only in its own process"]
fn resource_limits_helper() {
    use nix::sys::resource::{Resource, getrlimit};
    limit_resources().unwrap();
    for resource in [Resource::RLIMIT_AS, Resource::RLIMIT_DATA] {
        let (soft, hard) = getrlimit(resource).unwrap();
        #[cfg(target_os = "macos")]
        if soft > MAX_MEMORY_BYTES || hard > MAX_MEMORY_BYTES {
            // Darwin rejects finite limits on some systems; the allocation
            // refusal test independently proves the heap budget there.
            assert_eq!(
                nix::sys::resource::setrlimit(resource, MAX_MEMORY_BYTES, MAX_MEMORY_BYTES),
                Err(nix::errno::Errno::EINVAL)
            );
            continue;
        }
        assert!(
            soft <= 3 * 1024 * 1024 * 1024 && hard <= 3 * 1024 * 1024 * 1024,
            "{resource:?}"
        );
    }
    let (soft, hard) = getrlimit(Resource::RLIMIT_CPU).unwrap();
    assert!(soft <= MAX_CPU_SECONDS && hard <= MAX_CPU_SECONDS);
}

#[test]
fn the_child_refuses_allocations_over_its_heap_budget() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "pdf_text::tests::heap_limit_helper",
            "--ignored",
            "--nocapture",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("memory allocation of 2097152 bytes failed"));
}

#[test]
#[ignore = "subprocess helper; exhausts only its own small heap budget"]
fn heap_limit_helper() {
    #[cfg(unix)]
    nix::sys::resource::setrlimit(nix::sys::resource::Resource::RLIMIT_CORE, 0, 0).unwrap();
    // Enabling a tiny budget in this helper proves enforcement without putting
    // memory pressure on the shared machine. The real child installs 3 GiB.
    heap_limit::set(1024 * 1024);
    // Refused growth preserves the old allocation; shrinking and releasing it
    // restore capacity. Repeated allocations cannot consume the budget forever.
    let mut bytes = Vec::with_capacity(128 * 1024);
    bytes.resize(128 * 1024, 7u8);
    bytes.reserve_exact(256 * 1024);
    assert!(bytes.try_reserve_exact(2 * 1024 * 1024).is_err());
    assert!(bytes.iter().all(|byte| *byte == 7));
    bytes.truncate(64 * 1024);
    bytes.shrink_to_fit();
    drop(bytes);
    for _ in 0..16 {
        std::hint::black_box(vec![0u8; 128 * 1024]);
    }
    std::hint::black_box(vec![0u8; 2 * 1024 * 1024]);
}

#[test]
fn roman_and_letter_numbers_stay_short_enough_to_cite() {
    assert_eq!(roman(1994).as_deref(), Some("MCMXCIV"));
    assert_eq!(
        roman(16_999).map(|roman| roman.starts_with("MMMMMMMMMMMMMMMM")),
        Some(true)
    );
    assert_eq!(roman(17_000), None);
    let range = |style| LabelRange {
        first: 0,
        style: Some(style),
        prefix: String::new(),
        start: 1,
    };
    assert_eq!(label(&range(b'A'), 25).as_deref(), Some("Z"));
    assert_eq!(label(&range(b'a'), 26 * 16 - 1).as_deref(), Some("zzzzzzzzzzzzzzzz"));
    assert_eq!(label(&range(b'a'), 26 * 16), None);
    assert_eq!(label(&range(b'D'), u64::MAX), None);
}

#[test]
fn files_that_are_not_pdfs_or_are_damaged_are_refused() {
    assert_eq!(
        extract(b"Invented notes, not a PDF.", false).err(),
        Some(Refusal::NotPdf)
    );
    assert_eq!(extract(b"%PDF-1.7\n%%EOF\n", false).err(), Some(Refusal::Damaged));
    let pdf = invented_pdf(&["Invented page."], None);
    assert_eq!(extract(&pdf[..pdf.len() / 3], false).err(), Some(Refusal::Damaged));
}

#[test]
fn damaged_invented_pdfs_never_escape_their_bounds() {
    // A bounded property check over cut, flipped and repeated bytes of an
    // invented PDF: every answer is a refusal or within the limits.
    let pdf = invented_pdf(
        &["Invented first page.", "Invented second page."],
        Some(&[(0, Some("r"), "", 1)]),
    );
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..200 {
        let mut damaged = pdf.clone();
        match next() % 3 {
            0 => damaged.truncate((next() as usize) % damaged.len()),
            1 => {
                for _ in 0..1 + next() % 8 {
                    let at = (next() as usize) % damaged.len();
                    damaged[at] = next() as u8;
                }
            }
            _ => {
                let at = (next() as usize) % damaged.len();
                let piece = damaged[at..].to_vec();
                damaged.extend_from_slice(&piece);
            }
        }
        let outcome = std::panic::catch_unwind(|| extract(&damaged, false));
        // A panic would end only the child process; it must still be rare
        // enough not to matter here.
        if let Ok(Ok(extracted)) = outcome {
            assert!(ibis_engine::pdf::checked(extracted, false).is_ok());
        }
    }
}

#[test]
fn the_child_answers_with_json_and_refuses_unknown_arguments() {
    let pdf = invented_pdf(&["Invented page."], Some(&[(0, Some("D"), "", 7)]));
    let mut output = Vec::new();
    assert_eq!(child(&[], pdf.as_slice(), &mut output), 0);
    let answer: Answer = serde_json::from_slice(&output).unwrap();
    let Answer::Read(extracted) = answer else {
        panic!("a read")
    };
    assert_eq!(labels(&extracted), [Some("7")]);
    let mut output = Vec::new();
    assert_eq!(child(&[], b"not a pdf".as_slice(), &mut output), 0);
    assert!(serde_json::from_slice::<Answer>(&output).unwrap() == Answer::Refused(Refusal::NotPdf));
    assert_eq!(child(&["--pages".into()], b"".as_slice(), Vec::new()), 2);
}
