use serde::Serialize;

const MAX_BYTES: u64 = (1_u64 << 53) - 1;

#[derive(Clone, Serialize)]
pub(super) struct Snapshot {
    pub attempt: u32,
    pub revision: u32,
    pub received: u64,
    pub total: Option<u64>,
    pub phase: &'static str,
}

#[derive(Default)]
pub(super) struct Progress(pub Option<Snapshot>);

impl Progress {
    pub fn begin(&mut self) -> Option<u32> {
        let Some(attempt) = self.0.as_ref().map_or(Some(1), |s| s.attempt.checked_add(1)) else {
            if let Some(s) = self.0.as_mut() {
                s.phase = "failed";
            }
            return None;
        };
        self.0 = Some(Snapshot {
            attempt,
            revision: 0,
            received: 0,
            total: None,
            phase: "downloading",
        });
        Some(attempt)
    }

    pub fn change(&mut self, attempt: u32, chunk: Option<(usize, Option<u64>)>, phase: &'static str) {
        let Some(s) = self.0.as_mut().filter(|s| s.attempt == attempt && s.phase != "failed") else {
            return;
        };
        if let Some((bytes, total)) = chunk {
            if s.phase != "downloading" {
                return;
            }
            let received = s.received.saturating_add(bytes as u64);
            let consistent = s.received == 0 || s.total == total;
            s.received = received.min(MAX_BYTES);
            s.total = total.filter(|&n| consistent && n > 0 && n <= MAX_BYTES && received <= n);
        }
        s.phase = phase;
    }

    pub fn snapshot(&mut self) -> Option<Snapshot> {
        let s = self.0.as_mut()?;
        // Serial-number arithmetic on the receiver handles rollover. At one
        // publication/second, the half-range exceeds 68 years of queued events.
        s.revision = s.revision.wrapping_add(1);
        Some(s.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_deltas_with_honest_totals_and_terminal_failure() {
        let mut p = Progress::default();
        let a = p.begin().unwrap_or_else(|| panic!("first attempt"));
        p.change(a, Some((25, Some(100))), "downloading");
        p.change(a, Some((30, Some(100))), "downloading");
        assert_eq!(p.0.as_ref().map(|s| (s.received, s.total)), Some((55, Some(100))));
        for total in [Some(200), None, Some(0), Some(1), Some(u64::MAX)] {
            p.change(a, Some((1, total)), "downloading");
            assert_eq!(p.0.as_ref().and_then(|s| s.total), None);
        }
        p.change(a, None, "installing");
        p.change(a, Some((5, Some(100))), "downloading");
        assert_eq!(p.0.as_ref().map(|s| s.phase), Some("installing"));
        p.change(a, None, "failed");
        p.change(a, None, "installing");
        assert_eq!(p.0.as_ref().map(|s| s.phase), Some("failed"));
        let b = p.begin().unwrap_or_else(|| panic!("retry"));
        p.change(a, None, "failed");
        assert_eq!(p.0.as_ref().map(|s| (s.attempt, s.phase)), Some((b, "downloading")));
    }

    #[test]
    fn counters_never_serialize_imprecise_bytes_or_reuse_an_attempt() {
        let mut p = Progress::default();
        let a = p.begin().unwrap_or_else(|| panic!("attempt"));
        let s = p.0.as_mut().unwrap_or_else(|| panic!("snapshot"));
        s.received = MAX_BYTES;
        s.revision = u32::MAX;
        p.change(a, Some((usize::MAX, Some(u64::MAX))), "downloading");
        let s = p.snapshot().unwrap_or_else(|| panic!("snapshot"));
        assert_eq!((s.received, s.total, s.revision), (MAX_BYTES, None, 0));
        p.0.as_mut().unwrap_or_else(|| panic!("snapshot")).attempt = u32::MAX;
        assert_eq!(p.begin(), None);
        assert_eq!(p.0.as_ref().map(|s| s.phase), Some("failed"));
    }
}
