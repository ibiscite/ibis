//! Native streams over the shared CLI parser and engine dispatcher.
use std::io::IsTerminal;
fn main() -> std::process::ExitCode {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().collect();
    #[cfg(unix)]
    if arguments
        .get(1)
        .is_some_and(|arg| arg == ibis_sources::adapter::GUARD_ARGUMENT)
    {
        return std::process::ExitCode::from(ibis_sources::adapter::run_guard());
    }
    // The PDF extractor's child process, started only by `ibis` itself; it
    // reads no settings and opens no library (decision 0055).
    if arguments
        .get(1)
        .is_some_and(|argument| argument == ibis_engine::pdf::CHILD_ARGUMENT)
    {
        return std::process::ExitCode::from(ibis::pdf_text::run_child(&arguments[2..]));
    }
    // The document child, started only by `ibis` itself: it parses one
    // document's bytes from stdin and reads no settings (decision 0060).
    if arguments
        .get(1)
        .is_some_and(|argument| argument == ibis_sources::document::CHILD_ARGUMENT)
    {
        return std::process::ExitCode::from(ibis::run_document_child());
    }
    // A built-in source adapter, started only by `ibis` itself (decision 0056).
    if arguments
        .get(1)
        .is_some_and(|argument| argument == ibis_engine::sources::CHILD_ARGUMENT)
    {
        return std::process::ExitCode::from(match arguments.get(2).and_then(|name| name.to_str()) {
            Some(ibis_sources::adapter::ENDNOTE) => {
                ibis_sources::endnote::serve(std::io::stdin().lock(), std::io::stdout().lock())
            }
            _ => 2,
        });
    }
    // The shared child runner kills and reaps its groups before we exit on
    // SIGINT/SIGTERM. Child executables retain their normal signal masks.
    #[cfg(unix)]
    let termination = match ibis_sources::adapter::exit_on_termination() {
        Ok(termination) => termination,
        Err(_) => return std::process::ExitCode::from(70),
    };
    #[cfg(target_os = "macos")]
    if arguments.len() == 2 && arguments[1] == "__picker_endpoint" {
        let status = match ibis::daemon::picker_endpoint() {
            Ok(()) => 0,
            Err(error) if error.kind() == std::io::ErrorKind::Unsupported => 6,
            Err(_) => 5,
        };
        termination.exit(status);
    }
    if arguments.len() == 2 && arguments[1] == "daemon" {
        let status = ibis::run_daemon();
        #[cfg(unix)]
        termination.exit(status);
        #[cfg(not(unix))]
        return std::process::ExitCode::from(status);
    }
    let output = std::io::stdout();
    let mode = ibis::OutputMode::Streams {
        stdout: output.is_terminal(),
        stderr: std::io::stderr().is_terminal(),
        stdin: std::io::stdin().is_terminal(),
    };
    let status = ibis::run(arguments, std::io::stdin().lock(), output, std::io::stderr(), mode);
    #[cfg(unix)]
    termination.exit(status);
    #[cfg(not(unix))]
    std::process::ExitCode::from(status)
}
