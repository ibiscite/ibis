//! Stream faults use in-memory writers, no native pipes or user configuration.
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
