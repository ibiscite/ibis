//! Disposable-build probe only. Nested spans charge exclusive time, avoiding
//! double counting normalization inside parsing and fingerprints inside hydration.
use std::{cell::RefCell, collections::BTreeMap, time::Instant};
thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}
#[derive(Default)]
struct State {
    stack: Vec<(Instant, u64, &'static str)>,
    totals: BTreeMap<&'static str, u64>,
}
pub struct Span;
pub fn span(label: &'static str) -> Span {
    STATE.with_borrow_mut(|state| state.stack.push((Instant::now(), 0, label)));
    Span
}
impl Drop for Span {
    fn drop(&mut self) {
        STATE.with_borrow_mut(|state| {
            if let Some((began, children, label)) = state.stack.pop() {
                let elapsed = began.elapsed().as_nanos() as u64;
                *state.totals.entry(label).or_default() += elapsed.saturating_sub(children);
                if let Some(parent) = state.stack.last_mut() {
                    parent.1 += elapsed;
                }
            }
        });
    }
}
pub fn take() -> BTreeMap<&'static str, u64> {
    STATE.with_borrow_mut(|state| {
        assert!(state.stack.is_empty());
        std::mem::take(&mut state.totals)
    })
}
