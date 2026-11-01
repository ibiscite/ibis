use super::*;
use std::sync::atomic::AtomicUsize;

type Answer = Box<dyn FnOnce(bool) + Send>;
type Found = Result<Option<Candidate>, &'static str>;

/// Tauri's mock hard-codes visibility/focus. Replace just native actions and
/// feed responses; exercise the production check, deferral and consent paths.
#[derive(Default)]
struct NativePicker {
    visible: AtomicBool,
    focused: AtomicBool,
    native_focus: Mutex<Option<(Option<usize>, Option<usize>)>>,
    shows: AtomicUsize,
    refreshes: AtomicUsize,
    next: Mutex<Option<Found>>,
    offers: Mutex<Vec<(String, Answer)>>,
    menu_labels: Mutex<Vec<&'static str>>,
    notices: Mutex<Vec<String>>,
    installed: Mutex<Vec<String>>,
    /// Each closed offer: its presentation, and whether its lease was still held.
    closed: Mutex<Vec<(u64, bool)>>,
}

impl OfferUi for tauri::test::MockRuntime {
    fn visible(parent: &tauri::WebviewWindow<Self>) -> bool {
        parent.state::<NativePicker>().visible.load(Ordering::SeqCst)
    }

    fn focused(parent: &tauri::WebviewWindow<Self>) -> bool {
        let picker = parent.state::<NativePicker>();
        if let Some((main, foreground)) = *picker.native_focus.lock().unwrap_or_else(|e| panic!("{e}")) {
            crate::picker::foreground_focus(main, foreground) == Some(true)
        } else {
            picker.focused.load(Ordering::SeqCst)
        }
    }

    fn reveal_picker(app: &AppHandle<Self>) {
        let picker = app.state::<NativePicker>();
        picker.shows.fetch_add(1, Ordering::SeqCst);
        picker.visible.store(true, Ordering::SeqCst);
        picker.focused.store(true, Ordering::SeqCst);
        // Native show/focus hooks must not start another check or dialog.
        refresh_pending(app);
    }

    fn refresh(app: &AppHandle<Self>) {
        let picker = app.state::<NativePicker>();
        picker.refreshes.fetch_add(1, Ordering::SeqCst);
        let found = picker
            .next
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .take()
            .unwrap_or_else(|| panic!("a deferred offer must re-check the feed"));
        check_result(app, false, found);
    }

    fn availability_changed(app: &AppHandle<Self>, available: bool) {
        app.state::<NativePicker>()
            .menu_labels
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .push(menu_label(available));
    }

    fn tell(app: &AppHandle<Self>, text: &str) {
        app.state::<NativePicker>()
            .notices
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .push(text.into());
    }

    fn dialog_closed(parent: &tauri::WebviewWindow<Self>, generation: u64) {
        parent
            .state::<NativePicker>()
            .closed
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .push((generation, parent.state::<ChooserOpen>().is_open()));
    }

    fn present(
        app: &AppHandle<Self>,
        parent: &tauri::WebviewWindow<Self>,
        owner: ChooserLease,
        text: String,
        answer: impl FnOnce(bool) + Send + 'static,
    ) {
        assert_eq!(parent.label(), "main");
        assert!(Self::visible(parent));
        assert!(Self::focused(parent));
        app.state::<NativePicker>()
            .offers
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .push((
                text,
                Box::new(move |yes| {
                    drop(owner);
                    answer(yes);
                }),
            ));
    }

    fn present_question(
        app: &AppHandle<Self>,
        parent: &tauri::WebviewWindow<Self>,
        owner: ChooserLease,
        answer: impl FnOnce(bool) + Send + 'static,
    ) {
        assert_eq!(parent.label(), "main");
        app.state::<NativePicker>()
            .offers
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .push((
                "first-run question".into(),
                Box::new(move |yes| {
                    drop(owner);
                    answer(yes);
                }),
            ));
    }

    fn save_automatic(app: &AppHandle<Self>, on: bool) {
        app.state::<Updates>().automatic.store(on, Ordering::SeqCst);
    }

    fn return_focus(app: &AppHandle<Self>) {
        Self::reveal_picker(app);
    }
}

fn app(automatic: bool) -> tauri::Result<tauri::App<tauri::test::MockRuntime>> {
    let app = tauri::test::mock_builder()
        .manage(Updates::default())
        .manage(ChooserOpen::default())
        .manage(crate::picker::PickerGeneration::default())
        .manage(NativePicker::default())
        .build(tauri::test::mock_context(tauri::test::noop_assets()))?;
    tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .visible(false)
        .build()?;
    app.state::<Updates>().automatic.store(automatic, Ordering::SeqCst);
    // Existing scenarios start after the empty picker's idle handshake.
    let updates = app.state::<Updates>();
    let mut input = updates.input.lock().unwrap_or_else(|e| panic!("{e}"));
    let ticket = input.activity().unwrap_or_else(|| panic!("input ticket"));
    assert!(input.settle(ticket));
    drop(input);
    Ok(app)
}

fn pending_question(app: &tauri::App<tauri::test::MockRuntime>) -> usize {
    app.state::<Updates>().question.store(true, Ordering::SeqCst);
    app.state::<NativePicker>()
        .offers
        .lock()
        .unwrap_or_else(|e| panic!("{e}"))
        .len()
}

#[test]
fn first_run_question_keeps_the_picker_and_answering_gives_it_the_keyboard_back() -> tauri::Result<()> {
    for yes in [true, false] {
        let app = app(!yes)?;
        assert_eq!(pending_question(&app), 0);
        let picker = app.state::<NativePicker>();
        picker.visible.store(true, Ordering::SeqCst);
        ask_first_run(app.handle());
        // The open question is what suppresses the picker's hide on blur.
        assert!(app.state::<ChooserOpen>().is_open());
        assert!(!crate::picker::hides_on_blur(
            true,
            app.state::<ChooserOpen>().is_open(),
            false
        ));
        // Asked once: the dialog's own focus changes must not ask again.
        ask_first_run(app.handle());
        assert_eq!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).len(), 1);
        picker.focused.store(false, Ordering::SeqCst);
        let (_, answer) = picker
            .offers
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .pop()
            .unwrap_or_else(|| panic!("the question must be asked"));
        answer(yes);
        assert!(!app.state::<ChooserOpen>().is_open());
        assert_eq!(app.state::<Updates>().automatic.load(Ordering::SeqCst), yes);
        assert!(picker.visible.load(Ordering::SeqCst), "nothing left on screen");
        assert!(picker.focused.load(Ordering::SeqCst), "typing goes to the picker");
    }
    Ok(())
}

#[test]
fn first_run_question_never_opens_the_picker_by_itself_and_is_asked_only_when_needed() -> tauri::Result<()> {
    let app = app(false)?;
    // No focused picker has asked, as on a hidden start at login.
    ask_first_run(app.handle());
    assert!(
        app.state::<NativePicker>()
            .offers
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .is_empty()
    );
    assert_eq!(app.state::<NativePicker>().shows.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn first_run_question_waits_for_a_chooser_that_is_open_and_asks_on_the_next_focus() -> tauri::Result<()> {
    let app = app(false)?;
    pending_question(&app);
    let chooser = app.state::<ChooserOpen>().begin();
    ask_first_run(app.handle());
    assert!(
        app.state::<NativePicker>()
            .offers
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .is_empty()
    );
    drop(chooser);
    ask_first_run(app.handle());
    assert_eq!(
        app.state::<NativePicker>()
            .offers
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .len(),
        1
    );
    Ok(())
}

#[test]
fn automatic_offer_waits_for_idle_and_drops_metadata_found_while_typing() -> tauri::Result<()> {
    let app = app(true)?;
    let picker = app.state::<NativePicker>();
    picker.visible.store(true, Ordering::SeqCst);
    picker.focused.store(true, Ordering::SeqCst);
    let updates = app.state::<Updates>();
    let ticket = updates
        .input
        .lock()
        .unwrap_or_else(|e| panic!("{e}"))
        .activity()
        .unwrap_or_else(|| panic!("typing ticket"));
    check_result(
        app.handle(),
        false,
        Ok(Some(candidate(app.handle(), "9.0.0", "typing metadata"))),
    );
    assert!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(updates.offer_available.load(Ordering::SeqCst));
    assert!(updates.begin().is_some());
    assert!(updates.input.lock().unwrap_or_else(|e| panic!("{e}")).settle(ticket));
    *picker.next.lock().unwrap_or_else(|e| panic!("{e}")) =
        Some(Ok(Some(candidate(app.handle(), "9.1.0", "idle metadata"))));
    refresh_pending(app.handle());
    assert!(answer(app.handle(), true).contains("9.1.0"));
    assert_eq!(
        *picker.installed.lock().unwrap_or_else(|e| panic!("{e}")),
        ["idle metadata"]
    );
    Ok(())
}

#[test]
fn typing_during_a_check_and_old_idle_after_new_activity_never_present() -> tauri::Result<()> {
    let app = app(true)?;
    let picker = app.state::<NativePicker>();
    picker.visible.store(true, Ordering::SeqCst);
    picker.focused.store(true, Ordering::SeqCst);
    tauri::async_runtime::block_on(check(app.handle(), false, async {
        let updates = app.state::<Updates>();
        let mut input = updates.input.lock().unwrap_or_else(|e| panic!("{e}"));
        let old = input.activity().unwrap_or_else(|| panic!("old ticket"));
        let _new = input.activity(); // another key, hide or reopen
        assert!(!input.settle(old));
        Ok(Some(candidate(app.handle(), "9.0.0", "obsolete metadata")))
    }));
    assert!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(picker.installed.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(app.state::<Updates>().begin().is_some());
    Ok(())
}

#[test]
fn unknown_input_and_exhausted_tickets_never_permit_automatic_offers() {
    let mut input = InputGate::default();
    assert!(!input.idle);
    assert!(!input.settle(0));
    let current = input.activity().unwrap_or_else(|| panic!("input ticket"));
    assert!(input.settle(current));
    assert!(!input.settle(current));
    let _ = input.activity();
    input.serial = u32::MAX - 1;
    assert!(input.activity().is_none());
    assert!(!input.settle(u32::MAX));
    assert!(input.activity().is_none());
    assert!(!input.idle);
}

fn candidate(app: &AppHandle<tauri::test::MockRuntime>, version: &str, metadata: &str) -> Candidate {
    let handle = app.clone();
    let metadata = metadata.to_owned();
    Candidate {
        version: version.into(),
        text: format!("Ibis {version} is available"),
        install: Box::new(move |busy| {
            handle
                .state::<NativePicker>()
                .installed
                .lock()
                .unwrap_or_else(|e| panic!("{e}"))
                .push(metadata);
            drop(busy);
        }),
    }
}

fn check_result(app: &AppHandle<tauri::test::MockRuntime>, asked: bool, found: Found) {
    tauri::async_runtime::block_on(check(app, asked, std::future::ready(found)));
}

#[test]
fn automatic_offer_uses_the_exact_native_foreground_when_the_webview_has_focus() -> tauri::Result<()> {
    for (toolkit_focus, foreground, offered) in [(false, Some(10), true), (true, Some(20), false), (true, None, false)]
    {
        let app = app(true)?;
        let picker = app.state::<NativePicker>();
        picker.visible.store(true, Ordering::SeqCst);
        picker.focused.store(toolkit_focus, Ordering::SeqCst);
        *picker.native_focus.lock().unwrap_or_else(|e| panic!("{e}")) = Some((Some(10), foreground));
        check_result(app.handle(), false, Ok(Some(candidate(app.handle(), "9.1.0", "fresh"))));
        assert_eq!(
            picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).len(),
            usize::from(offered)
        );
        if offered {
            answer(app.handle(), true);
            assert_eq!(*picker.installed.lock().unwrap_or_else(|e| panic!("{e}")), ["fresh"]);
        }
        assert_eq!(picker.shows.load(Ordering::SeqCst), 0);
    }
    Ok(())
}

#[test]
fn a_held_chooser_defers_even_a_native_focused_offer_and_reopening_checks_fresh_metadata() -> tauri::Result<()> {
    let app = app(true)?;
    let picker = app.state::<NativePicker>();
    picker.visible.store(true, Ordering::SeqCst);
    *picker.native_focus.lock().unwrap_or_else(|e| panic!("{e}")) = Some((Some(10), Some(10)));
    let chooser = app
        .state::<ChooserOpen>()
        .begin()
        .unwrap_or_else(|| panic!("owned chooser"));
    check_result(app.handle(), false, Ok(Some(candidate(app.handle(), "9.0.0", "old"))));
    assert!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(app.state::<Updates>().offer_available.load(Ordering::SeqCst));
    drop(chooser);
    *picker.next.lock().unwrap_or_else(|e| panic!("{e}")) = Some(Ok(Some(candidate(app.handle(), "9.1.0", "fresh"))));
    refresh_pending(app.handle());
    assert!(answer(app.handle(), true).contains("9.1.0"));
    assert_eq!(*picker.installed.lock().unwrap_or_else(|e| panic!("{e}")), ["fresh"]);
    assert_eq!(picker.shows.load(Ordering::SeqCst), 0);
    Ok(())
}

fn answer(app: &AppHandle<tauri::test::MockRuntime>, accepted: bool) -> String {
    let (text, answer) = app
        .state::<NativePicker>()
        .offers
        .lock()
        .unwrap_or_else(|e| panic!("{e}"))
        .pop()
        .unwrap_or_else(|| panic!("one native offer"));
    answer(accepted);
    assert!(!app.state::<ChooserOpen>().is_open());
    text
}

#[test]
fn automatic_update_while_hidden_signals_availability_without_showing_or_holding_the_check() -> tauri::Result<()> {
    use tauri::Listener;

    let app = app(true)?;
    let events = Arc::new(Mutex::new(Vec::<bool>::new()));
    let heard = Arc::clone(&events);
    app.listen_any("update:available", move |event| {
        heard
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .push(event.payload() == "true");
    });
    check_result(
        app.handle(),
        false,
        Ok(Some(candidate(app.handle(), "9.0.0", "old signature"))),
    );
    let picker = app.state::<NativePicker>();
    let updates = app.state::<Updates>();
    assert!(!picker.visible.load(Ordering::SeqCst));
    assert!(!picker.focused.load(Ordering::SeqCst));
    assert_eq!(picker.shows.load(Ordering::SeqCst), 0);
    assert!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(updates.offer_available.load(Ordering::SeqCst));
    assert!(
        updates.begin().is_some(),
        "deferred availability releases the check lease"
    );
    assert!(!app.state::<ChooserOpen>().is_open());
    assert_eq!(
        *picker.menu_labels.lock().unwrap_or_else(|e| panic!("{e}")),
        ["Update available…"]
    );
    assert_eq!(*events.lock().unwrap_or_else(|e| panic!("{e}")), [true]);

    // Neither hidden+stale focus nor visible+unfocused starts a refresh.
    picker.focused.store(true, Ordering::SeqCst);
    refresh_pending(app.handle());
    picker.focused.store(false, Ordering::SeqCst);
    picker.visible.store(true, Ordering::SeqCst);
    refresh_pending(app.handle());
    assert_eq!(picker.refreshes.load(Ordering::SeqCst), 0);
    Ok(())
}

#[test]
fn explicit_command_with_automatic_checks_off_opens_and_presents_immediately() -> tauri::Result<()> {
    let app = app(false)?;
    assert!(!app.state::<Updates>().automatic.load(Ordering::SeqCst));
    check_result(
        app.handle(),
        true,
        Ok(Some(candidate(app.handle(), "9.1.0", "fresh signature"))),
    );
    let picker = app.state::<NativePicker>();
    assert_eq!(picker.shows.load(Ordering::SeqCst), 1);
    assert_eq!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).len(), 1);
    assert!(!app.state::<Updates>().offer_available.load(Ordering::SeqCst));
    assert!(
        app.state::<Updates>().begin().is_none(),
        "a visible consent dialog owns the check"
    );
    assert!(picker.closed.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(answer(app.handle(), false).contains("9.1.0"));
    // The person may have gone to another app while the offer was open. With
    // the lease released, the picker gets the blur check it missed, for the
    // presentation that opened the offer.
    let current = app.state::<crate::picker::PickerGeneration>().current();
    assert_eq!(
        *picker.closed.lock().unwrap_or_else(|e| panic!("{e}")),
        [(current, false)]
    );
    assert!(app.state::<Updates>().begin().is_some());
    assert!(picker.installed.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    Ok(())
}

#[test]
fn deferred_offers_allow_new_checks_and_recheck_before_presenting_newest_metadata() -> tauri::Result<()> {
    let app = app(true)?;
    check_result(
        app.handle(),
        false,
        Ok(Some(candidate(app.handle(), "9.0.0", "old signature"))),
    );
    check_result(
        app.handle(),
        false,
        Ok(Some(candidate(app.handle(), "9.1.0", "replaced signature"))),
    );
    let picker = app.state::<NativePicker>();
    *picker.next.lock().unwrap_or_else(|e| panic!("{e}")) =
        Some(Ok(Some(candidate(app.handle(), "9.2.0", "latest signature"))));
    picker.visible.store(true, Ordering::SeqCst);
    picker.focused.store(true, Ordering::SeqCst);
    refresh_pending(app.handle());
    refresh_pending(app.handle()); // no duplicate while the consent dialog owns the check
    assert_eq!(picker.refreshes.load(Ordering::SeqCst), 1);
    assert_eq!(
        picker.shows.load(Ordering::SeqCst),
        0,
        "returning offers never show/focus the picker"
    );
    assert!(answer(app.handle(), true).contains("9.2.0"));
    assert_eq!(
        *picker.installed.lock().unwrap_or_else(|e| panic!("{e}")),
        ["latest signature"]
    );
    assert_eq!(
        *picker.menu_labels.lock().unwrap_or_else(|e| panic!("{e}")),
        ["Update available…", "Check for Updates…"]
    );
    Ok(())
}

#[test]
fn explicit_command_refreshes_a_hidden_pending_offer_instead_of_reporting_waiting() -> tauri::Result<()> {
    let app = app(true)?;
    check_result(
        app.handle(),
        false,
        Ok(Some(candidate(app.handle(), "9.0.0", "old signature"))),
    );
    check_result(
        app.handle(),
        true,
        Ok(Some(candidate(app.handle(), "9.1.0", "fresh signature"))),
    );
    assert_eq!(app.state::<NativePicker>().shows.load(Ordering::SeqCst), 1);
    assert!(answer(app.handle(), true).contains("9.1.0"));
    assert!(
        app.state::<NativePicker>()
            .notices
            .lock()
            .unwrap_or_else(|e| panic!("{e}"))
            .is_empty()
    );
    assert_eq!(
        *app.state::<NativePicker>()
            .installed
            .lock()
            .unwrap_or_else(|e| panic!("{e}")),
        ["fresh signature"]
    );
    Ok(())
}

#[test]
fn failed_or_empty_refresh_never_presents_or_installs_the_deferred_candidate() -> tauri::Result<()> {
    let app = app(true)?;
    check_result(
        app.handle(),
        false,
        Ok(Some(candidate(app.handle(), "9.0.0", "old signature"))),
    );
    let picker = app.state::<NativePicker>();
    picker.visible.store(true, Ordering::SeqCst);
    picker.focused.store(true, Ordering::SeqCst);
    *picker.next.lock().unwrap_or_else(|e| panic!("{e}")) = Some(Err("network"));
    refresh_pending(app.handle());
    assert!(app.state::<Updates>().offer_available.load(Ordering::SeqCst));
    assert!(app.state::<Updates>().begin().is_some());
    *picker.next.lock().unwrap_or_else(|e| panic!("{e}")) = Some(Ok(None));
    refresh_pending(app.handle());
    assert!(!app.state::<Updates>().offer_available.load(Ordering::SeqCst));
    assert!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(picker.installed.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    Ok(())
}

#[test]
fn picker_hidden_during_refresh_stays_hidden_and_drops_the_fresh_candidate() -> tauri::Result<()> {
    let app = app(true)?;
    let picker = app.state::<NativePicker>();
    picker.visible.store(true, Ordering::SeqCst);
    picker.focused.store(true, Ordering::SeqCst);
    tauri::async_runtime::block_on(check(app.handle(), false, async {
        picker.visible.store(false, Ordering::SeqCst);
        picker.focused.store(false, Ordering::SeqCst);
        Ok(Some(candidate(app.handle(), "9.0.0", "unused signature")))
    }));
    assert_eq!(picker.shows.load(Ordering::SeqCst), 0);
    assert!(picker.offers.lock().unwrap_or_else(|e| panic!("{e}")).is_empty());
    assert!(app.state::<Updates>().offer_available.load(Ordering::SeqCst));
    assert!(app.state::<Updates>().begin().is_some());
    Ok(())
}

#[test]
fn disabling_automatic_checks_keeps_the_signal_but_requires_an_explicit_refresh() -> tauri::Result<()> {
    let app = app(true)?;
    check_result(
        app.handle(),
        false,
        Ok(Some(candidate(app.handle(), "9.0.0", "old signature"))),
    );
    let picker = app.state::<NativePicker>();
    app.state::<Updates>().automatic.store(false, Ordering::SeqCst);
    picker.visible.store(true, Ordering::SeqCst);
    picker.focused.store(true, Ordering::SeqCst);
    refresh_pending(app.handle());
    assert_eq!(picker.refreshes.load(Ordering::SeqCst), 0);
    assert!(app.state::<Updates>().offer_available.load(Ordering::SeqCst));
    // An automatic task scheduled before opt-out also must not poll its feed.
    tauri::async_runtime::block_on(check(app.handle(), false, async {
        panic!("disabled automatic checks must not fetch");
    }));
    check_result(
        app.handle(),
        true,
        Ok(Some(candidate(app.handle(), "9.1.0", "fresh signature"))),
    );
    assert!(answer(app.handle(), true).contains("9.1.0"));
    assert_eq!(
        *picker.installed.lock().unwrap_or_else(|e| panic!("{e}")),
        ["fresh signature"]
    );
    Ok(())
}
