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
    let mut scope = Scope::new(&|| false);
    scope.until = Instant::now() + Duration::from_millis(300);
    // Earlier source I/O consumes the same deadline as the attachment batch.
    scope
        .run(|| {
            std::thread::sleep(Duration::from_millis(200));
            Ok(())
        })
        .unwrap_or_else(|e| panic!("{e}"));
    let (release, blocked) = mpsc::channel::<()>();
    let start = Instant::now();
    let mut first = true;
    scope
        .attachments_with(&mut objects, references, move |_| {
            if first {
                first = false;
                return Availability::OnDisk;
            }
            let _ = blocked.recv();
            Availability::OnDisk
        })
        .unwrap_or_else(|e| panic!("{e}"));
    assert!(start.elapsed() < Duration::from_millis(200));
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
    release.send(()).unwrap_or_else(|e| panic!("{e}"));
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
