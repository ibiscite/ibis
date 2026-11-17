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
fn the_total_label_output_is_bounded_before_expansion() {
    let pdf = invented_pdf(&["Invented"], Some(&[(0, None, &"界".repeat(64), 1)]));
    let document = Document::load_mem(&pdf).unwrap();
    // Even a caller supplying an excessive count cannot trigger expansion.
    assert!(page_labels(&document, &vec![String::new(); 10_000]).is_none());
}

#[cfg(unix)]
#[test]
fn resource_limits_apply_in_an_isolated_child() {
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
    let (soft, hard) = getrlimit(Resource::RLIMIT_CPU).unwrap();
    assert!(soft <= MAX_CPU_SECONDS && hard <= MAX_CPU_SECONDS);
    assert_eq!(getrlimit(Resource::RLIMIT_CORE).unwrap(), (0, 0));
    #[cfg(target_os = "linux")]
    {
        let (soft, hard) = getrlimit(Resource::RLIMIT_AS).unwrap();
        assert!(soft <= MAX_MEMORY_BYTES && hard <= MAX_MEMORY_BYTES);
    }
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
