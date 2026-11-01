//! One session owner claims feedback before the existing settings writer flushes it.
use std::collections::BTreeSet;

use ibis_core::settings::Delights;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::preferences::SettingsFile;

/// Local calendar facts from the webview clock; eligibility is decided here.
#[derive(Clone, Copy, Deserialize, specta::Type)]
pub(crate) struct LocalMoment {
    year: i32,
    month: u8,
    day: u8,
}

impl LocalMoment {
    fn date(self) -> [i32; 3] {
        [self.year, self.month.into(), self.day.into()]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DelightStatus {
    PasteTip,
    Library100,
    Library1000,
    Library10000,
}

// Set this once the first stable release date is known; no invented release date.
const FIRST_STABLE_DATE: Option<[i32; 3]> = None;

fn release_birthday(now: LocalMoment, first_stable: Option<[i32; 3]>) -> bool {
    first_stable.is_some_and(|date| {
        date[0]
            .checked_add(2)
            .is_some_and(|year| now.date() == [year, date[1], date[2]])
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DelightFact {
    Thoth,
    Touch,
    Scarlet,
    Mummies,
    National,
    Flight,
}
const FACTS: [DelightFact; 6] = [
    DelightFact::Thoth,
    DelightFact::Touch,
    DelightFact::Scarlet,
    DelightFact::Mummies,
    DelightFact::National,
    DelightFact::Flight,
];

#[derive(Default, Serialize, specta::Type)]
pub(crate) struct DelightView {
    status: Option<DelightStatus>,
    peck: bool,
    anniversary: bool,
    build: String,
    fun: bool,
    fact: Option<DelightFact>,
    birthday: bool,
    library: Option<String>,
    revision: u64,
}

#[derive(Default)]
pub(crate) struct DelightOwner {
    record: Option<Delights>,
    deliveries: BTreeSet<u32>,
    revision: u64,
    flushed: u64,
}

impl DelightOwner {
    pub(crate) fn new(record: Option<Delights>) -> Self {
        Self {
            record,
            ..Self::default()
        }
    }
    fn record(&mut self) -> &mut Delights {
        self.record.get_or_insert_with(Delights::default)
    }
}

impl SettingsFile {
    #[cfg(test)]
    pub(crate) fn claim_delights(
        &self,
        now: LocalMoment,
        picker: bool,
        library: Option<(String, usize)>,
    ) -> DelightView {
        self.claim_delights_when(now, picker, library, || true, false)
    }

    pub(crate) fn claim_about(&self, now: LocalMoment, click: bool) -> DelightView {
        self.claim_delights_when(now, false, None, || true, click)
    }

    fn claim_delights_when(
        &self,
        now: LocalMoment,
        picker: bool,
        library: Option<(String, usize)>,
        current: impl FnOnce() -> bool,
        about_click: bool,
    ) -> DelightView {
        let Ok(mut owner) = self.delights.lock() else {
            return DelightView::default();
        };
        if !current() {
            return DelightView::default();
        }
        let fun = self.surprises_enabled();
        let before = owner.record.clone();
        let saved = owner.record();
        if saved.first_run == [0; 3] {
            saved.first_run = now.date();
        }
        let anniversary = saved.first_run[0]
            .checked_add(1)
            .is_some_and(|year| now.date() == [year, saved.first_run[1], saved.first_run[2]]);
        let mut view = DelightView {
            anniversary: fun && anniversary,
            birthday: fun && release_birthday(now, FIRST_STABLE_DATE),
            fun,
            build: format!(
                "{} · {} · {}",
                env!("CARGO_PKG_VERSION"),
                option_env!("GITHUB_SHA").unwrap_or("local"),
                env!("IBIS_BUILD_DATE")
            ),
            ..DelightView::default()
        };
        if fun && about_click {
            saved.about_clicks = (saved.about_clicks % 5) + 1;
            if saved.about_clicks == 5 {
                saved.about_clicks = 0;
                saved.fact_index = Some(saved.fact_index.map_or(0, |last| (last % 6 + 1) % 6));
            }
        }
        view.fact = if fun {
            saved.fact_index.map(|index| FACTS[usize::from(index % 6)])
        } else {
            None
        };
        if picker {
            view.peck = fun && !saved.welcome;
            if fun {
                saved.welcome = true;
            }
            if saved.paste_count > 0 && !saved.paste_tip {
                saved.paste_tip = true;
                view.status = Some(DelightStatus::PasteTip);
            }
            if let Some((identity, count)) = library
                && view.status.is_none()
                && fun
            {
                let seen = saved.libraries.entry(identity).or_default();
                // Claim only what is rendered; larger imports retain their other milestones.
                for (threshold, bit, status) in [
                    (100, 1, DelightStatus::Library100),
                    (1000, 2, DelightStatus::Library1000),
                    (10000, 4, DelightStatus::Library10000),
                ] {
                    if count >= threshold && *seen & bit == 0 {
                        *seen |= bit;
                        view.status = Some(status);
                        break;
                    }
                }
            }
        }
        if owner.record != before {
            owner.revision = self.issue_preference_revision().unwrap_or(owner.revision);
        }
        view.revision = owner.revision;
        view
    }

    /// Called only after an accepted successful native delivery. Never performs I/O.
    pub(crate) fn record_delight_paste(&self, token: u32) -> Option<u32> {
        let mut owner = self.delights.lock().ok()?;
        if !owner.deliveries.insert(token) {
            return None;
        }
        let saved = owner.record();
        saved.paste_count = saved.paste_count.saturating_add(1);
        let count = saved.paste_count;
        owner.revision = self.issue_preference_revision().unwrap_or(owner.revision);
        (self.surprises_enabled() && [1, 100, 1000].contains(&count)).then_some(count)
    }

    pub(crate) fn flush_delights(&self) -> Result<(), String> {
        // Snapshot INSIDE the single writer: delayed A always writes the latest B.
        let mut revision = 0;
        let saved = self.update_when(
            || self.delights.lock().is_ok_and(|owner| owner.revision > owner.flushed),
            |settings| {
                if let Ok(owner) = self.delights.lock() {
                    settings.delights = owner.record.clone();
                    revision = owner.revision;
                }
            },
        )?;
        if saved.is_some()
            && let Ok(mut owner) = self.delights.lock()
        {
            owner.flushed = owner.flushed.max(revision);
        }
        Ok(())
    }
}

pub(crate) fn flush_later(file: SettingsFile) {
    tauri::async_runtime::spawn_blocking(move || {
        let _ = file.flush_delights();
    });
}

fn quiet_load(status: &crate::library::LibraryStatus, revision: Option<&str>) -> bool {
    revision == Some(status.revision.as_str())
        && matches!(status.state, crate::library::LibraryState::Loaded)
        && status.has_snapshot
        && !status.stale
        && status.error.is_none()
        && status.note.is_none()
}

fn library_claim(status: &crate::library::LibraryStatus, revision: Option<&str>) -> Option<(String, usize)> {
    (quiet_load(status, revision) && !status.sample).then(|| {
        // A fixed, length-prefixed fingerprint survives Rust/toolchain upgrades.
        let mut identity = 0xcbf29ce484222325_u64;
        for source in status.source_counts.keys() {
            for byte in (source.len() as u64).to_le_bytes().into_iter().chain(source.bytes()) {
                identity = (identity ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        (format!("{identity:016x}"), status.references)
    })
}

/// One request per visible presentation/snapshot. Settings passes no presentation.
#[tauri::command]
#[specta::specta]
pub(crate) fn delight_view(
    app: AppHandle,
    now: LocalMoment,
    presentation: Option<u64>,
    revision: Option<String>,
    about_click: Option<bool>,
) -> DelightView {
    if !(1..=9999).contains(&now.year) || !(1..=12).contains(&now.month) || !(1..=31).contains(&now.day) {
        return DelightView::default();
    }
    let picker = presentation.is_some();
    if picker && presentation != Some(crate::picker::picker_presentation(app.state())) {
        return DelightView::default();
    }
    let file = app.state::<SettingsFile>();
    let view = if picker {
        let library = app.state::<std::sync::Mutex<crate::library::Library>>();
        let Ok(library) = library.lock() else {
            return DelightView::default();
        };
        // Keep the library owner through claiming; a replacement cannot consume an old snapshot's claim.
        let status = crate::library::snapshot_status(&library);
        let library = library_claim(&status, revision.as_deref());
        let mut view = file.claim_delights_when(
            now,
            quiet_load(&status, revision.as_deref()),
            library,
            || presentation == Some(crate::picker::picker_presentation(app.state())),
            false,
        );
        view.library = status.snapshot_name;
        view
    } else {
        file.claim_about(now, about_click.unwrap_or(false))
    };
    flush_later(file.inner().clone());
    view
}

#[cfg(test)]
#[path = "delights_tests.rs"]
mod tests;
