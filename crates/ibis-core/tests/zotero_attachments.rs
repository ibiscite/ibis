//! Synthetic Zotero items with and without a best attachment (#573): only
//! the kind of the attachment is read from `links.attachment`, hostile
//! values give no kind and never break the library, and no attachment
//! detail other than the kind leaves core outside `raw`.

use ibis_core::model::{AttachmentKind, Reference};
use ibis_core::zotero::{Library, load_items};
use serde_json::{Value, json};

fn fixture(file: &str) -> Vec<Value> {
    let path = format!("{}/../../fixtures/zotero/{file}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path} is not JSON: {e}"))
}

/// Rewrites the object CSL-JSON of each item into the text form Zotero's
/// local API sends (the CSL export of a one-item array).
fn as_local_api(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .map(|item| {
            let mut item = item.clone();
            let csl = item["csljson"].take();
            item["csljson"] = json!(json!([csl]).to_string());
            item
        })
        .collect()
}

fn load_both_forms(file: &str) -> Vec<Vec<Reference>> {
    let items = fixture(file);
    [items.clone(), as_local_api(&items)]
        .into_iter()
        .map(|items| {
            let loaded = load_items(Library::User, &items);
            assert!(loaded.warnings.is_empty(), "{file}: {:?}", loaded.warnings);
            loaded.references
        })
        .collect()
}

fn only<'a>(references: &'a [Reference], key: &str) -> &'a Reference {
    references
        .iter()
        .find(|r| r.id.native_key == key)
        .unwrap_or_else(|| panic!("`{key}` not loaded"))
}

fn kind_of(references: &[Reference], key: &str) -> Option<AttachmentKind> {
    only(references, key).attachment
}

fn serialized(reference: &Reference) -> Value {
    serde_json::to_value(reference).unwrap_or_else(|e| panic!("cannot serialize: {e}"))
}

#[test]
fn the_kind_of_the_best_attachment_is_read_in_both_envelope_forms() {
    for references in load_both_forms("attachments.json") {
        assert_eq!(kind_of(&references, "SYN00001"), Some(AttachmentKind::Pdf));
        assert_eq!(kind_of(&references, "SYN00002"), Some(AttachmentKind::WebPage));
        assert_eq!(kind_of(&references, "SYN00003"), Some(AttachmentKind::WebPage));
        assert_eq!(kind_of(&references, "SYN00004"), None, "a linked URL is not a file");
        assert_eq!(kind_of(&references, "SYN00005"), Some(AttachmentKind::Other));
        assert_eq!(kind_of(&references, "SYN00006"), None, "no children");
        assert_eq!(
            kind_of(&references, "SYN00007"),
            Some(AttachmentKind::Pdf),
            "parameters and case"
        );
        assert!(
            references.iter().all(|r| r.id.native_key != "SYN00011"),
            "the child attachment item is still not a reference"
        );
        assert_eq!(references.len(), 7);
    }
}

#[test]
fn hostile_attachment_links_give_no_kind_and_keep_the_reference() {
    for references in load_both_forms("attachments-hostile.json") {
        assert_eq!(references.len(), 6, "one bad link never drops an item");
        for key in ["SYN00101", "SYN00102", "SYN00103", "SYN00104", "SYN00105"] {
            assert_eq!(kind_of(&references, key), None, "{key}");
            assert!(references.iter().any(|r| r.id.native_key == key && r.title.is_some()));
        }
        assert_eq!(kind_of(&references, "SYN00106"), Some(AttachmentKind::Pdf));
    }
}

#[test]
fn the_attachment_type_is_bounded_in_length() {
    for len in [256usize, 257, 10_000, 1 << 20] {
        let mime = format!("application/{}", "x".repeat(len - "application/".len()));
        assert_eq!(mime.len(), len);
        let item = json!({
            "key": "SYN00201",
            "links": {"attachment": {"attachmentType": mime}},
            "data": {"itemType": "book"},
            "csljson": {"type": "book", "title": "Synthetic bounded book"}
        });
        let loaded = load_items(Library::User, &[item]);
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.references.len(), 1);
        let expected = if len <= 256 { Some(AttachmentKind::Other) } else { None };
        assert_eq!(loaded.references[0].attachment, expected, "length {len}");
    }
}

#[test]
fn only_the_kind_leaves_core_and_raw_is_the_whole_item() {
    let items = fixture("attachments-hostile.json");
    let sentinel_links = items
        .iter()
        .find(|i| i["key"] == "SYN00106")
        .map(|i| i["links"].clone())
        .unwrap_or_default();
    assert!(
        sentinel_links["attachment"]["href"].is_string(),
        "the fixture carries sentinels"
    );
    for items in [items.clone(), as_local_api(&items)] {
        let loaded = load_items(Library::User, &items);
        let reference = only(&loaded.references, "SYN00106");
        let mut value = serialized(reference);
        assert_eq!(value["attachment"], json!("pdf"));
        let raw = value["raw"].take();
        let text = value.to_string();
        for sentinel in [
            "SENTINEL",
            "sentinel-user",
            "999999999999",
            "localhost:23119",
            "SYN00112",
        ] {
            assert!(!text.contains(sentinel), "`{sentinel}` leaked into {text}");
        }
        // ADR 0005: the source record is kept verbatim, links included.
        let raw: Value = serde_json::from_str(raw.as_str().unwrap_or_default()).unwrap_or_default();
        assert_eq!(raw["links"], sentinel_links);
        assert_eq!(raw["key"], json!("SYN00106"));
        // The CSL export does not mention the attachment either.
        assert!(!reference.to_csl_json().to_string().contains("SENTINEL"));
    }
}

#[test]
fn the_kind_serializes_as_kebab_case_and_is_absent_when_unknown() {
    let references = load_both_forms("attachments.json").swap_remove(0);
    let text = |key: &str| serialized(only(&references, key));
    assert_eq!(text("SYN00001")["attachment"], json!("pdf"));
    assert_eq!(text("SYN00002")["attachment"], json!("web-page"));
    assert_eq!(text("SYN00005")["attachment"], json!("other"));
    assert!(text("SYN00006").get("attachment").is_none());
    let round_trip: Result<Reference, _> = serde_json::from_value(text("SYN00002"));
    assert_eq!(
        round_trip.map(|r| r.attachment).ok(),
        Some(Some(AttachmentKind::WebPage))
    );
    let old: Result<Reference, _> = serde_json::from_value(json!({
        "id": {"source": "zotero:user", "native_key": "SYN00001"}, "item_type": "book"
    }));
    assert_eq!(
        old.map(|r| r.attachment).ok(),
        Some(None),
        "records saved before the field exists still load"
    );
}

#[test]
fn unicode_mime_values_use_ascii_matching_and_a_byte_bound() {
    for (mime, expected) in [
        (json!("application/PDF; name=日本語"), Some(AttachmentKind::Pdf)),
        (json!("text/HTML; charset=utf-8"), Some(AttachmentKind::WebPage)),
        (json!("application/pdƒ"), Some(AttachmentKind::Other)),
        (json!("applıcation/pdf"), Some(AttachmentKind::Other)),
        (json!("application/ｐｄｆ"), Some(AttachmentKind::Other)),
        (json!("\u{2003}\t ; charset=日本語"), None),
        (json!("😀".repeat(64)), Some(AttachmentKind::Other)),
        (json!("😀".repeat(65)), None),
        (
            json!(format!("application/{}", "é".repeat(122))),
            Some(AttachmentKind::Other),
        ),
        (json!(format!("application/{}", "é".repeat(123))), None),
        (Value::Null, None),
        (json!(false), None),
        (json!({"type": "application/pdf"}), None),
    ] {
        let item = json!({
            "key": "SYN00301",
            "links": {"attachment": {"attachmentType": mime}},
            "data": {"itemType": "book"},
            "csljson": {"type": "book", "title": "Synthetic Unicode metadata"}
        });
        for items in [vec![item.clone()], as_local_api(&[item])] {
            let loaded = load_items(Library::Group(7), &items);
            assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
            assert_eq!(loaded.references.len(), 1);
            assert_eq!(loaded.references[0].attachment, expected);
            assert_eq!(loaded.references[0].id.source, "zotero:group:7");
        }
    }
}

#[test]
fn attachment_kinds_use_the_existing_single_page_request() {
    use ibis_core::zotero::api::{Response, Transport, TransportError, scan};

    struct OnePage {
        response: Option<Response>,
        asked: Vec<String>,
    }
    impl Transport for OnePage {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            self.asked.push(path.to_owned());
            self.response.take().ok_or(TransportError::Unreachable)
        }
    }

    let mut items = fixture("attachments.json")[..2].to_vec();
    items[0]["csljson"]["title"] = json!("日本 < b > 한국어");
    for items in [items.clone(), as_local_api(&items)] {
        let mut transport = OnePage {
            response: Some(Response {
                status: 200,
                headers: vec![
                    ("Zotero-API-Version".into(), "3".into()),
                    ("Last-Modified-Version".into(), "8".into()),
                    ("Total-Results".into(), "2".into()),
                    ("Zotero-Server-ID".into(), "synthetic-server".into()),
                ],
                body: serde_json::to_vec(&items).unwrap_or_else(|error| panic!("{error}")),
            }),
            asked: Vec::new(),
        };
        let (loaded, _) = scan(&mut transport, Library::Group(7)).unwrap_or_else(|error| panic!("{error}"));
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.references.len(), 2);
        assert_eq!(kind_of(&loaded.references, "SYN00001"), Some(AttachmentKind::Pdf));
        assert_eq!(kind_of(&loaded.references, "SYN00002"), Some(AttachmentKind::WebPage));
        assert_eq!(loaded.references[0].title_plain().as_deref(), Some("日本 < b > 한국어"));
        assert_eq!(
            transport.asked,
            ["groups/7/items/top?format=json&include=data,csljson&sort=dateAdded&direction=asc&limit=100&start=0"]
        );
    }
}
