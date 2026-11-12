//! Native streams over the shared CLI parser and engine dispatcher.
use std::io::IsTerminal;
fn main() -> std::process::ExitCode {
    let output = std::io::stdout();
    let mode = ibis::OutputMode::Streams {
        stdout: output.is_terminal(),
        stderr: std::io::stderr().is_terminal(),
        stdin: std::io::stdin().is_terminal(),
    };
    std::process::ExitCode::from(ibis::run(
        std::env::args_os().collect(),
        std::io::stdin().lock(),
        output,
        std::io::stderr(),
        mode,
    ))
}
