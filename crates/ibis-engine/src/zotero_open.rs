//! Resolve a PDF through the existing read-only loopback transport. Only a
//! validated Zotero item URI is accepted; uncertainty falls back to the item.
use ibis_core::zotero::api::Transport;

pub fn pdf_uri(item: &str) -> Option<String> {
    with_transport(item, &mut ibis_sources::zotero_source::LocalZotero::default())
}

fn key(value: &str) -> bool {
    value.len() == 8
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn with_transport(item: &str, transport: &mut impl Transport) -> Option<String> {
    let item = item.strip_prefix("zotero://select/")?;
    let (library, parent) = item.split_once("/items/")?;
    if !key(parent) {
        return None;
    }
    let api = if library == "library" {
        "users/0".to_owned()
    } else {
        let group = library.strip_prefix("groups/")?;
        if group.is_empty() || !group.bytes().all(|byte| byte.is_ascii_digit()) || group.parse::<u64>().ok()? == 0 {
            return None;
        }
        format!("groups/{group}")
    };
    let response = transport
        .get(&format!("{api}/items/{parent}/children?itemType=attachment&limit=100"))
        .ok()?;
    if response.status != 200 || response.header("Zotero-API-Version") != Some("3") || response.body.len() > 1024 * 1024
    {
        return None;
    }
    let entries: Vec<serde_json::Value> = serde_json::from_slice(&response.body).ok()?;
    let attachment = entries.iter().take(100).find_map(|entry| {
        let data = entry.get("data")?;
        if data.get("itemType")?.as_str()? != "attachment"
            || data.get("contentType")?.as_str()? != "application/pdf"
            || data.get("parentItem")?.as_str()? != parent
        {
            return None;
        }
        let id = entry.get("key")?.as_str()?;
        key(id).then_some(id)
    })?;
    Some(format!("zotero://open-pdf/{library}/items/{attachment}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibis_core::zotero::api::{Response, TransportError};
    struct Fake(Result<Response, TransportError>, &'static str);
    impl Transport for Fake {
        fn get(&mut self, path: &str) -> Result<Response, TransportError> {
            assert_eq!(path, self.1);
            self.0.clone()
        }
    }
    fn response(body: &str) -> Fake {
        Fake(
            Ok(Response {
                status: 200,
                headers: vec![("Zotero-API-Version".into(), "3".into())],
                body: body.as_bytes().to_vec(),
            }),
            "users/0/items/ABCD1234/children?itemType=attachment&limit=100",
        )
    }
    #[test]
    fn find_opens_a_pdf_and_otherwise_keeps_the_item_available() {
        let item = "zotero://select/library/items/ABCD1234";
        let mut pdf = response(
            r#"[{"key":"PDF12345","data":{"itemType":"attachment","contentType":"application/pdf","parentItem":"ABCD1234"}}]"#,
        );
        assert_eq!(
            with_transport(item, &mut pdf).as_deref(),
            Some("zotero://open-pdf/library/items/PDF12345")
        );
        for body in [
            "[]",
            "not JSON",
            r#"[{"key":"BAD/KEY!","data":{"contentType":"application/pdf","parentItem":"ABCD1234"}}]"#,
            r#"[{"key":"PDF12345","data":{"contentType":"application/pdf","parentItem":"OTHER123"}}]"#,
        ] {
            assert!(with_transport(item, &mut response(body)).is_none());
        }
        assert!(
            with_transport(
                item,
                &mut Fake(
                    Err(TransportError::Unreachable),
                    "users/0/items/ABCD1234/children?itemType=attachment&limit=100"
                )
            )
            .is_none()
        );
        assert!(with_transport("https://example.invalid/private", &mut response("[]")).is_none());
    }
    #[test]
    fn a_group_pdf_keeps_its_library_and_an_invalid_group_cannot_make_a_request() {
        let mut group = response(
            r#"[{"key":"PDF12345","data":{"itemType":"attachment","contentType":"application/pdf","parentItem":"ABCD1234"}}]"#,
        );
        group.1 = "groups/42/items/ABCD1234/children?itemType=attachment&limit=100";
        assert_eq!(
            with_transport("zotero://select/groups/42/items/ABCD1234", &mut group).as_deref(),
            Some("zotero://open-pdf/groups/42/items/PDF12345")
        );
        assert!(with_transport("zotero://select/groups/0/items/ABCD1234", &mut group).is_none());
    }
}
