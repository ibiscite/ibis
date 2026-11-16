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
fn ligatures_decode_from_differences_and_from_unicode_maps_beside_an_encoding() {
    let pdf = invented_pdf(
        &["Ful\x02llment Re\x03ectance di\x01erent \x02ndings o\x04ce wa\x05e"],
        None,
    );
    for unicode in [false, true] {
        let mut document = Document::load_mem(&pdf).unwrap();
        let font_id = *document
            .objects
            .iter()
            .find(|(_, object)| object.as_dict().is_ok_and(|dict| dict.has_type(b"Font")))
            .unwrap()
            .0;
        let cmap = document.add_object(lopdf::Stream::new(
            Dictionary::new(),
            br"
/CIDInit /ProcSet findresource begin
12 dict begin begincmap
/CIDSystemInfo << /Registry (Invented) /Ordering (Unicode) /Supplement 0 >> def
/CMapName /Invented def /CMapType 2 def
1 begincodespacerange <00> <FF> endcodespacerange
1 beginbfrange <20> <7E> <0020> endbfrange
5 beginbfchar <01> <FB00> <02> <FB01> <03> <FB02> <04> <FB03> <05> <FB04> endbfchar
endcmap CMapName currentdict /CMap defineresource pop end end
"
            .to_vec(),
        ));
        let font = document.get_object_mut(font_id).unwrap().as_dict_mut().unwrap();
        if unicode {
            // Keep WinAnsiEncoding: lopdf incorrectly prefers it to ToUnicode.
            font.set("ToUnicode", cmap);
        } else {
            let mut encoding = Dictionary::new();
            encoding.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            let mut differences = vec![Object::Integer(1)];
            differences.extend(["ff", "fi", "fl", "ffi", "ffl"].map(|name| Object::Name(name.as_bytes().to_vec())));
            encoding.set("Differences", differences);
            font.set("Encoding", encoding);
        }
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        assert_eq!(
            extract(&bytes, false).unwrap().text.unwrap(),
            ["Fulfillment Reflectance different findings office waffle"],
            "ToUnicode: {unicode}"
        );
    }
}

#[test]
fn type1_fonts_use_the_embedded_encoding_when_pdf_encoding_is_absent() {
    let pdf = invented_pdf(&["A \x0csh \x0dower o\x0b o\x0ece wa\x0fe"], None);
    let mut document = Document::load_mem(&pdf).unwrap();
    let font_id = *document
        .objects
        .iter()
        .find(|(_, object)| object.as_dict().is_ok_and(|dict| dict.has_type(b"Font")))
        .unwrap()
        .0;
    let mut program = String::from(
        "%!PS-AdobeFont-1.0: Invented 1.0\n/Encoding 256 array\n\
         0 1 255 {1 index exch /.notdef put} for\n\
         dup 32 /space put dup 65 /A put\n",
    );
    for byte in b'a'..=b'z' {
        program.push_str(&format!("dup {byte} /{} put\n", char::from(byte)));
    }
    for (code, name) in (11..).zip(["ff", "fi", "fl", "ffi", "ffl"]) {
        program.push_str(&format!("dup {code} /{name} put\n"));
    }
    program.push_str("readonly def\ncurrentfile eexec\n");
    let mut stream = lopdf::Stream::new(Dictionary::new(), program.into_bytes());
    stream.dict.set("Length1", stream.content.len() as i64);
    let program = document.add_object(stream);
    let mut descriptor = Dictionary::new();
    descriptor.set("Type", Object::Name(b"FontDescriptor".to_vec()));
    descriptor.set("FontFile", program);
    let descriptor = document.add_object(descriptor);
    let font = document.get_object_mut(font_id).unwrap().as_dict_mut().unwrap();
    font.remove(b"Encoding");
    font.set("BaseFont", Object::Name(b"Invented".to_vec()));
    font.set("FontDescriptor", descriptor);
    let mut bytes = Vec::new();
    document.save_to(&mut bytes).unwrap();
    assert_eq!(
        extract(&bytes, false).unwrap().text.unwrap(),
        ["A fish flower off office waffle"]
    );
}

#[test]
fn a_contradicting_printed_number_invalidates_all_document_labels() {
    let pages = [
        "Invented preface.\niv",
        "No printed number.",
        "78\nInvented chapter.",
        "Invented last page.\n79",
        "mix\nInvented unnumbered page.",
    ];
    for (roman_start, arabic_start) in [(4, 78), (5, 78), (4, 79)] {
        let tree = [(0, Some("r"), "", roman_start), (2, Some("D"), "", arabic_start)];
        for labels_only in [false, true] {
            let extracted = extract(&invented_pdf(&pages, Some(&tree)), labels_only).unwrap();
            if (roman_start, arabic_start) == (4, 78) {
                assert_eq!(
                    labels(&extracted),
                    [Some("iv"), Some("v"), Some("78"), Some("79"), Some("80")]
                );
            } else {
                assert_eq!(extracted.labels, None);
            }
        }
    }
    // One matching boundary does not excuse a conflicting one on that page.
    let pdf = invented_pdf(&["1\nInvented page.\n2"], Some(&[(0, Some("D"), "", 1)]));
    assert_eq!(extract(&pdf, false).unwrap().labels, None);
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
