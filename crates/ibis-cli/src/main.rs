//! Native streams over the shared CLI parser and engine dispatcher.
use std::io::IsTerminal;
fn main() -> std::process::ExitCode {
    let arguments: Vec<std::ffi::OsString> = std::env::args_os().collect();
    // The PDF extractor's child process, started only by `ibis` itself; it
    // reads no settings and opens no library (decision 0055).
    if arguments
        .get(1)
        .is_some_and(|argument| argument == ibis_engine::pdf::CHILD_ARGUMENT)
    {
        return std::process::ExitCode::from(ibis::pdf_text::run_child(&arguments[2..]));
    }
    // A built-in source adapter, started only by `ibis` itself (decision 0056).
    if arguments
        .get(1)
        .is_some_and(|argument| argument == ibis_engine::sources::CHILD_ARGUMENT)
    {
        return std::process::ExitCode::from(match arguments.get(2).and_then(|name| name.to_str()) {
            Some("endnote") => ibis_source_endnote::serve(std::io::stdin().lock(), std::io::stdout().lock()),
            _ => 2,
        });
    }
    let output = std::io::stdout();
    let mode = ibis::OutputMode::Streams {
        stdout: output.is_terminal(),
        stderr: std::io::stderr().is_terminal(),
        stdin: std::io::stdin().is_terminal(),
    };
    std::process::ExitCode::from(ibis::run(
        arguments,
        std::io::stdin().lock(),
        output,
        std::io::stderr(),
        mode,
    ))
}
