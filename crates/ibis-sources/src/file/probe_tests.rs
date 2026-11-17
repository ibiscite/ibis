use super::*;
use ibis_core::contract::{Availability, ItemRef, LibraryId, LibraryObject, Snapshot};

fn references() -> (Vec<LibraryObject>, Vec<(usize, String)>) {
    let mut objects = Vec::new();
    let fields = (0..500)
        .map(|index| {
            (
                ItemRef {
                    library: LibraryId::digest(b"invented"),
                    key: format!("record:item{index:03}"),
                },
                "blocked.pdf".into(),
                None,
            )
        })
        .collect();
    let references = crate::attachments::append(fields, &mut objects, &mut Vec::new());
    (objects, references)
}

#[test]
fn five_hundred_blocked_attachments_share_the_sources_remaining_deadline() {
    let (mut objects, references) = references();
    let time = Arc::new(Mutex::new(Instant::now()));
    let mut scope = Scope::new(&|| false);
    scope.0.until = *time.lock().unwrap_or_else(|e| panic!("{e}")) + Duration::from_millis(300);
    let clock = time.clone();
    scope.0.now = Arc::new(move || *clock.lock().unwrap_or_else(|e| panic!("{e}")));
    // Earlier source I/O consumes the same deadline as the attachment batch.
    let clock = time.clone();
    scope
        .run(move || {
            *clock.lock().unwrap_or_else(|e| panic!("{e}")) += Duration::from_millis(200);
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(scope.remaining(), Ok(Duration::from_millis(100)));
    let until = scope.0.until;
    let (probed, probes) = mpsc::channel();
    let (release, blocked) = mpsc::channel::<()>();
    let mut first = true;
    scope
        .attachments_with(&mut objects, references, move |_| {
            let mut now = time.lock().unwrap_or_else(|e| panic!("{e}"));
            probed
                .send(until.saturating_duration_since(*now))
                .unwrap_or_else(|e| panic!("{e}"));
            // Each stat consumes 50 ms; the second stays blocked at the deadline.
            *now += Duration::from_millis(50);
            drop(now);
            if first {
                first = false;
            } else {
                let _ = blocked.recv();
            }
            Availability::OnDisk
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let snapshot = Snapshot::new(objects).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(snapshot.objects().len(), 500);
    for (index, entry) in snapshot.objects().iter().enumerate() {
        let LibraryObject::Attachment(attachment) = &entry.object else {
            panic!("attachment")
        };
        assert_eq!(attachment.id.key, format!("attachment:record:item{index:03}:0"));
        assert_eq!(
            attachment.availability,
            if index == 0 {
                Availability::OnDisk
            } else {
                Availability::Unknown
            }
        );
    }
    assert_eq!(Scope::new(&|| false).run(|| Ok("local B")), Ok("local B"));
    // A late stat must not start the next probe or change the published result.
    drop(release);
    assert_eq!(
        probes.iter().collect::<Vec<_>>(),
        [Duration::from_millis(100), Duration::from_millis(50)]
    );
}

#[test]
fn cancellation_during_a_blocked_probe_returns_without_waiting_for_the_mount() {
    let (mut objects, references) = references();
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let (release, blocked) = mpsc::channel::<()>();
    let (started, probing) = mpsc::channel();
    std::thread::scope(|threads| {
        let load = threads.spawn(|| {
            let check = || cancelled.load(Ordering::Acquire);
            Scope::new(&check).attachments_with(&mut objects, references, move |_| {
                let _ = started.send(());
                let _ = blocked.recv();
                Availability::OnDisk
            })
        });
        probing
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|e| panic!("{e}"));
        let start = Instant::now();
        signal.store(true, Ordering::Release);
        assert_eq!(
            load.join().unwrap_or_else(|e| std::panic::resume_unwind(e)),
            Err(SourceError::Cancelled)
        );
        assert!(start.elapsed() < Duration::from_millis(200));
        release.send(()).unwrap_or_else(|e| panic!("{e}"));
    });
}
