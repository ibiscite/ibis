//! STUB until #1981 lands: replaced by the PDF reader's runner, shared.
use std::path::Path;
use std::time::Duration;

pub enum Input {
    Bytes(Vec<u8>),
}
pub enum Failure {
    TimedOut,
    TooLarge,
    Unavailable,
    Failed,
}
pub fn run(
    _program: &Path,
    _arguments: &[&str],
    _input: Input,
    _deadline: Duration,
    _cancelled: &dyn Fn() -> bool,
    _max_answer: u64,
) -> Result<Vec<u8>, Failure> {
    Err(Failure::Unavailable)
}
