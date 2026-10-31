//! In-process, opt-in paste evidence. No API here accepts a native object or user string.

use std::cell::RefCell;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Snapshot {
    stage: &'static str,
    ax_error: Option<i32>,
    ax_duration_us: Option<u64>,
    clock_sample_us: Option<u64>,
    owner_match: Option<bool>,
    document_equal: Option<bool>,
    surface_equal: Option<bool>,
    focus_equal: Option<bool>,
    manual_accessibility_ran: bool,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            stage: "capture",
            ax_error: None,
            ax_duration_us: None,
            clock_sample_us: None,
            owner_match: None,
            document_equal: None,
            surface_equal: None,
            focus_equal: None,
            manual_accessibility_ran: false,
        }
    }
}

impl Snapshot {
    pub(in crate::paste) fn inherit_activation(&mut self, capture: Option<&Self>) {
        self.manual_accessibility_ran |= capture.is_some_and(|capture| capture.manual_accessibility_ran);
    }
}

#[derive(Clone, Default)]
pub(super) struct Trace(Option<Arc<Mutex<Option<Snapshot>>>>);

thread_local! {
    static ACTIVE: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
    static CAPTURE: RefCell<Option<Snapshot>> = const { RefCell::new(None) };
}

impl Trace {
    #[cfg(test)]
    pub(super) fn enabled() -> Self {
        Self(Some(Arc::new(Mutex::new(Some(Snapshot::default())))))
    }

    pub(super) fn new(capture: Option<Snapshot>) -> Self {
        if std::env::var_os("IBIS_MAC_PASTE_DIAGNOSTICS").is_some_and(|value| value == "1") {
            Self(Some(Arc::new(Mutex::new(Some(capture.unwrap_or_default())))))
        } else {
            Self::default()
        }
    }

    // Nested/reentrant main-thread work restores the previous attempt, and queued
    // workers share only this attempt's trace. No borrow spans provider IPC.
    pub(super) fn with<T>(&self, work: impl FnOnce() -> T) -> T {
        struct Restore {
            trace: Trace,
            previous: Option<Snapshot>,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                let updated = ACTIVE.with(|active| active.replace(self.previous.take()));
                if let Some(trace) = &self.trace.0
                    && let Ok(mut record) = trace.lock()
                    && record.is_some()
                {
                    *record = updated;
                }
            }
        }
        let previous = ACTIVE.with(|active| active.replace(self.snapshot()));
        let _restore = Restore {
            trace: self.clone(),
            previous,
        };
        work()
    }

    pub(super) fn snapshot(&self) -> Option<Snapshot> {
        self.0.as_ref()?.lock().ok()?.clone()
    }

    pub(super) fn remember_capture(&self) {
        CAPTURE.with(|capture| capture.replace(self.snapshot()));
    }

    pub(super) fn stage(&self, stage: &'static str) {
        self.with(|| update(|record| record.stage = stage));
    }

    // Taking the record makes completion one-use; late cancelled tasks cannot
    // alter or print a second line. This runs after posting/refusal, never in its gap.
    fn finish_line(&self, duration: Duration) -> Option<String> {
        #[derive(Serialize)]
        struct Line {
            #[serde(flatten)]
            evidence: Snapshot,
            duration_us: u64,
        }
        let evidence = self.0.as_ref()?.lock().ok()?.take()?;
        serde_json::to_string(&Line {
            evidence,
            duration_us: micros(duration),
        })
        .ok()
    }

    pub(super) fn finish(&self, duration: Duration) {
        if let Some(line) = self.finish_line(duration) {
            eprintln!("{line}");
        }
    }
}

pub(in crate::paste) fn capture_snapshot() -> Option<Snapshot> {
    CAPTURE.with(|capture| capture.borrow().clone())
}

fn update(change: impl FnOnce(&mut Snapshot)) {
    ACTIVE.with(|active| {
        if let Some(record) = active.borrow_mut().as_mut() {
            change(record);
        }
    });
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().try_into().unwrap_or(u64::MAX)
}

pub(super) fn stage(stage: &'static str) {
    update(|record| record.stage = stage);
}

pub(super) fn ax(error: i32, duration: Duration) {
    update(|record| {
        // Retain the first failure, including a capture failure followed by
        // successful repair. Otherwise report the latest successful call.
        if record.ax_error.is_none_or(|previous| previous == 0) {
            record.ax_error = Some(error);
            record.ax_duration_us = Some(micros(duration));
        }
    });
}

pub(super) fn clock(duration: Duration) {
    update(|record| record.clock_sample_us = Some(micros(duration)));
}

pub(super) fn owner(matches: bool) {
    update(|record| record.owner_match = Some(matches));
}

/// Records that the capture actually attempted AXManualAccessibility, even if
/// the setter failed. Eligibility, missing attributes and a process claim alone
/// do not count as a write attempt.
pub(super) fn manual_accessibility() {
    update(|record| record.manual_accessibility_ran = true);
}

pub(super) fn equality(document: Option<bool>, surface: Option<bool>, focus: Option<bool>) {
    update(|record| {
        if document.is_some() {
            record.document_equal = document;
        }
        if surface.is_some() {
            record.surface_equal = surface;
        }
        if focus.is_some() {
            record.focus_equal = focus;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> Trace {
        Trace::enabled()
    }

    #[test]
    fn diagnostics_are_one_line_one_use_and_contain_only_allowlisted_evidence() {
        let trace = enabled();
        trace.with(|| {
            stage("ax.focused_window");
            ax(-25212, Duration::from_micros(3000));
            owner(true);
            equality(Some(false), None, None);
        });
        let line = trace
            .finish_line(Duration::from_micros(3500))
            .unwrap_or_else(|| panic!("line"));
        assert!(!line.contains('\n'));
        let value: serde_json::Value = serde_json::from_str(&line).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            value,
            serde_json::json!({
                "stage": "ax.focused_window", "ax_error": -25212, "ax_duration_us": 3000,
                "duration_us": 3500, "clock_sample_us": null, "owner_match": true,
                "document_equal": false, "surface_equal": null, "focus_equal": null,
                "manual_accessibility_ran": false
            })
        );
        trace.with(|| stage("late.cancelled"));
        assert!(trace.finish_line(Duration::ZERO).is_none());
        assert!(Trace::default().finish_line(Duration::ZERO).is_none());
    }

    #[test]
    fn a_reentrant_or_newer_attempt_cannot_consume_an_older_attempts_diagnostic() {
        let a = enabled();
        let b = enabled();
        a.with(|| {
            stage("readiness");
            b.with(|| {
                stage("capture");
                ax(-25205, Duration::ZERO);
            });
            stage("posted");
        });
        assert_eq!(a.snapshot().map(|s| (s.stage, s.ax_error)), Some(("posted", None)));
        assert_eq!(
            b.snapshot().map(|s| (s.stage, s.ax_error)),
            Some(("capture", Some(-25205)))
        );
    }

    #[test]
    fn failed_pre_write_clock_evidence_is_frozen_in_its_own_copy_proof() {
        use crate::paste::{LastTarget, SafetyStamp, capture::Captured};
        use ibis_core::destination::System;
        let trace = enabled();
        trace.with(|| {
            clock(Duration::from_micros(700));
            stage("lifecycle.clock_sample");
        });
        let last = LastTarget::default();
        let capture = enabled();
        capture.with(manual_accessibility);
        let mut captured = Captured::unknown(System::Macos);
        captured.diagnostic = capture.snapshot();
        last.begin_presentation(Some(captured));
        let token = last
            .write_copy_observed(
                "[@invented]",
                SafetyStamp {
                    epoch: 0,
                    elapsed: Duration::ZERO,
                },
                |_| Ok(None),
                || Ok(Some(11)),
                trace.snapshot(),
            )
            .unwrap_or_else(|e| panic!("{e}"));
        let proof = last.take_copy("[@invented]", token).unwrap_or_else(|| panic!("proof"));
        // A subsequent successful sample/attempt cannot replace this failure.
        trace.with(|| {
            clock(Duration::from_micros(1));
            stage("readiness");
        });
        let saved = proof
            .captured
            .and_then(|captured| captured.diagnostic)
            .unwrap_or_else(|| panic!("evidence"));
        assert_eq!(
            (saved.stage, saved.clock_sample_us),
            ("lifecycle.clock_sample", Some(700))
        );
        assert!(saved.manual_accessibility_ran);
    }
}
