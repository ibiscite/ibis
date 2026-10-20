//! The shortcut is ignored while a full-screen app or presentation is in
//! front, unless it is pressed again soon after (docs/design.md, "Never").
//! A presenter who brushes the keys does not cover the audience's slides,
//! and someone who wants Ibis there gets it with a second press.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How soon a second press must follow an ignored one to open the picker
/// anyway: long enough to press again on purpose, short enough that two
/// unrelated slips minutes apart still do nothing.
pub(crate) const PRESS_AGAIN_WITHIN: Duration = Duration::from_secs(2);

/// Remembers the last press that was ignored because of a full-screen app.
#[derive(Default)]
pub(crate) struct FullScreenGuard(Mutex<Option<Instant>>);

impl FullScreenGuard {
    /// Whether a press that would show the picker may do so. `full_screen` is
    /// `None` where the foreground window cannot be read: the shortcut then
    /// always works, because a guess must never make Ibis unreachable.
    pub(crate) fn allows(&self, full_screen: Option<bool>, now: Instant) -> bool {
        let Ok(mut ignored) = self.0.lock() else {
            return true;
        };
        let (allow, next) = decide(full_screen, *ignored, now);
        *ignored = next;
        allow
    }
}

/// The pure decision: whether to show, and the ignored press to remember.
fn decide(full_screen: Option<bool>, ignored: Option<Instant>, now: Instant) -> (bool, Option<Instant>) {
    if full_screen != Some(true) {
        return (true, None);
    }
    let pressed_again = ignored
        .and_then(|earlier| now.checked_duration_since(earlier))
        .is_some_and(|gap| gap <= PRESS_AGAIN_WITHIN);
    if pressed_again {
        (true, None)
    } else {
        (false, Some(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shortcut_works_normally_without_a_full_screen_app_in_front() {
        let now = Instant::now();
        assert_eq!(decide(Some(false), None, now), (true, None));
        assert_eq!(
            decide(Some(false), Some(now), now),
            (true, None),
            "a stale press is forgotten"
        );
    }

    #[test]
    fn an_unreadable_foreground_never_blocks_the_shortcut() {
        assert_eq!(decide(None, None, Instant::now()), (true, None));
    }

    #[test]
    fn a_first_press_over_a_full_screen_app_is_ignored_and_a_second_opens() {
        let guard = FullScreenGuard::default();
        let first = Instant::now();
        assert!(!guard.allows(Some(true), first));
        assert!(guard.allows(Some(true), first + Duration::from_millis(600)));
        // The override is used up: the next lone press is ignored again.
        assert!(!guard.allows(Some(true), first + Duration::from_secs(10)));
    }

    #[test]
    fn a_second_press_after_the_window_starts_over() {
        let first = Instant::now();
        let late = first + PRESS_AGAIN_WITHIN + Duration::from_millis(1);
        assert_eq!(decide(Some(true), Some(first), late), (false, Some(late)));
        let edge = first + PRESS_AGAIN_WITHIN;
        assert_eq!(decide(Some(true), Some(first), edge), (true, None));
    }

    #[test]
    fn a_clock_that_seems_to_run_backwards_does_not_count_as_a_second_press() {
        let now = Instant::now();
        let later = now + Duration::from_secs(1);
        assert_eq!(decide(Some(true), Some(later), now), (false, Some(now)));
    }

    #[test]
    fn leaving_full_screen_clears_an_ignored_press() {
        let guard = FullScreenGuard::default();
        let now = Instant::now();
        assert!(!guard.allows(Some(true), now));
        assert!(guard.allows(Some(false), now + Duration::from_millis(100)));
        assert!(!guard.allows(Some(true), now + Duration::from_millis(200)));
    }
}
