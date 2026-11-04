//! Requests to the editor endpoint, a Better BibTeX Cite as You Write
//! compatible address on `127.0.0.1` (decision 0008).
//!
//! This is the security boundary, and it is pure: bytes that any program on
//! the computer can send come in, and a typed request or a fixed refusal comes
//! out. Nothing from a request is ever echoed. A browser cannot be told apart
//! by `Origin` alone, because a page's `<img src>` sends none, so a page is kept
//! out four ways: the token in the path, which it cannot read; the headers
//! browsers add to anything they send; `Host`, which DNS rebinding cannot forge;
//! and answers with no CORS headers.

use ibis_core::contract::{CitationForm, KeySyntax, OutputSpec};

/// A request head is small; anything larger is not from an editor.
pub const MAX_HEAD_BYTES: usize = 8 * 1024;
const MAX_HEADERS: usize = 32;
const PATH: &str = "better-bibtex/cayw";

/// Why a request was refused. Fixed text: nothing a client sent is repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Malformed,
    /// Sent by a browser, or addressed through another host name.
    Browser,
    /// No token, or the wrong one.
    Token,
    NotFound,
    Method,
    TooLarge,
    Format,
    Option,
}

impl Refusal {
    pub const fn status(self) -> (u16, &'static str) {
        match self {
            Self::Malformed | Self::Format | Self::Option => (400, "Bad Request"),
            Self::Browser | Self::Token => (403, "Forbidden"),
            Self::NotFound => (404, "Not Found"),
            Self::Method => (405, "Method Not Allowed"),
            Self::TooLarge => (431, "Request Header Fields Too Large"),
        }
    }
    pub const fn message(self) -> &'static str {
        match self {
            Self::Malformed => "This is not a request Ibis can read.",
            Self::Browser => "Ibis does not answer web pages.",
            Self::Token => "Use the address shown in Ibis Settings.",
            Self::NotFound => "Ibis has nothing at this address.",
            Self::Method => "Ibis only answers GET here.",
            Self::TooLarge => "This request is too large.",
            Self::Format => "Ibis cannot write this format.",
            Self::Option => "Ibis does not support this option.",
        }
    }
}

/// What a citation is written as; formatted output uses the person's style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaywOutput {
    Key(OutputSpec),
    Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaywPick {
    pub output: CaywOutput,
    pub form: CitationForm,
    /// Also put the citation on the clipboard.
    pub clipboard: bool,
}

impl CaywPick {
    pub fn spec(&self, style: &str) -> OutputSpec {
        match &self.output {
            CaywOutput::Key(spec) => spec.clone(),
            CaywOutput::Style => OutputSpec::Csl {
                style: style.to_owned(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaywRequest {
    Probe,
    Pick(CaywPick),
}

/// The end of a request head in what has arrived so far.
pub fn head_end(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|end| end + 4)
}

/// Compare without stopping at the first difference.
fn same(given: &[u8], token: &[u8]) -> bool {
    given.len() == token.len() && given.iter().zip(token).fold(0, |differ, (a, b)| differ | (a ^ b)) == 0
}

pub fn parse(head: &[u8], port: u16, token: &str) -> Result<CaywRequest, Refusal> {
    if head.len() > MAX_HEAD_BYTES {
        return Err(Refusal::TooLarge);
    }
    let head = std::str::from_utf8(head).map_err(|_| Refusal::Malformed)?;
    let head = head.strip_suffix("\r\n\r\n").ok_or(Refusal::Malformed)?;
    let mut lines = head.split("\r\n");
    let start = lines.next().ok_or(Refusal::Malformed)?;
    let mut request = start.split(' ');
    let (Some(method), Some(target), Some(version), None) =
        (request.next(), request.next(), request.next(), request.next())
    else {
        return Err(Refusal::Malformed);
    };
    if !matches!(version, "HTTP/1.1" | "HTTP/1.0") {
        return Err(Refusal::Malformed);
    }
    check_headers(lines, port)?;
    if method != "GET" {
        return Err(Refusal::Method);
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let path = path.strip_prefix('/').ok_or(Refusal::Malformed)?;
    let (given, rest) = path.split_once('/').unwrap_or((path, ""));
    // The token is checked before the path, so a wrong one learns nothing else.
    if path == PATH || !same(given.as_bytes(), token.as_bytes()) {
        return Err(Refusal::Token);
    }
    if rest != PATH {
        return Err(Refusal::NotFound);
    }
    options(query)
}

/// Each line a client sends, refusing anything a browser adds or that could
/// frame a second request. No proxy, cookie or body has a place here.
fn check_headers<'a>(lines: impl Iterator<Item = &'a str>, port: u16) -> Result<(), Refusal> {
    let mut host = None;
    for (count, line) in lines.enumerate() {
        if count >= MAX_HEADERS {
            return Err(Refusal::TooLarge);
        }
        let (name, value) = line.split_once(':').ok_or(Refusal::Malformed)?;
        if name.is_empty()
            || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || value.bytes().any(|byte| byte.is_ascii_control() && byte != b'\t')
        {
            return Err(Refusal::Malformed);
        }
        let name = name.to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "origin" | "referer" | "cookie" | "access-control-request-method" | "access-control-request-headers" => {
                return Err(Refusal::Browser);
            }
            _ if name.starts_with("sec-") => return Err(Refusal::Browser),
            "transfer-encoding" | "upgrade" => return Err(Refusal::Malformed),
            "content-length" if value != "0" => return Err(Refusal::Malformed),
            "host" if host.replace(value).is_some() => return Err(Refusal::Malformed),
            _ => {}
        }
    }
    // Under DNS rebinding the browser names the attacker's host, so anything
    // but this computer's own names is a web page.
    let host = host.ok_or(Refusal::Browser)?;
    // Exactly one of this computer's own names and this port, compared as bytes:
    // no slicing can split a character, and `+23124` or `023124` is not 23124.
    let port = port.to_string();
    let named = |name: &str| {
        host.len() == name.len() + 1 + port.len()
            && host.as_bytes()[..name.len()].eq_ignore_ascii_case(name.as_bytes())
            && host.as_bytes()[name.len()] == b':'
            && &host.as_bytes()[name.len() + 1..] == port.as_bytes()
    };
    if named("127.0.0.1") || named("localhost") {
        Ok(())
    } else {
        Err(Refusal::Browser)
    }
}

fn decode(text: &str) -> Result<String, Refusal> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut rest = text.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        rest = after;
        match byte {
            b'%' => {
                let [high, low, tail @ ..] = rest else {
                    return Err(Refusal::Malformed);
                };
                let digit = |byte: u8| char::from(byte).to_digit(16).ok_or(Refusal::Malformed);
                bytes.push((digit(*high)? * 16 + digit(*low)?) as u8);
                rest = tail;
            }
            b'+' => bytes.push(b' '),
            byte => bytes.push(byte),
        }
    }
    let text = String::from_utf8(bytes).map_err(|_| Refusal::Malformed)?;
    if text.chars().any(char::is_control) {
        return Err(Refusal::Malformed);
    }
    Ok(text)
}

fn truthy(value: &str) -> Result<bool, Refusal> {
    match value {
        "" | "1" | "true" | "yes" => Ok(true),
        "0" | "false" | "no" => Ok(false),
        _ => Err(Refusal::Option),
    }
}

fn options(query: &str) -> Result<CaywRequest, Refusal> {
    let mut format = None;
    let mut command = None;
    let mut brackets = false;
    let mut clipboard = false;
    let mut probe = false;
    let mut seen: Vec<String> = vec![];
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let (name, value) = (decode(name)?, decode(value)?);
        if seen.contains(&name) {
            return Err(Refusal::Option);
        }
        match name.as_str() {
            "format" => format = Some(value),
            "command" => command = Some(value),
            "brackets" => brackets = truthy(&value)?,
            "clipboard" => clipboard = truthy(&value)?,
            "probe" => probe = truthy(&value)?,
            // Window behaviour with no meaning for a picker that is already a
            // window of its own; docs/parity.md lists them as excluded.
            "minimize" | "texstudio" | "select" | "application" => {}
            // Asking for this and being given something else would be a lie.
            _ => return Err(Refusal::Option),
        }
        seen.push(name);
    }
    if probe {
        return Ok(CaywRequest::Probe);
    }
    let (output, form) = output(format.as_deref().unwrap_or("pandoc"), command.as_deref(), brackets)?;
    Ok(CaywRequest::Pick(CaywPick { output, form, clipboard }))
}

fn key(syntax: KeySyntax, command: Option<&str>) -> CaywOutput {
    CaywOutput::Key(OutputSpec::Key {
        syntax,
        command: command.map(str::to_owned),
    })
}

/// One row per format in docs/parity.md. A command the syntax cannot write is
/// refused rather than swapped for one it can.
fn output(
    format: &str,
    command: Option<&str>,
    brackets: bool,
) -> Result<(CaywOutput, CitationForm), Refusal> {
    use CitationForm::{BibliographyOnly, Narrative, Normal};
    let no_command = |result: (CaywOutput, CitationForm)| match command {
        None | Some("") => Ok(result),
        Some(_) => Err(Refusal::Format),
    };
    match format {
        "pandoc" => no_command((key(KeySyntax::Pandoc, (!brackets).then_some("bare")), Normal)),
        "biblatex" => match command {
            None | Some("" | "autocite") => Ok((key(KeySyntax::Biblatex, Some("autocite")), Normal)),
            Some(command @ ("parencite" | "textcite")) => {
                Ok((key(KeySyntax::Biblatex, Some(command)), Normal))
            }
            Some("cite") => Ok((key(KeySyntax::Latex, Some("cite")), Normal)),
            Some(_) => Err(Refusal::Format),
        },
        "natbib" => match command {
            None | Some("" | "cite") => Ok((key(KeySyntax::Latex, Some("cite")), Normal)),
            Some("citep") => Ok((key(KeySyntax::Natbib, None), Normal)),
            Some("citet") => Ok((key(KeySyntax::Natbib, None), Narrative)),
            Some(_) => Err(Refusal::Format),
        },
        "latex" | "cite" => no_command((key(KeySyntax::Latex, Some("cite")), Normal)),
        "citep" => no_command((key(KeySyntax::Natbib, None), Normal)),
        "citet" => no_command((key(KeySyntax::Natbib, None), Narrative)),
        "typst" => no_command((key(KeySyntax::Typst, None), Normal)),
        "mmd" => no_command((key(KeySyntax::MultiMarkdown, None), Normal)),
        "asciidoctor-bibtex" => no_command((key(KeySyntax::AsciiDoc, None), Normal)),
        "jupyter" => no_command((key(KeySyntax::Jupyter, None), Normal)),
        "json" => no_command((key(KeySyntax::CaywJson, None), Normal)),
        "formatted-citation" => no_command((CaywOutput::Style, Normal)),
        "formatted-bibliography" => no_command((CaywOutput::Style, BibliographyOnly)),
        _ => Err(Refusal::Format),
    }
}

/// One whole response. It carries no CORS header, so a page that reaches this
/// far still cannot read it, and it is never cached or sniffed as a script.
pub fn response(status: (u16, &str), body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n{body}",
        status.0,
        status.1,
        body.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: u16 = 23_124;
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn head(target: &str, headers: &[&str]) -> Vec<u8> {
        let mut text = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\n");
        for header in headers {
            text.push_str(header);
            text.push_str("\r\n");
        }
        text.push_str("\r\n");
        text.into_bytes()
    }
    fn cayw(query: &str) -> String {
        format!("/{TOKEN}/better-bibtex/cayw{query}")
    }
    fn request(query: &str) -> Result<CaywRequest, Refusal> {
        parse(&head(&cayw(query), &[]), PORT, TOKEN)
    }
    fn pick(query: &str) -> CaywPick {
        match request(query) {
            Ok(CaywRequest::Pick(pick)) => pick,
            other => panic!("{query}: {other:?}"),
        }
    }
    fn spec(query: &str) -> (Option<KeySyntax>, Option<String>, CitationForm) {
        let pick = pick(query);
        match pick.spec("apa") {
            OutputSpec::Key { syntax, command } => (Some(syntax), command, pick.form),
            _ => (None, None, pick.form),
        }
    }

    #[test]
    fn every_listed_format_has_its_output() {
        use CitationForm::{BibliographyOnly, Narrative, Normal};
        use KeySyntax::*;
        let owned = |command: &str| Some(command.to_owned());
        for (query, syntax, command, form) in [
            ("", Some(Pandoc), owned("bare"), Normal),
            ("?format=pandoc", Some(Pandoc), owned("bare"), Normal),
            ("?format=pandoc&brackets=1", Some(Pandoc), None, Normal),
            ("?format=pandoc&brackets=true", Some(Pandoc), None, Normal),
            ("?format=biblatex", Some(Biblatex), owned("autocite"), Normal),
            (
                "?format=biblatex&command=parencite",
                Some(Biblatex),
                owned("parencite"),
                Normal,
            ),
            (
                "?format=biblatex&command=textcite",
                Some(Biblatex),
                owned("textcite"),
                Normal,
            ),
            ("?format=natbib", Some(Latex), owned("cite"), Normal),
            ("?format=natbib&command=citep", Some(Natbib), None, Normal),
            ("?format=natbib&command=citet", Some(Natbib), None, Narrative),
            ("?format=latex", Some(Latex), owned("cite"), Normal),
            ("?format=cite", Some(Latex), owned("cite"), Normal),
            ("?format=citep", Some(Natbib), None, Normal),
            ("?format=citet", Some(Natbib), None, Narrative),
            ("?format=typst", Some(Typst), None, Normal),
            ("?format=mmd", Some(MultiMarkdown), None, Normal),
            ("?format=asciidoctor-bibtex", Some(AsciiDoc), None, Normal),
            ("?format=jupyter", Some(Jupyter), None, Normal),
            ("?format=json", Some(CaywJson), None, Normal),
            ("?format=formatted-citation", None, None, Normal),
            ("?format=formatted-bibliography", None, None, BibliographyOnly),
        ] {
            assert_eq!(spec(query), (syntax, command, form), "{query}");
        }
        assert_eq!(
            pick("?format=formatted-citation").spec("ieee"),
            OutputSpec::Csl { style: "ieee".into() }
        );
    }

    #[test]
    fn a_probe_opens_nothing() {
        assert_eq!(request("?probe=true"), Ok(CaywRequest::Probe));
        assert_eq!(request("?probe=1&format=pandoc"), Ok(CaywRequest::Probe));
        assert!(matches!(request("?probe=false"), Ok(CaywRequest::Pick(_))));
    }

    #[test]
    fn options_that_change_nothing_are_allowed_and_clipboard_is_kept() {
        assert!(pick("?format=pandoc&clipboard=1").clipboard);
        assert!(!pick("?format=pandoc").clipboard);
        assert!(!pick("?format=pandoc&clipboard=false").clipboard);
        for option in ["minimize=true", "texstudio=true", "select=true", "application=1"] {
            pick(&format!("?format=pandoc&{option}"));
        }
    }

    #[test]
    fn a_format_or_option_ibis_cannot_honour_is_refused_not_replaced() {
        for query in [
            "?format=eta",
            "?format=translate",
            "?format=playground",
            "?format=scannable-cite",
            "?format=biblatex&command=footcite",
            "?format=biblatex&command=fullcite%7Binjected%7D",
            "?format=natbib&command=citealt",
            "?format=typst&command=cite",
        ] {
            assert_eq!(request(query), Err(Refusal::Format), "{query}");
        }
        for query in [
            "?selected=true",
            "?page=p.",
            "?brackets=maybe",
            "?format=pandoc&format=typst",
            "?clipboard=2",
        ] {
            assert_eq!(request(query), Err(Refusal::Option), "{query}");
        }
    }

    #[test]
    fn a_request_without_the_token_is_refused_whatever_it_asks() {
        for target in [
            "/better-bibtex/cayw?format=pandoc".to_owned(),
            "/better-bibtex/cayw?probe=true".to_owned(),
            format!("/{}/better-bibtex/cayw", &TOKEN[..31]),
            format!("/{TOKEN}0/better-bibtex/cayw"),
            "/0123456789abcdef0123456789abcdeE/better-bibtex/cayw".to_owned(),
            "/".to_owned(),
            "/better-bibtex/json-rpc".to_owned(),
        ] {
            assert_eq!(parse(&head(&target, &[]), PORT, TOKEN), Err(Refusal::Token), "{target}");
        }
        assert_eq!(
            parse(&head(&format!("/{TOKEN}/better-bibtex/json-rpc"), &[]), PORT, TOKEN),
            Err(Refusal::NotFound)
        );
    }

    #[test]
    fn anything_a_browser_adds_is_refused_even_with_the_token() {
        for header in [
            "Origin: https://invented.example",
            "origin: null",
            "Referer: https://invented.example/page",
            "Cookie: a=b",
            "Sec-Fetch-Site: cross-site",
            "Sec-Fetch-Mode: no-cors",
            "sec-fetch-dest: image",
            "Sec-CH-UA: invented",
            "Access-Control-Request-Method: GET",
        ] {
            assert_eq!(
                parse(&head(&cayw("?format=pandoc"), &[header]), PORT, TOKEN),
                Err(Refusal::Browser),
                "{header}"
            );
        }
    }

    #[test]
    fn only_this_computers_own_names_may_address_it() {
        let with_host = |host: &str| format!("GET {} HTTP/1.1\r\nHost: {host}\r\n\r\n", cayw("")).into_bytes();
        for host in [
            "invented.example".to_owned(),
            format!("invented.example:{PORT}"),
            format!("127.0.0.1.invented.example:{PORT}"),
            "127.0.0.1".to_owned(),
            "127.0.0.1:1".to_owned(),
            format!("[::1]:{PORT}"),
            format!("127.0.0.1:{PORT}.invented.example"),
        ] {
            assert_eq!(parse(&with_host(&host), PORT, TOKEN), Err(Refusal::Browser), "{host}");
        }
        for host in [
            format!("127.0.0.1:{PORT}"),
            format!("localhost:{PORT}"),
            format!("LOCALHOST:{PORT}"),
        ] {
            assert!(parse(&with_host(&host), PORT, TOKEN).is_ok(), "{host}");
        }
        // Bytes that would split a character if sliced, and numbers that only parse as the port.
        for host in [
            format!("12345678é:{PORT}"),
            format!("localhosé:{PORT}"),
            "é".repeat(40),
            format!("127.0.0.1:+{PORT}"),
            format!("127.0.0.1:0{PORT}"),
            format!("127.0.0.1: {PORT}"),
        ] {
            assert_eq!(parse(&with_host(&host), PORT, TOKEN), Err(Refusal::Browser), "{host}");
        }
        let no_host = format!("GET {} HTTP/1.1\r\n\r\n", cayw("")).into_bytes();
        assert_eq!(parse(&no_host, PORT, TOKEN), Err(Refusal::Browser));
        let two = format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\nHost: invented.example\r\n\r\n",
            cayw("")
        );
        assert_eq!(parse(two.as_bytes(), PORT, TOKEN), Err(Refusal::Malformed));
    }

    #[test]
    fn only_get_is_answered_and_a_preflight_learns_nothing() {
        let with_method = |method: &str, extra: &str| {
            format!(
                "{method} {} HTTP/1.1\r\nHost: 127.0.0.1:{PORT}\r\n{extra}\r\n",
                cayw("")
            )
            .into_bytes()
        };
        for method in ["POST", "PUT", "DELETE", "HEAD", "OPTIONS"] {
            assert_eq!(
                parse(&with_method(method, ""), PORT, TOKEN),
                Err(Refusal::Method),
                "{method}"
            );
        }
        assert_eq!(
            parse(
                &with_method("OPTIONS", "Origin: https://invented.example\r\n"),
                PORT,
                TOKEN
            ),
            Err(Refusal::Browser)
        );
    }

    #[test]
    fn a_body_or_a_second_framing_is_refused() {
        for header in ["Content-Length: 5", "Transfer-Encoding: chunked", "Upgrade: websocket"] {
            assert_eq!(
                parse(&head(&cayw(""), &[header]), PORT, TOKEN),
                Err(Refusal::Malformed),
                "{header}"
            );
        }
        assert!(parse(&head(&cayw(""), &["Content-Length: 0"]), PORT, TOKEN).is_ok());
    }

    #[test]
    fn broken_or_oversized_requests_are_refused() {
        for bytes in [
            &b"GET\r\n\r\n"[..],
            b"GET /a b HTTP/1.1\r\n\r\n",
            b"GET / HTTP/2\r\n\r\n",
            b"\xff\xfe\r\n\r\n",
            b"GET / HTTP/1.1\r\nno colon\r\n\r\n",
            b"GET / HTTP/1.1\r\nBad Name: x\r\n\r\n",
            b"GET / HTTP/1.1\r\nHost: 127.0.0.1:1\r\n",
        ] {
            assert_eq!(parse(bytes, PORT, TOKEN), Err(Refusal::Malformed), "{bytes:?}");
        }
        assert_eq!(
            parse(&vec![b'a'; MAX_HEAD_BYTES + 1], PORT, TOKEN),
            Err(Refusal::TooLarge)
        );
        let many: Vec<_> = (0..=MAX_HEADERS).map(|index| format!("X-{index}: 1")).collect();
        let many: Vec<_> = many.iter().map(String::as_str).collect();
        assert_eq!(parse(&head(&cayw(""), &many), PORT, TOKEN), Err(Refusal::TooLarge));
        for query in ["?format=%", "?format=%zz", "?format=%ff", "?format=%0a"] {
            assert_eq!(request(query), Err(Refusal::Malformed), "{query}");
        }
        assert_eq!(pick("?format=%70andoc"), pick("?format=pandoc"));
    }

    #[test]
    fn a_response_repeats_nothing_and_grants_nothing() {
        for refusal in [
            Refusal::Malformed,
            Refusal::Browser,
            Refusal::Token,
            Refusal::NotFound,
            Refusal::Method,
            Refusal::TooLarge,
            Refusal::Format,
            Refusal::Option,
        ] {
            let text = String::from_utf8(response(refusal.status(), refusal.message())).unwrap_or_default();
            assert!(!text.to_ascii_lowercase().contains("access-control"), "{text}");
            assert!(text.contains("X-Content-Type-Options: nosniff") && text.contains("Cache-Control: no-store"));
        }
        let text = String::from_utf8(response((200, "OK"), "[@é]")).unwrap_or_default();
        assert!(text.contains("Content-Length: 5\r\n"), "bytes, not characters: {text}");
    }

    /// Any bytes at all end in a request or a refusal, never a panic. A fixed
    /// generator keeps every run the same, with no new dependency.
    #[test]
    fn no_bytes_make_the_parser_panic() {
        let mut state: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let pieces: [&[u8]; 12] = [
            b"GET ",
            b" HTTP/1.1",
            b"\r\n",
            b"Host: ",
            b"127.0.0.1:",
            b"23124",
            b"%",
            b"?format=",
            "é".as_bytes(),
            b"\xff",
            b":",
            TOKEN.as_bytes(),
        ];
        for _ in 0..20_000 {
            let mut bytes = vec![];
            for _ in 0..(next() % 24) {
                match next() % 3 {
                    0 => bytes.push(next() as u8),
                    _ => bytes.extend_from_slice(pieces[(next() % pieces.len() as u64) as usize]),
                }
            }
            if next() % 2 == 0 {
                bytes.extend_from_slice(b"\r\n\r\n");
            }
            let _ = parse(&bytes, PORT, TOKEN);
        }
    }

    #[test]
    fn a_head_ends_at_the_first_blank_line() {
        assert_eq!(head_end(b"GET / HTTP/1.1\r\n\r\nmore"), Some(18));
        assert_eq!(head_end(b"GET / HTTP/1.1\r\n"), None);
    }
}
