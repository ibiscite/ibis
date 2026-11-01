//! Native geometry and persistence scheduling. No native calls or I/O under this owner.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ibis_core::settings::PickerWindow;

pub(super) const SAVE_DELAY: Duration = Duration::from_millis(500);
pub(super) const MINIMUM: (u32, u32) = (480, 360);

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Display {
    pub key: String,
    pub scale: f64,
    pub origin: (f64, f64),
    pub room: (f64, f64),
}

#[derive(Clone, Debug)]
pub(super) struct Sample {
    pub display: Display,
    pub frame: PickerWindow,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Event {
    Geometry,
    Dpi { scale: f64, logical: (u32, u32) },
}

#[derive(Clone, Debug)]
pub(super) struct Save {
    pub revision: u64,
    pub windows: BTreeMap<String, PickerWindow>,
    pub due: Instant,
}

#[derive(Clone, Debug)]
pub(super) struct Undo {
    pub revision: u64,
    pub previous: Sample,
    display: String,
    saved: Option<PickerWindow>,
}

#[derive(Default)]
pub(super) struct Owner {
    pub windows: BTreeMap<String, PickerWindow>,
    pub revision: u64,
    pub last: Option<Sample>,
    pub constraints: Option<Display>,
    pub applying: bool,
    pub pending_event: bool,
    pending_read: bool,
    read_queued: bool,
    dpi: Option<Event>,
    automatic: Option<(u32, u32)>,
    pub pending_save: Option<Save>,
    pub in_flight: Option<u64>,
    pub completed: u64,
    pub stopping: bool,
    pub finished: bool,
    pub started: bool,
    pub undo: Option<Undo>,
}

/// Keep the native read outside the owner's lock, including reentrant events.
pub(super) fn settled(state: &Mutex<Owner>, read: impl FnOnce()) {
    let pending = state.lock().is_ok_and(|mut state| state.take_read());
    if pending {
        read();
    }
}

pub(super) fn display_changed(state: &Mutex<Owner>, now: Instant, enqueue: impl FnOnce()) {
    let queue = state.lock().is_ok_and(|mut state| {
        state.event(Event::Geometry, now);
        if !state.pending_read || state.read_queued {
            return false;
        }
        state.read_queued = true;
        true
    });
    if queue {
        enqueue();
    }
}

pub(super) fn display_settled(state: &Mutex<Owner>, read: impl FnOnce()) {
    let pending = state.lock().is_ok_and(|mut state| {
        state.read_queued = false;
        state.take_read()
    });
    if pending {
        read();
    }
}

pub(super) fn display_wake_failed(state: &Mutex<Owner>) {
    if let Ok(mut state) = state.lock() {
        state.read_queued = false;
    }
}

pub(super) fn content_room(inner: (f64, f64), outer: (f64, f64), work: (f64, f64)) -> Option<(f64, f64)> {
    if [inner.0, inner.1, outer.0, outer.1, work.0, work.1]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return None;
    }
    let chrome = (outer.0 - inner.0, outer.1 - inner.1);
    if chrome.0 < 0.0 || chrome.1 < 0.0 || chrome.0 >= work.0 || chrome.1 >= work.1 {
        return None;
    }
    Some((work.0 - chrome.0, work.1 - chrome.1))
}

fn constrained(size: (u32, u32), room: (f64, f64)) -> (u32, u32) {
    (
        size.0.max(MINIMUM.0).min(room.0.floor().max(1.0) as u32),
        size.1.max(MINIMUM.1).min(room.1.floor().max(1.0) as u32),
    )
}

impl Owner {
    pub fn prepared(&mut self, sample: Sample) {
        self.constraints = Some(sample.display.clone());
        self.last = Some(sample);
        self.pending_event = false;
        self.pending_read = false;
        // Do not release read_queued here or in flush(): its callback still
        // exists. It can serve a later real event, or drain without a read.
        self.dpi = None;
        self.automatic = None;
    }

    pub fn event(&mut self, event: Event, now: Instant) {
        if self.applying || self.stopping || self.last.is_none() {
            return;
        }
        self.pending_event = true;
        self.pending_read = true;
        // A suggestion for the scale already accepted by this owner is stale.
        // It cannot classify a later edge drag, even if their sizes coincide.
        if matches!(event, Event::Dpi { scale, .. } if self.last.as_ref().is_some_and(|last| last.display.scale != scale))
        {
            self.dpi = Some(event);
        }
        if let Some(save) = &mut self.pending_save {
            save.due = now + SAVE_DELAY;
        }
    }

    /// Consume before any handle lookup: even cloning a Tauri window wakes
    /// Tao's macOS loop. Hidden windows, failed I/O and rejected samples must
    /// not retry on the resulting cleared event. A real later event can rearm.
    pub fn take_read(&mut self) -> bool {
        std::mem::take(&mut self.pending_read)
    }

    pub fn observe(&mut self, sample: Sample, now: Instant, settled: bool) -> Option<Display> {
        if self.applying || self.stopping {
            return None;
        }
        let size = (sample.frame.width, sample.frame.height);
        let dpi = self
            .dpi
            .filter(|event| matches!(event, Event::Dpi { scale, .. } if *scale == sample.display.scale));
        if dpi.is_some()
            && (!settled
                || self.last.as_ref().is_some_and(|last| {
                    // The new scale with the old physical client area is not a drag.
                    // Even a closing flush must not accept this intermediate frame.
                    !dpi.is_some_and(|event| matches!(event, Event::Dpi { logical, .. } if logical == size))
                        && last.display.scale != sample.display.scale
                        && (f64::from(last.frame.width) * last.display.scale - f64::from(size.0) * sample.display.scale)
                            .abs()
                            <= 1.0
                        && (f64::from(last.frame.height) * last.display.scale
                            - f64::from(size.1) * sample.display.scale)
                            .abs()
                            <= 1.0
                }))
        {
            return None;
        }
        let constraints = (self.constraints.as_ref() != Some(&sample.display)).then(|| sample.display.clone());
        if !self.pending_event && constraints.is_none() {
            return None;
        }
        let automatic = self.automatic == Some(size)
            || dpi.is_some_and(|event| matches!(event, Event::Dpi { logical, .. } if logical.0.abs_diff(size.0) <= 1 && logical.1.abs_diff(size.1) <= 1))
            || constraints.is_some() && self.last.as_ref().is_some_and(|last| constrained((last.frame.width, last.frame.height), sample.display.room) == size);
        let changed = self.last.as_ref().is_none_or(|last| {
            last.display.key != sample.display.key
                || (last.frame.width, last.frame.height, last.frame.x, last.frame.y)
                    != (size.0, size.1, sample.frame.x, sample.frame.y)
        });
        let size_changed = !automatic
            && self
                .last
                .as_ref()
                .is_none_or(|last| (last.frame.width, last.frame.height) != size);
        if changed {
            let mut frame = sample.frame;
            frame.custom_size =
                size_changed || self.windows.get(&sample.display.key).is_some_and(|old| old.custom_size);
            if !size_changed && let Some(old) = self.windows.get(&sample.display.key) {
                frame.width = old.width;
                frame.height = old.height;
            }
            if self.windows.get(&sample.display.key) != Some(&frame) {
                self.windows.insert(sample.display.key.clone(), frame);
                self.queue_save(now);
            }
        }
        if let Some(display) = &constraints {
            let clamped = constrained(size, display.room);
            self.automatic = (clamped != size).then_some(clamped);
        } else if automatic || size_changed {
            self.automatic = None;
        }
        self.last = Some(sample);
        self.pending_event = false;
        self.dpi = None;
        constraints
    }

    /// A size chosen in Settings, kept for this display like a dragged one.
    pub fn choose(&mut self, display: &str, frame: PickerWindow, now: Instant) {
        if self.windows.get(display) != Some(&frame) {
            self.windows.insert(display.to_owned(), frame);
            self.queue_save(now);
        }
    }

    pub fn queue_save(&mut self, now: Instant) {
        self.undo = None;
        self.revision += 1;
        self.pending_save = Some(Save {
            revision: self.revision,
            windows: self.windows.clone(),
            due: now + SAVE_DELAY,
        });
    }

    pub fn forget(&mut self, display: &str, now: Instant) -> Option<Save> {
        if self.stopping {
            return None;
        }
        self.windows.remove(display);
        self.queue_save(now);
        self.pending_save.clone()
    }

    pub fn reset(&mut self, display: &str, previous: Sample, now: Instant) -> Option<u64> {
        let saved = self.windows.get(display).copied();
        let revision = self.forget(display, now)?.revision;
        self.undo = Some(Undo {
            revision,
            previous,
            display: display.into(),
            saved,
        });
        Some(revision)
    }

    pub fn undo_for(&self, revision: u64) -> Option<Undo> {
        self.undo
            .as_ref()
            .filter(|undo| !self.stopping && undo.revision == revision && self.revision == revision)
            .cloned()
    }

    pub fn restore(&mut self, revision: u64, now: Instant) -> bool {
        let Some(undo) = self.undo_for(revision) else {
            return false;
        };
        match undo.saved {
            Some(frame) => {
                self.windows.insert(undo.display, frame);
            }
            None => {
                self.windows.remove(&undo.display);
            }
        }
        self.queue_save(now);
        true
    }

    pub fn dismiss_undo(&mut self, revision: u64) {
        if self.undo.as_ref().is_some_and(|undo| undo.revision == revision) {
            self.undo = None;
        }
    }

    pub fn take_save(&mut self, now: Instant) -> Option<Save> {
        if self.pending_save.as_ref()?.due > now {
            return None;
        }
        let save = self.pending_save.take()?;
        self.in_flight = Some(save.revision);
        Some(save)
    }

    pub fn complete(&mut self, revision: u64) {
        self.completed = self.completed.max(revision);
        if self.in_flight == Some(revision) {
            self.in_flight = None;
        }
    }

    pub fn flush(&mut self, now: Instant, stop: bool) -> u64 {
        if let Some(save) = &mut self.pending_save {
            save.due = now;
        }
        self.pending_event = false;
        self.stopping |= stop;
        self.pending_read = false;
        self.revision
    }

    pub fn drained(&self, revision: u64, stop: bool) -> bool {
        self.completed >= revision && (!stop || self.finished)
    }
}

#[cfg(test)]
#[path = "owner/tests.rs"]
mod tests;
