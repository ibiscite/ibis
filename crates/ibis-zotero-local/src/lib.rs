//! The one network connection Ibis makes: to Zotero on this computer.
//!
//! Requests go to `127.0.0.1` only, as plain HTTP/1.1 written by hand. A
//! general HTTP client is deliberately not used: it would bring proxy
//! settings, redirects, TLS and connection pooling, none of which this may
//! use. Requests carry no browser `Origin` and no browser User-Agent (Zotero
//! refuses browser-like requests), follow no redirects and give up quickly.
//! The rules of the conversation are in [`ibis_core::zotero::api`].
//!
//! Whatever listens on the port is believed to be Zotero, as with any client
//! of Zotero's local API; requests carry nothing secret, and answers are
//! checked for `Zotero-API-Version: 3` and bounded in size and time.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use ibis_core::zotero::api::{Response, Transport, TransportError};

/// Zotero's local API port.
pub const DEFAULT_PORT: u16 = 23119;
/// Largest answer accepted: one page of items is far smaller.
pub const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_HEADER_BYTES: usize = 64 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const IO_TIMEOUT: Duration = Duration::from_secs(15);
/// The whole answer must arrive within this time, however it trickles in.
const DEADLINE: Duration = Duration::from_secs(60);

/// Zotero's local API on this computer.
#[derive(Debug, Clone, Copy)]
pub struct LocalZotero {
    port: u16,
}

impl Default for LocalZotero {
    fn default() -> Self {
        LocalZotero { port: DEFAULT_PORT }
    }
}

impl LocalZotero {
    /// Another port on this computer, for tests.
    pub fn on_port(port: u16) -> Self {
        LocalZotero { port }
    }
}

impl Transport for LocalZotero {
    fn get(&mut self, path: &str) -> Result<Response, TransportError> {
        // The path is ours, but never let it change the request's shape.
        if !path.bytes().all(|b| b.is_ascii_graphic()) || path.starts_with('/') {
            return Err(TransportError::Malformed);
        }
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        // A refused connection can also time out (Windows retries it), so
        // both mean Zotero is not there.
        let mut stream =
            TcpStream::connect_timeout(&address, CONNECT_TIMEOUT).map_err(|_| TransportError::Unreachable)?;
        stream
            .set_read_timeout(Some(IO_TIMEOUT))
            .map_err(|_| TransportError::Unreachable)?;
        stream
            .set_write_timeout(Some(IO_TIMEOUT))
            .map_err(|_| TransportError::Unreachable)?;
        let request = format!(
            "GET /api/{path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUser-Agent: Ibis/{}\r\n\
             Zotero-API-Version: 3\r\nAccept: application/json\r\nAccept-Encoding: identity\r\n\
             Connection: close\r\n\r\n",
            self.port,
            env!("CARGO_PKG_VERSION")
        );
        stream.write_all(request.as_bytes()).map_err(io_error)?;
        read_response(Deadline {
            stream,
            until: Instant::now() + DEADLINE,
        })
    }
}

/// A stream whose reads give up once the overall deadline has passed, so a
/// peer sending a byte now and then cannot keep a request open.
struct Deadline {
    stream: TcpStream,
    until: Instant,
}

impl Read for Deadline {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let left = self.until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left.min(IO_TIMEOUT)))?;
        self.stream.read(buffer)
    }
}

fn io_error(error: std::io::Error) -> TransportError {
    match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => TransportError::TimedOut,
        std::io::ErrorKind::InvalidData | std::io::ErrorKind::UnexpectedEof => TransportError::Malformed,
        _ => TransportError::Unreachable,
    }
}

fn read_response(stream: impl Read) -> Result<Response, TransportError> {
    let mut reader = BufReader::new(stream.take(MAX_RESPONSE_BYTES + MAX_HEADER_BYTES as u64));
    let mut header_bytes = 0;
    let mut line = || -> Result<String, TransportError> {
        let mut buffer = Vec::new();
        let read = reader
            .by_ref()
            .take((MAX_HEADER_BYTES - header_bytes) as u64)
            .read_until(b'\n', &mut buffer)
            .map_err(io_error)?;
        header_bytes += read;
        if !buffer.ends_with(b"\n") {
            return Err(if header_bytes >= MAX_HEADER_BYTES {
                TransportError::TooLarge
            } else {
                TransportError::Malformed
            });
        }
        let text = String::from_utf8(buffer).map_err(|_| TransportError::Malformed)?;
        Ok(text.trim_end_matches(['\r', '\n']).to_string())
    };

    let status_line = line()?;
    let mut parts = status_line.splitn(3, ' ');
    let (Some(version), Some(code)) = (parts.next(), parts.next()) else {
        return Err(TransportError::Malformed);
    };
    if !version.starts_with("HTTP/1.") || code.len() != 3 {
        return Err(TransportError::Malformed);
    }
    let status = digits(code)
        .and_then(|code| u16::try_from(code).ok())
        .ok_or(TransportError::Malformed)?;
    let mut headers = Vec::new();
    loop {
        let header = line()?;
        if header.is_empty() {
            break;
        }
        let (name, value) = header.split_once(':').ok_or(TransportError::Malformed)?;
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    let find = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    };
    // Only plain chunked framing is understood; anything else could be
    // misread as the body.
    let chunked = match find("Transfer-Encoding") {
        None => false,
        Some(v) if v.eq_ignore_ascii_case("chunked") => true,
        Some(_) => return Err(TransportError::Malformed),
    };
    let mut lengths = headers
        .iter()
        .filter(|(n, _)| n.eq_ignore_ascii_case("Content-Length"))
        .map(|(_, v)| digits(v).ok_or(TransportError::Malformed));
    let length = lengths.next().transpose()?;
    if lengths.any(|other| other != length.ok_or(TransportError::Malformed)) {
        return Err(TransportError::Malformed);
    }
    let mut reader = reader.take(MAX_RESPONSE_BYTES + 1);
    let body = if chunked {
        read_chunked(&mut reader)?
    } else if let Some(length) = length {
        if length > MAX_RESPONSE_BYTES {
            return Err(TransportError::TooLarge);
        }
        let mut body = Vec::new();
        reader.by_ref().take(length).read_to_end(&mut body).map_err(io_error)?;
        if body.len() as u64 != length {
            return Err(TransportError::Malformed);
        }
        body
    } else {
        let mut body = Vec::new();
        reader.read_to_end(&mut body).map_err(io_error)?;
        body
    };
    if body.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(TransportError::TooLarge);
    }
    Ok(Response { status, headers, body })
}

/// A decimal number written with digits only (no sign or spaces).
fn digits(text: &str) -> Option<u64> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

fn read_chunked(reader: &mut impl BufRead) -> Result<Vec<u8>, TransportError> {
    let mut body = Vec::new();
    loop {
        let mut size_line = String::new();
        reader.by_ref().take(1024).read_line(&mut size_line).map_err(io_error)?;
        let size_text = size_line.split(';').next().unwrap_or_default().trim();
        if size_text.is_empty() || !size_text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(TransportError::Malformed);
        }
        let size = u64::from_str_radix(size_text, 16).map_err(|_| TransportError::TooLarge)?;
        if size == 0 {
            return Ok(body);
        }
        if size > MAX_RESPONSE_BYTES - body.len() as u64 {
            return Err(TransportError::TooLarge);
        }
        let before = body.len();
        reader.by_ref().take(size).read_to_end(&mut body).map_err(io_error)?;
        if (body.len() - before) as u64 != size {
            return Err(TransportError::Malformed);
        }
        let mut end = [0u8; 2];
        reader.read_exact(&mut end).map_err(io_error)?;
        if &end != b"\r\n" {
            return Err(TransportError::Malformed);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::TcpListener;
    use std::thread;

    use super::*;

    /// A one-shot fake Zotero on a free loopback port: returns `answer` and
    /// hands back the request it received.
    fn serve(answer: &'static [u8]) -> (LocalZotero, thread::JoinHandle<String>) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap_or_else(|e| panic!("{e}"));
        let port = listener.local_addr().unwrap_or_else(|e| panic!("{e}")).port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap_or_else(|e| panic!("{e}"));
            let mut request = Vec::new();
            let mut buffer = [0u8; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                let n = stream.read(&mut buffer).unwrap_or_else(|e| panic!("{e}"));
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..n]);
            }
            stream.write_all(answer).unwrap_or_else(|e| panic!("{e}"));
            String::from_utf8_lossy(&request).into_owned()
        });
        (LocalZotero::on_port(port), server)
    }

    fn request_of(server: thread::JoinHandle<String>) -> String {
        server.join().unwrap_or_else(|_| panic!("fake server failed"))
    }

    #[test]
    fn requests_are_plain_loopback_gets_without_browser_headers() {
        let (mut zotero, server) = serve(b"HTTP/1.1 200 OK\r\nZotero-API-Version: 3\r\nContent-Length: 2\r\n\r\n[]");
        let response = zotero.get("users/0/items?limit=1").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(response.status, 200);
        assert_eq!(response.header("zotero-api-version"), Some("3"));
        assert_eq!(response.body, b"[]");
        let request = request_of(server);
        assert!(request.starts_with("GET /api/users/0/items?limit=1 HTTP/1.1\r\n"));
        assert!(request.contains("\r\nHost: 127.0.0.1:"));
        assert!(request.contains("\r\nZotero-API-Version: 3\r\n"));
        assert!(request.contains("\r\nUser-Agent: Ibis/"));
        assert!(!request.to_ascii_lowercase().contains("origin:"));
        assert!(!request.contains("Mozilla"));
    }

    #[test]
    fn chunked_answers_are_reassembled() {
        let (mut zotero, server) =
            serve(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n[{}\r\n1;x=y\r\n]\r\n0\r\n\r\n");
        let response = zotero.get("users/0/items").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(response.body, b"[{}]");
        request_of(server);
    }

    #[test]
    fn redirects_are_returned_not_followed() {
        let (mut zotero, server) =
            serve(b"HTTP/1.1 302 Found\r\nLocation: http://example.invalid/\r\nContent-Length: 0\r\n\r\n");
        let response = zotero.get("users/0/items").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(response.status, 302);
        request_of(server);
    }

    #[test]
    fn broken_answers_are_errors() {
        for answer in [
            &b"not http\r\n\r\n"[..],
            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\nshort",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
            b"HTTP/1.1 200 OK\r\nno colon here\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nContent-Length: 99999999999\r\n\r\n",
        ] {
            let (mut zotero, server) = serve(answer);
            assert!(zotero.get("users/0/items").is_err());
            request_of(server);
        }
    }

    #[test]
    fn an_unavailable_port_means_zotero_is_not_running() {
        // Binding zero allocates a nonzero port; connecting to zero fails.
        // A released ephemeral port could instead be claimed by another test.
        assert_eq!(
            LocalZotero::on_port(0).get("users/0/items").err(),
            Some(TransportError::Unreachable)
        );
    }

    #[test]
    fn paths_cannot_inject_request_lines() {
        let mut zotero = LocalZotero::on_port(1);
        assert_eq!(
            zotero.get("users/0/items HTTP/1.1\r\nOrigin: x").err(),
            Some(TransportError::Malformed)
        );
        assert_eq!(zotero.get("/absolute").err(), Some(TransportError::Malformed));
    }

    #[test]
    fn oversized_headers_are_refused() {
        let answer: &'static [u8] = Box::leak(
            format!("HTTP/1.1 200 OK\r\nX: {}\r\n\r\n", "a".repeat(MAX_HEADER_BYTES))
                .into_bytes()
                .into_boxed_slice(),
        );
        let (mut zotero, server) = serve(answer);
        assert_eq!(zotero.get("users/0/items").err(), Some(TransportError::TooLarge));
        request_of(server);
    }

    #[test]
    fn huge_chunk_sizes_and_unknown_framing_are_errors_not_crashes() {
        for answer in [
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\n[]\r\nffffffffffffffff\r\n"[..],
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1ffffffffffffffff\r\n",
        ] {
            assert_eq!(read_response(answer).err(), Some(TransportError::TooLarge));
        }
        for answer in [
            &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n2\r\n[]\r\n0\r\n\r\n"[..],
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nContent-Length: 3\r\n\r\n[]",
            b"HTTP/1.1 200 OK\r\nContent-Length: +2\r\n\r\n[]",
            b"HTTP/1.1 +20 OK\r\n\r\n",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\xff\r\n",
        ] {
            assert_eq!(read_response(answer).err(), Some(TransportError::Malformed));
        }
        let extension =
            read_response(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2 ;x=y\r\n[]\r\n0\r\n\r\n"[..]);
        assert_eq!(extension.map(|r| r.body).ok(), Some(b"[]".to_vec()));
    }

    #[test]
    fn a_trickling_answer_stops_at_the_overall_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap_or_else(|e| panic!("{e}"));
        let address = listener.local_addr().unwrap_or_else(|e| panic!("{e}"));
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap_or_else(|e| panic!("{e}"));
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\n");
            for _ in 0..40 {
                thread::sleep(Duration::from_millis(50));
                if stream.write_all(b"X").is_err() {
                    break;
                }
            }
        });
        let stream = TcpStream::connect(address).unwrap_or_else(|e| panic!("{e}"));
        let started = Instant::now();
        let outcome = read_response(Deadline {
            stream,
            until: Instant::now() + Duration::from_millis(300),
        });
        assert_eq!(outcome.err(), Some(TransportError::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(2));
        let _ = server.join();
    }
}
