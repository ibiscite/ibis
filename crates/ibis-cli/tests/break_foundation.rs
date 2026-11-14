//! Stream faults use in-memory writers, no native pipes or user configuration.
#![allow(clippy::unwrap_used)]
use std::io::{self, Cursor, Write};

struct FullOnFlush;
impl Write for FullOnFlush {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::StorageFull, "invented full destination"))
    }
}

#[test]
fn help_flush_failure_cannot_exit_successfully() {
    let code = ibis::run_with(
        ibis_engine::Engine::new().unwrap(),
        vec!["ibis".into(), "--help".into()],
        Cursor::new(Vec::<u8>::new()),
        FullOnFlush,
        Vec::<u8>::new(),
        ibis::OutputMode::Plain,
    );
    assert_eq!(
        code, 70,
        "help acknowledged output that the destination could not flush"
    );
}

#[test]
fn ordinary_output_reports_the_same_flush_failure() {
    let code = ibis::run_with(
        ibis_engine::Engine::new().unwrap(),
        vec!["ibis".into(), "outputs".into()],
        Cursor::new(Vec::<u8>::new()),
        FullOnFlush,
        Vec::<u8>::new(),
        ibis::OutputMode::Plain,
    );
    assert_eq!(code, 70);
}

#[test]
fn invented_citation_is_independent_of_locale_timezone_and_tty() {
    let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
    let path = dir.path().join("invented.bib");
    std::fs::write(
        &path,
        "@book{invented,title={Invented},author={Example, Ada},year={2026}}",
    )
    .unwrap();
    let mut outputs = Vec::new();
    for (locale, timezone) in [
        ("C", "UTC"),
        ("tr_TR.UTF-8", "Pacific/Kiritimati"),
        ("C.UTF-8", "America/New_York"),
    ] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_ibis"))
            .args(["cite", "invented", "--file"])
            .arg(&path)
            .args(["--style", "apa"])
            .env("LC_ALL", locale)
            .env("TZ", timezone)
            .env("XDG_CONFIG_HOME", dir.path().join("invented-config"))
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "locale={locale}, timezone={timezone}: {:?}",
            output.stderr
        );
        assert!(output.stderr.is_empty());
        outputs.push(output.stdout);
    }
    assert!(outputs.iter().all(|output| output == &outputs[0]));
    assert_eq!(outputs[0], b"(Example, 2026)\n");
}
