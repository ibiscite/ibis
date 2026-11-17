//! Reads a real, running Zotero through its local API.
//!
//! Ignored by default: `scripts/real-zotero.sh` downloads Zotero, starts it
//! with a fresh profile and the local API turned on, then runs this test with
//! `--ignored`. The test adds synthetic items through Zotero's connector
//! endpoint, so what Ibis reads is exactly what Zotero itself sends, not a
//! hand-written imitation of it.
#![cfg(test)]

use std::error::Error;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::{Duration, Instant};

use ibis_zotero_local::live::{Library, LiveReader, Refresh};
use ibis_zotero_local::{DEFAULT_PORT, LocalZotero};

const ARTICLE: &str = r#"{
  "itemType": "journalArticle",
  "title": "Sleep and memory consolidation in synthetic subjects",
  "creators": [
    {"firstName": "Ada", "lastName": "Example", "creatorType": "author"},
    {"firstName": "Ben", "lastName": "Sample", "creatorType": "author"}
  ],
  "date": "2020-03-14",
  "publicationTitle": "Journal of Synthetic Studies",
  "volume": "12",
  "pages": "101-120",
  "DOI": "10.0000/synthetic.1",
  "extra": "Citation Key: example2020sleep\nIbis synthetic raw Extra marker."
}"#;

const BOOK: &str = r#"{
  "itemType": "book",
  "title": "Résumé des données: 数据与记忆",
  "creators": [{"name": "Synthetic Institute", "creatorType": "author"}],
  "date": "2019",
  "publisher": "Example Press",
  "extra": "Citation Key: institute2019\nIbis synthetic raw Extra marker."
}"#;

const KEYLESS: &str = r#"{
  "itemType": "report",
  "title": "A report without a citation key",
  "creators": [{"firstName": "Chloé", "lastName": "Placeholder", "creatorType": "author"}],
  "date": "2021"
}"#;

const LATER: &str = r#"{
  "itemType": "journalArticle",
  "title": "Added while Ibis was watching",
  "creators": [{"firstName": "Dana", "lastName": "Stand-In", "creatorType": "author"}],
  "date": "2022",
  "extra": "Citation Key: standin2022\nIbis synthetic raw Extra marker."
}"#;

/// The throwaway Zotero's port. The script sets it; there is no default,
/// so the test cannot write into a Zotero someone is using.
fn port() -> u16 {
    std::env::var("IBIS_ZOTERO_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .filter(|port| *port != DEFAULT_PORT)
        .unwrap_or(0)
}

/// Saves items the way the browser connector does, into the library
/// selected in Zotero (My Library in a fresh profile).
fn save(items: &[&str], session: &str) -> Result<(), Box<dyn Error>> {
    let body = format!(
        r#"{{"items": [{}], "uri": "https://example.org/synthetic", "sessionID": "{session}"}}"#,
        items.join(",")
    );
    let mut stream = TcpStream::connect(("127.0.0.1", port()))?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    capture_guard(&stream)?;
    write!(
        stream,
        "POST /connector/saveItems HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        port(),
        body.len()
    )?;
    let mut answer = String::new();
    stream.read_to_string(&mut answer)?;
    let status = answer.split(' ').nth(1).unwrap_or_default();
    assert!(
        status == "201" || status == "200",
        "Zotero refused the synthetic items: {answer}"
    );
    Ok(())
}

/// Scans until Zotero lists `count` items; saving finishes after the
/// connector answers.
fn scan_until(reader: &mut LiveReader, count: usize) {
    let started = Instant::now();
    loop {
        let scanned = reader.refresh(&mut LocalZotero::on_port(port()), false, &|| false);
        match scanned {
            Ok(_) if reader.items().len() >= count => return,
            other if started.elapsed() > Duration::from_secs(60) => {
                panic!("Zotero did not list {count} synthetic items within a minute: {other:?}")
            }
            _ => thread::sleep(Duration::from_millis(500)),
        }
    }
}

#[test]
#[ignore = "needs a running Zotero; see scripts/real-zotero.sh"]
fn ibis_reads_what_a_real_zotero_sends() -> Result<(), Box<dyn Error>> {
    if std::env::var_os("IBIS_THROWAWAY_ZOTERO").is_none() || port() == 0 {
        return Err("run this through scripts/real-zotero.sh, which starts a throwaway Zotero".into());
    }
    let mut reader = LiveReader::new(Library::User);
    reader.refresh(&mut LocalZotero::on_port(port()), true, &|| false)?;
    if !reader.items().is_empty() {
        return Err("this Zotero library is not empty; refusing to add synthetic items".into());
    }
    save(&[ARTICLE, BOOK, KEYLESS], "ibis-synthetic-1")?;
    scan_until(&mut reader, 3);
    let items = reader.items();
    assert_eq!(items.len(), 3);
    // The mapper's differential suite separately verifies normalization.
    let article = items
        .iter()
        .find(|item| {
            item.fields
                .get("title")
                .is_some_and(|title| title == "Sleep and memory consolidation in synthetic subjects")
        })
        .ok_or("the article was not listed")?;
    assert!(
        has_key(article, "example2020sleep"),
        "the article lost its native/Extra key"
    );
    assert!(
        article
            .fields
            .get("extra")
            .is_some_and(|extra| extra.contains("Ibis synthetic raw Extra marker.")),
        "the article lost its ordinary raw Extra field"
    );
    assert!(
        article
            .fields
            .get("title")
            .is_some_and(|title| title.contains("Sleep and memory"))
    );
    let families: Vec<_> = article
        .creators
        .iter()
        .map(|creator| creator.last_name.as_str())
        .collect();
    assert_eq!(families, ["Example", "Sample"]);
    assert_eq!(article.fields.get("date").map(String::as_str), Some("2020-03-14"));
    assert_eq!(article.item_type, "journalArticle");
    assert_eq!(article.key.len(), 8);
    let book = items
        .iter()
        .find(|item| {
            item.fields
                .get("title")
                .is_some_and(|title| title == "Résumé des données: 数据与记忆")
        })
        .ok_or("the book was not listed")?;
    assert!(has_key(book, "institute2019"), "the book lost its native/Extra key");
    assert!(
        book.fields
            .get("extra")
            .is_some_and(|extra| extra.contains("Ibis synthetic raw Extra marker."))
    );
    assert!(
        book.fields
            .get("title")
            .is_some_and(|title| title.contains("数据与记忆"))
    );
    assert_eq!(book.item_type, "book");
    assert!(
        book.creators
            .iter()
            .any(|creator| creator.name == "Synthetic Institute")
    );
    let keyless = items
        .iter()
        .find(|item| {
            item.fields
                .get("title")
                .is_some_and(|title| title.contains("without a citation key"))
        })
        .ok_or("the keyless item was not listed")?;
    assert!(keyless.fields.get("citationKey").is_none_or(String::is_empty));
    assert_eq!(
        reader.refresh(&mut LocalZotero::on_port(port()), false, &|| false)?,
        Refresh::Unchanged
    );
    save(&[LATER], "ibis-synthetic-2")?;
    scan_until(&mut reader, 4);
    let items = reader.items();
    assert_eq!(items.len(), 4);
    assert!(items.iter().any(|item| {
        has_key(item, "standin2022")
            && item
                .fields
                .get("extra")
                .is_some_and(|extra| extra.contains("Ibis synthetic raw Extra marker."))
    }));
    Ok(())
}

// Zotero 10 migrates recognized Extra key lines into the native key field.
// Ordinary Extra prose must survive independently of that migration.
fn has_key(item: &ibis_zotero_local::live::ItemData, key: &str) -> bool {
    item.fields.get("citationKey").is_some_and(|value| value == key)
        || item.fields.get("extra").is_some_and(|extra| {
            extra.lines().any(|line| {
                line.trim()
                    .strip_prefix("Citation Key:")
                    .is_some_and(|value| value.trim() == key)
            })
        })
}

const CAPTURE_OWNERSHIP_ERROR: &str = "Zotero capture socket ownership could not be verified.";

fn capture_identity(
    pid: Option<&std::ffi::OsStr>,
    start: Option<&std::ffi::OsStr>,
) -> Result<Option<(u32, u64)>, &'static str> {
    if pid.is_none() && start.is_none() {
        return Ok(None);
    }
    let number = |value: Option<&std::ffi::OsStr>| {
        value
            .and_then(|value| value.to_str())
            .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|value| *value != 0)
            .ok_or(CAPTURE_OWNERSHIP_ERROR)
    };
    Ok(Some((
        u32::try_from(number(pid)?).map_err(|_| CAPTURE_OWNERSHIP_ERROR)?,
        number(start)?,
    )))
}

/// Capture mode proves ownership of this exact connected TCP stream before POST.
fn capture_guard(stream: &TcpStream) -> Result<(), &'static str> {
    let pid = std::env::var_os("IBIS_ZOTERO_CAPTURE_PID");
    let start = std::env::var_os("IBIS_ZOTERO_CAPTURE_START");
    let Some((pid, start)) = capture_identity(pid.as_deref(), start.as_deref())? else {
        return Ok(());
    };
    if !cfg!(target_os = "linux") {
        return Err(CAPTURE_OWNERSHIP_ERROR);
    }
    let client = stream.local_addr().map_err(|_| CAPTURE_OWNERSHIP_ERROR)?;
    let server = stream.peer_addr().map_err(|_| CAPTURE_OWNERSHIP_ERROR)?;
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        // The server can be ESTABLISHED but still in its accept queue (inode 0).
        let result = verify_capture_socket(std::path::Path::new("/proc"), pid, start, client, server);
        if Instant::now() >= deadline {
            return Err(CAPTURE_OWNERSHIP_ERROR);
        }
        if result.is_ok() {
            return result;
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn capture_text(path: &std::path::Path, limit: u64) -> Result<String, &'static str> {
    let file = std::fs::File::open(path).map_err(|_| CAPTURE_OWNERSHIP_ERROR)?;
    capture_read(file, limit)
}

fn capture_read(file: std::fs::File, limit: u64) -> Result<String, &'static str> {
    let mut text = String::new();
    file.take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|_| CAPTURE_OWNERSHIP_ERROR)?;
    if text.len() as u64 > limit {
        return Err(CAPTURE_OWNERSHIP_ERROR);
    }
    Ok(text)
}

fn capture_process(root: &std::path::Path, pid: u32) -> Result<(u32, u64), &'static str> {
    let text = capture_text(&root.join(format!("{pid}/stat")), 64 * 1024)?;
    let fields: Vec<_> = text
        .rsplit_once(") ")
        .ok_or(CAPTURE_OWNERSHIP_ERROR)?
        .1
        .split_whitespace()
        .collect();
    Ok((
        fields
            .get(1)
            .and_then(|value| value.parse().ok())
            .ok_or(CAPTURE_OWNERSHIP_ERROR)?,
        fields
            .get(19)
            .and_then(|value| value.parse().ok())
            .ok_or(CAPTURE_OWNERSHIP_ERROR)?,
    ))
}

fn capture_address(text: &str) -> Result<std::net::SocketAddr, &'static str> {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    let (address, port) = text.rsplit_once(':').ok_or(CAPTURE_OWNERSHIP_ERROR)?;
    if !matches!(address.len(), 8 | 32)
        || port.len() != 4
        || !address.bytes().chain(port.bytes()).all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(CAPTURE_OWNERSHIP_ERROR);
    }
    let mut bytes = Vec::new();
    for word in address.as_bytes().as_chunks::<8>().0 {
        let word = std::str::from_utf8(word).map_err(|_| CAPTURE_OWNERSHIP_ERROR)?;
        bytes.extend(
            u32::from_str_radix(word, 16)
                .map_err(|_| CAPTURE_OWNERSHIP_ERROR)?
                .to_ne_bytes(),
        );
    }
    let ip = if bytes.len() == 4 {
        IpAddr::V4(Ipv4Addr::new(bytes[0], bytes[1], bytes[2], bytes[3]))
    } else {
        let ip = Ipv6Addr::from(<[u8; 16]>::try_from(bytes).map_err(|_| CAPTURE_OWNERSHIP_ERROR)?);
        ip.to_ipv4_mapped().map(IpAddr::V4).unwrap_or(IpAddr::V6(ip))
    };
    Ok(SocketAddr::new(
        ip,
        u16::from_str_radix(port, 16).map_err(|_| CAPTURE_OWNERSHIP_ERROR)?,
    ))
}

fn capture_sockets(
    root: &std::path::Path,
    client: std::net::SocketAddr,
    server: std::net::SocketAddr,
) -> Result<std::collections::BTreeSet<u64>, &'static str> {
    let mut listeners = std::collections::BTreeSet::new();
    let mut accepted = std::collections::BTreeSet::new();
    for table in ["tcp", "tcp6"] {
        let file = match std::fs::File::open(root.join("net").join(table)) {
            Ok(file) => file,
            Err(error) if table == "tcp6" && error.kind() == std::io::ErrorKind::NotFound => {
                continue;
            }
            Err(_) => return Err(CAPTURE_OWNERSHIP_ERROR),
        };
        for line in capture_read(file, 1024 * 1024)?
            .lines()
            .skip(1)
            .filter(|line| !line.trim().is_empty())
        {
            let fields: Vec<_> = line.split_whitespace().collect();
            let state = fields.get(3).ok_or(CAPTURE_OWNERSHIP_ERROR)?;
            if !matches!(*state, "0A" | "01") {
                continue;
            }
            let local = capture_address(fields.get(1).ok_or(CAPTURE_OWNERSHIP_ERROR)?)?;
            if local.port() != server.port() {
                continue;
            }
            let remote = capture_address(fields.get(2).ok_or(CAPTURE_OWNERSHIP_ERROR)?)?;
            let inode = fields
                .get(9)
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or(CAPTURE_OWNERSHIP_ERROR)?;
            if *state == "0A" {
                listeners.insert(inode);
            }
            if *state == "01" && local == server && remote == client && inode != 0 {
                accepted.insert(inode);
            }
        }
    }
    if listeners.is_empty() || listeners.contains(&0) || accepted.is_empty() {
        return Err(CAPTURE_OWNERSHIP_ERROR);
    }
    listeners.extend(accepted);
    Ok(listeners)
}

fn verify_capture_socket(
    root: &std::path::Path,
    pid: u32,
    start: u64,
    client: std::net::SocketAddr,
    server: std::net::SocketAddr,
) -> Result<(), &'static str> {
    use std::collections::{BTreeSet, VecDeque};
    if !client.ip().is_loopback()
        || !server.ip().is_loopback()
        || client.port() == 0
        || server.port() == 0
        || capture_process(root, pid)?.1 != start
    {
        return Err(CAPTURE_OWNERSHIP_ERROR);
    }
    let required = capture_sockets(root, client, server)?;
    let mut missing = required.clone();
    let mut queue = VecDeque::from([(pid, None)]);
    let mut discovered = BTreeSet::from([pid]);
    let (mut descriptors, mut tasks) = (0, 0);
    while let Some((process, parent)) = queue.pop_front() {
        let Ok(identity) = capture_process(root, process) else {
            continue;
        };
        if parent.is_some_and(|parent| identity.0 != parent) {
            continue;
        }
        let mut found = BTreeSet::new();
        if let Ok(entries) = std::fs::read_dir(root.join(format!("{process}/fd"))) {
            for entry in entries {
                descriptors += 1;
                if descriptors > 4096 {
                    return Err(CAPTURE_OWNERSHIP_ERROR);
                }
                let Ok(target) = entry.and_then(|entry| std::fs::read_link(entry.path())) else {
                    continue;
                };
                if let Some(inode) = target
                    .to_str()
                    .and_then(|value| value.strip_prefix("socket:["))
                    .and_then(|value| value.strip_suffix(']'))
                    .and_then(|value| value.parse().ok())
                {
                    found.insert(inode);
                }
                if missing.is_subset(&found) {
                    break;
                }
            }
        }
        if capture_process(root, process).ok() != Some(identity) {
            continue;
        }
        missing.retain(|inode| !found.contains(inode));
        if missing.is_empty() {
            return if capture_process(root, pid)?.1 == start && capture_sockets(root, client, server)? == required {
                Ok(())
            } else {
                Err(CAPTURE_OWNERSHIP_ERROR)
            };
        }
        // Children can be forked by any thread, not just the main thread.
        if let Ok(entries) = std::fs::read_dir(root.join(format!("{process}/task"))) {
            for entry in entries {
                tasks += 1;
                if tasks > 4096 {
                    return Err(CAPTURE_OWNERSHIP_ERROR);
                }
                let Ok(entry) = entry else {
                    continue;
                };
                let Ok(children) = capture_text(&entry.path().join("children"), 64 * 1024) else {
                    continue;
                };
                for child in children.split_whitespace() {
                    let child = child.parse::<u32>().map_err(|_| CAPTURE_OWNERSHIP_ERROR)?;
                    if discovered.insert(child) {
                        if discovered.len() > 64 {
                            return Err(CAPTURE_OWNERSHIP_ERROR);
                        }
                        queue.push_back((child, Some(process)));
                    }
                }
            }
        }
    }
    Err(CAPTURE_OWNERSHIP_ERROR)
}
