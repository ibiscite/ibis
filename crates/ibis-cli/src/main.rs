//! Native streams over the shared CLI parser and engine dispatcher.
use std::io::IsTerminal;
fn main() -> std::process::ExitCode {
    let output = std::io::stdout();
    let mode = if output.is_terminal() {
        ibis::OutputMode::Terminal
    } else {
        ibis::OutputMode::Plain
    };
    std::process::ExitCode::from(ibis::run(
        std::env::args_os().collect(),
        std::io::stdin().lock(),
        output,
        std::io::stderr(),
        mode,
    ))
}
