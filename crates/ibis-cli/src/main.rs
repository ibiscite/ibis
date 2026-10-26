//! Native streams over the shared CLI parser and engine dispatcher.
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(ibis::run(
        std::env::args_os().collect(),
        std::io::stdin().lock(),
        std::io::stdout(),
        std::io::stderr(),
    ))
}
