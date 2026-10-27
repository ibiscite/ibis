use super::*;
use std::error::Error;
use std::io::{BufRead, BufReader, Read};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;

type Result<T = ()> = std::result::Result<T, Box<dyn Error>>;

/// A one-request-per-answer stand-in for Zotero. Each answer is a status and a
/// JSON body; the requests it saw come back on the channel.
fn zotero(answers: Vec<(u16, String)>) -> Result<(u16, mpsc::Receiver<String>)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    let (sent, seen) = mpsc::channel();
    thread::spawn(move || {
        for (status, body) in answers {
            let Ok((stream, _)) = listener.accept() else { return };
            let mut reader = BufReader::new(stream);
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let mut request = vec![0; length];
            if reader.read_exact(&mut request).is_err() {
                return;
            }
            let _ = sent.send(head + &String::from_utf8_lossy(&request));
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let _ = reader.get_mut().write_all(reply.as_bytes());
        }
    });
    Ok((port, seen))
}

fn result(value: Value) -> (u16, String) {
    (200, json!({"jsonrpc": "2.0", "id": 1, "result": value}).to_string())
}

#[test]
fn live_keys_come_from_one_read_only_call_per_batch() -> Result {
    let (port, seen) = zotero(vec![result(json!({"ABCD2345": "Synthetic2026", "EFGH6789": null}))])?;
    let keys = LiveBbt::on_port(port).keys(None, &["ABCD2345", "EFGH6789", "bad:key"])?;
    assert_eq!(
        keys,
        BTreeMap::from([("ABCD2345".to_owned(), "Synthetic2026".to_owned())])
    );

    let request = seen.recv()?;
    assert!(request.starts_with("POST /better-bibtex/json-rpc HTTP/1.1\r\nHost: 127.0.0.1:"));
    assert!(!request.to_ascii_lowercase().contains("origin:"));
    let body: Value = serde_json::from_str(request.split("\r\n\r\n").nth(1).ok_or("body")?)?;
    assert_eq!(body["method"], "item.citationkey");
    // The malformed item key is never sent.
    assert_eq!(body["params"], json!([["ABCD2345", "EFGH6789"]]));
    Ok(())
}

#[test]
fn group_keys_are_addressed_by_local_library_id() -> Result {
    let (port, seen) = zotero(vec![result(json!({"7:ABCD2345": "Group2026"}))])?;
    let keys = LiveBbt::on_port(port).keys(Some(7), &["ABCD2345"])?;
    assert_eq!(keys.get("ABCD2345").map(String::as_str), Some("Group2026"));
    assert!(seen.recv()?.contains(r#""params":[["7:ABCD2345"]]"#));
    Ok(())
}

#[test]
fn libraries_are_matched_to_groups_only_by_a_unique_name() -> Result {
    let (port, _) = zotero(vec![result(json!([
        {"id": 1, "name": "My Library", "collections": null},
        {"id": 4, "name": "Synthetic Group", "collections": null},
        {"id": 5, "name": "Shared Name", "collections": null},
        {"id": 6, "name": "Shared Name", "collections": null},
    ]))])?;
    let libraries = LiveBbt::on_port(port).libraries()?;
    assert_eq!(library_named(&libraries, "Synthetic Group"), Some(4));
    assert_eq!(library_named(&libraries, "Shared Name"), None);
    assert_eq!(library_named(&libraries, "Missing"), None);
    Ok(())
}

#[test]
fn zotero_without_better_bibtex_is_unavailable_not_an_error() -> Result {
    let (port, _) = zotero(vec![
        (404, "No endpoint found".to_owned()),
        (
            200,
            json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32601, "message": "x"}}).to_string(),
        ),
        (
            200,
            json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32603, "message": "x"}}).to_string(),
        ),
        (302, "{}".to_owned()),
    ])?;
    let live = LiveBbt::on_port(port);
    assert_eq!(live.keys(None, &["ABCD2345"]), Err(BbtError::Unavailable));
    assert_eq!(live.keys(None, &["ABCD2345"]), Err(BbtError::Unavailable));
    assert_eq!(live.keys(None, &["ABCD2345"]), Err(BbtError::Malformed));
    assert_eq!(live.libraries(), Err(BbtError::Malformed));
    Ok(())
}

#[test]
fn a_closed_zotero_is_unreachable() -> Result {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?.local_addr()?.port();
    assert_eq!(
        LiveBbt::on_port(port).libraries(),
        Err(BbtError::Transport(TransportError::Unreachable))
    );
    Ok(())
}

#[test]
fn many_keys_are_sent_in_batches() -> Result {
    let item_keys: Vec<String> = (0..BATCH + 1).map(|n| format!("A{n:07}")).collect();
    let keys: Vec<&str> = item_keys.iter().map(String::as_str).collect();
    let (port, seen) = zotero(vec![result(json!({})), result(json!({"A0005000": "Last2026"}))])?;
    let found = LiveBbt::on_port(port).keys(None, &keys)?;
    assert_eq!(found.len(), 1);
    assert!(seen.recv()?.contains("A0004999"));
    assert!(seen.recv()?.contains(r#"[["A0005000"]]"#));
    Ok(())
}

#[test]
fn the_closed_store_is_read_from_its_citationkey_table() -> Result {
    let store = rusqlite::Connection::open_in_memory()?;
    store.execute_batch(
        "CREATE TABLE citationkey (itemID INTEGER PRIMARY KEY, itemKey TEXT, libraryID INTEGER,
                                   citationKey TEXT, pinned INTEGER);
         INSERT INTO citationkey VALUES (1, 'ABCD2345', 1, 'Synthetic2026', 0),
                                        (2, 'EFGH6789', 3, 'Group2026', 1),
                                        (3, 'IJKL2345', 1, '', 0),
                                        (4, 'MNPQ6789', 1, NULL, 0);",
    )?;
    assert_eq!(
        read_store(&store)?,
        vec![
            StoredKey {
                library_id: 1,
                item_key: "ABCD2345".into(),
                citation_key: "Synthetic2026".into()
            },
            StoredKey {
                library_id: 3,
                item_key: "EFGH6789".into(),
                citation_key: "Group2026".into()
            },
        ]
    );
    Ok(())
}

#[test]
fn an_older_or_damaged_store_is_refused() -> Result {
    let older = rusqlite::Connection::open_in_memory()?;
    older.execute_batch("CREATE TABLE \"better-bibtex\" (name TEXT, data TEXT);")?;
    assert_eq!(read_store(&older), Err(BbtError::Unavailable));

    let damaged = rusqlite::Connection::open_in_memory()?;
    damaged.execute_batch(
        "CREATE TABLE citationkey (itemKey TEXT, libraryID INTEGER, citationKey TEXT);
         INSERT INTO citationkey VALUES ('not a key', 1, 'Synthetic2026');",
    )?;
    assert_eq!(read_store(&damaged), Err(BbtError::Malformed));
    Ok(())
}

#[test]
fn read_only_keys_are_parsed_and_empty_ones_skipped() -> Result {
    let json = br#"[
        {"itemID": 9, "libraryID": 4, "itemKey": "ABCD2345", "citationKey": "Shadow2026"},
        {"itemID": 10, "libraryID": 4, "itemKey": "EFGH6789", "citationKey": ""}
    ]"#;
    assert_eq!(
        read_only_keys(json)?,
        vec![StoredKey {
            library_id: 4,
            item_key: "ABCD2345".into(),
            citation_key: "Shadow2026".into()
        }]
    );
    assert_eq!(read_only_keys(b"{}"), Err(BbtError::Malformed));
    assert_eq!(
        read_only_keys(br#"[{"itemKey": "ABCD2345"}]"#),
        Err(BbtError::Malformed)
    );
    Ok(())
}
