use super::*;

const XMP: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about="" xmlns:dc="http://purl.org/dc/elements/1.1/"
     xmlns:p3="http://prismstandard.org/namespaces/basic/3.0/"
     p3:doi="10.5555/Invented.Tides" p3:volume="12">
   <dc:title><rdf:Alt>
     <rdf:li xml:lang="de">Erfundene Gezeiten</rdf:li>
     <rdf:li xml:lang="x-default">Invented Tides &amp; Currents</rdf:li>
   </rdf:Alt></dc:title>
   <dc:creator><rdf:Seq><rdf:li>Ada Quillfeather</rdf:li><rdf:li>Venn-Harrow, Bo</rdf:li></rdf:Seq></dc:creator>
   <dc:identifier>doi:10.5555/invented.tides</dc:identifier>
   <p3:publicationName>Journal of Invented Oceans</p3:publicationName>
   <p3:startingPage>101</p3:startingPage><p3:endingPage>119</p3:endingPage>
   <p3:coverDate>2031-04-02</p3:coverDate>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

#[test]
fn the_info_dictionary_and_xmp_are_reported_as_stated() {
    let pdf = invented_pdf_with_details(
        &["Invented first page.", "Invented second page.", "Invented third page."],
        &[
            ("Title", "Erfundene Gezeiten — Entwurf"),
            ("Author", "Ada Quillfeather; Bo Venn-Harrow"),
            ("doi", "10.5555/info"),
        ],
        Some(XMP),
    );
    let said = details(&pdf).unwrap();
    assert_eq!(said.pages, 3);
    assert_eq!(said.text, ["Invented first page.", "Invented second page."]);
    assert_eq!(said.info.title.as_deref(), Some("Erfundene Gezeiten — Entwurf"));
    assert_eq!(said.info.creators, ["Ada Quillfeather; Bo Venn-Harrow"]);
    assert_eq!(said.info.doi.as_deref(), Some("10.5555/info"));
    let xmp = said.xmp;
    assert_eq!(xmp.title.as_deref(), Some("Invented Tides & Currents"));
    assert_eq!(xmp.creators, ["Ada Quillfeather", "Venn-Harrow, Bo"]);
    assert_eq!(xmp.doi.as_deref(), Some("10.5555/Invented.Tides"));
    assert_eq!(xmp.identifiers, ["doi:10.5555/invented.tides"]);
    assert_eq!(xmp.container.as_deref(), Some("Journal of Invented Oceans"));
    assert_eq!(xmp.volume.as_deref(), Some("12"));
    assert_eq!(
        (xmp.first_page.as_deref(), xmp.last_page.as_deref()),
        (Some("101"), Some("119"))
    );
    assert_eq!(xmp.date.as_deref(), Some("2031-04-02"));
}

#[test]
fn a_damaged_or_hostile_packet_keeps_the_rest() {
    for xmp in [
        "<!DOCTYPE x [<!ENTITY e SYSTEM 'file:///invented-must-not-be-opened'>]><x>&e;</x>".to_owned(),
        format!("<x>{}</x>", "<a>".repeat(MAX_DEPTH + 1)),
        "<rdf:Description xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Unclosed".to_owned(),
        "\u{0}\u{1}not xml at all".to_owned(),
    ] {
        let pdf = invented_pdf_with_details(&["Invented page."], &[("Title", "Invented Info Title")], Some(&xmp));
        let said = details(&pdf).unwrap();
        assert_eq!(said.info.title.as_deref(), Some("Invented Info Title"), "{xmp:.40}");
        assert_eq!(said.xmp.title, None, "{xmp:.40}");
        assert_eq!(said.text, ["Invented page."]);
    }
}

#[test]
fn values_and_lists_are_bounded_here_too() {
    let long = "w".repeat(MAX_VALUE_BYTES * 2);
    let creators: String = (0..MAX_LIST + 20)
        .map(|i| format!("<rdf:li>Invented Person {i}</rdf:li>"))
        .collect();
    let xmp = format!(
        "<rdf:Description xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
         <dc:title>{long}</dc:title><dc:creator><rdf:Seq>{creators}</rdf:Seq></dc:creator></rdf:Description>"
    );
    let pdf = invented_pdf_with_details(&["Invented page."], &[("Title", &long)], Some(&xmp));
    let said = details(&pdf).unwrap();
    assert!(said.info.title.unwrap().len() <= MAX_VALUE_BYTES);
    assert!(said.xmp.title.unwrap().len() <= MAX_VALUE_BYTES);
    assert_eq!(said.xmp.creators.len(), MAX_LIST);
}

#[test]
fn a_file_without_details_says_nothing_and_a_non_pdf_is_refused() {
    let said = details(&crate::pdf_text::invented_pdf(&["Invented page."], None)).unwrap();
    assert_eq!((said.info, said.xmp), (Stated::default(), Stated::default()));
    assert_eq!(
        details(b"Invented notes saved as text.").map(|_| ()),
        Err(Refusal::NotPdf)
    );
}

#[test]
fn the_child_answers_details_when_asked() {
    let pdf = invented_pdf_with_details(&["Invented page."], &[("Title", "Invented Info Title")], None);
    let mut output = Vec::new();
    let status = crate::pdf_text::child(&[ibis_engine::pdf::DETAILS.into()], pdf.as_slice(), &mut output);
    assert_eq!(status, 0);
    let answer: ibis_engine::pdf::Answer = serde_json::from_slice(&output).unwrap();
    let ibis_engine::pdf::Answer::Details(said) = answer else {
        panic!("a details answer")
    };
    assert_eq!(said.info.title.as_deref(), Some("Invented Info Title"));
}

#[test]
fn prefixes_mean_what_their_scope_declares() {
    let xmp = r#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description xmlns:prism="urn:invented-unrelated" prism:doi="10.5555/other"><prism:publicationName>Not a journal</prism:publicationName></rdf:Description>
<rdf:Description xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Scoped Title</dc:title></rdf:Description>
<rdf:Description><dc:title>Out of scope, read by its usual prefix</dc:title></rdf:Description>
</rdf:RDF></x:xmpmeta>"#;
    let pdf = invented_pdf_with_details(&["Invented page."], &[], Some(xmp));
    let xmp = details(&pdf).unwrap().xmp;
    assert_eq!(xmp.doi, None);
    assert_eq!(xmp.container, None);
    assert_eq!(xmp.title.as_deref(), Some("Scoped Title"));
}
