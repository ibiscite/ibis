//! Test-only Notepad lifecycle and activation, also exercised without Windows.

use std::process::Child;
use std::time::{Duration, Instant};

/// Only portable values cross UI Automation query threads. An absent or
/// implausible provider identity can never identify the synthetic editor.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct EditorIdentity {
    pub process: u32,
    runtime: Vec<i32>,
}

impl EditorIdentity {
    pub fn new(process: u32, runtime: Vec<i32>) -> Option<Self> {
        (process != 0 && !runtime.is_empty() && runtime.len() <= 128).then_some(Self { process, runtime })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Query,
    Automation,
    Window,
    Document,
    Foreground,
    Editor,
    Identity,
    Focus,
    Password,
    Pattern,
    Range,
    Read,
    NotEmpty,
    Input,
    Readback,
}

/// Fixed classifications only: never retain provider messages or text here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Failure {
    pub stage: Stage,
    pub code: Option<i32>,
    pub owned: Option<bool>,
    pub focused: Option<bool>,
}

impl Failure {
    pub const fn new(stage: Stage) -> Self {
        Self {
            stage,
            code: None,
            owned: None,
            focused: None,
        }
    }
}

pub(crate) struct Reading {
    pub editor: EditorIdentity,
    pub text: String,
}

#[derive(Clone, Copy)]
pub(crate) enum CandidateKind {
    ModernDocument,
    ClassicRootEdit,
    Other,
}

pub(crate) struct Candidate {
    pub kind: Option<CandidateKind>,
    pub editor: Option<EditorIdentity>,
}

/// Native discovery supplies positive semantic/structural evidence. A first
/// text-field match is not itself a document identity.
pub(crate) fn select_document(candidates: &[Candidate]) -> Result<EditorIdentity, Failure> {
    let mut selected = None;
    for candidate in candidates {
        match candidate.kind {
            Some(CandidateKind::Other) => continue,
            None => return Err(Failure::new(Stage::Editor)),
            Some(CandidateKind::ModernDocument | CandidateKind::ClassicRootEdit) => {}
        }
        let editor = candidate.editor.as_ref().ok_or(Failure::new(Stage::Identity))?;
        if selected.replace(editor.clone()).is_some() {
            return Err(Failure::new(Stage::Editor));
        }
    }
    selected.ok_or(Failure::new(Stage::Editor))
}

/// The real Windows test and portable consumer regressions share this exact
/// sequence. Readbacks are never inferred from successful input injection.
pub(crate) fn qualify_paste(
    editor: &EditorIdentity,
    expected: &str,
    limit: Duration,
    read: impl Fn(&EditorIdentity) -> Result<Reading, Failure>,
    prepare: impl FnOnce() -> Result<(), Failure>,
    paste: impl FnOnce(&dyn Fn() -> bool) -> bool,
) -> Result<(), Failure> {
    let checked = || {
        let reading = read(editor)?;
        if reading.editor != *editor {
            return Err(Failure {
                focused: Some(false),
                ..Failure::new(Stage::Identity)
            });
        }
        Ok(reading.text)
    };
    let await_text = |wanted: &str, stage| {
        let mut last = Failure::new(stage);
        wait_for(limit, || match checked() {
            Ok(text) if text == wanted => Some(()),
            Ok(_) => {
                last = Failure {
                    owned: Some(true),
                    focused: Some(true),
                    ..Failure::new(stage)
                };
                None
            }
            Err(failure) => {
                last = failure;
                None
            }
        })
        .ok_or(last)
    };
    await_text("", Stage::NotEmpty)?;
    prepare()?;
    let refused = std::cell::Cell::new(None);
    if !paste(&|| match checked() {
        Ok(text) if text.is_empty() => true,
        Ok(_) => {
            refused.set(Some(Failure {
                owned: Some(true),
                focused: Some(true),
                ..Failure::new(Stage::NotEmpty)
            }));
            false
        }
        Err(failure) => {
            refused.set(Some(failure));
            false
        }
    }) {
        return Err(refused.get().unwrap_or(Failure::new(Stage::Input)));
    }
    await_text(expected, Stage::Readback)
}

pub(crate) fn wait_for<T>(limit: Duration, mut condition: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + limit;
    loop {
        if let Some(value) = condition() {
            return Some(value);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(100)));
    }
}

/// Keep stable process handles, never infer ownership from a PID or HWND.
/// Even the launcher may restore unrelated documents, so neither process is
/// known to be exclusive to this test. Dropping a Child does not terminate it.
pub(crate) struct Session<P> {
    pub launcher: Child,
    pub process: Option<P>,
}

impl<P> Drop for Session<P> {
    fn drop(&mut self) {
        let _ = self.launcher.try_wait();
    }
}

/// Each attempt must wait for activation to complete before trying ALT.
/// Focus recovery runs after activation, including when ALT entered menu mode.
pub(crate) fn activate(
    limit: Duration,
    mut request: impl FnMut(),
    in_front: impl Fn() -> bool,
    tap_alt: impl FnOnce(),
    focus_document: impl FnOnce(bool),
) -> bool {
    request();
    let used_alt = wait_for(limit, || in_front().then_some(())).is_none();
    if used_alt {
        tap_alt();
        request();
        if wait_for(limit, || in_front().then_some(())).is_none() {
            return false;
        }
    }
    focus_document(used_alt);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};

    fn editor(id: i32) -> EditorIdentity {
        EditorIdentity::new(7, vec![42, id]).unwrap_or_else(|| panic!("synthetic editor identity"))
    }

    #[test]
    fn one_paste_must_change_the_owned_document_not_a_same_process_search_field() {
        use std::cell::RefCell;
        let owner = editor(1);
        let focus = Cell::new(1);
        let document = RefCell::new(String::new());
        let search = RefCell::new(String::new());
        let sends = Cell::new(0);
        let outcome = qualify_paste(
            &owner,
            "synthetic citation",
            Duration::ZERO,
            |_| {
                Ok(Reading {
                    editor: editor(focus.get()),
                    text: if focus.get() == 1 {
                        document.borrow().clone()
                    } else {
                        search.borrow().clone()
                    },
                })
            },
            || Ok(()),
            |ready| {
                if !ready() {
                    return false;
                }
                // Focus changes after the final guard. The actual document is
                // unchanged even though another field receives the citation.
                focus.set(2);
                *search.borrow_mut() = "synthetic citation".into();
                sends.set(sends.get() + 1);
                true
            },
        );
        assert!(
            outcome.is_err(),
            "a search-field readback must not qualify a document paste"
        );
        assert!(document.borrow().is_empty());
        assert_eq!(&*search.borrow(), "synthetic citation");
        assert_eq!(sends.get(), 1);
    }

    #[test]
    fn replacing_focus_before_send_refuses_input_and_an_old_editor_cannot_own_a_fresh_attempt() {
        for replaced_before_send in [false, true] {
            let owner = editor(2);
            let focus = Cell::new(if replaced_before_send { 2 } else { 1 });
            let sends = Cell::new(0);
            let outcome = qualify_paste(
                &owner,
                "synthetic citation",
                Duration::ZERO,
                |_| {
                    Ok(Reading {
                        editor: editor(focus.get()),
                        text: String::new(),
                    })
                },
                || {
                    focus.set(1);
                    Ok(())
                },
                |ready| {
                    if ready() {
                        sends.set(sends.get() + 1);
                        true
                    } else {
                        false
                    }
                },
            );
            assert_eq!(outcome.map_err(|failure| failure.stage), Err(Stage::Identity));
            assert_eq!(sends.get(), 0);
        }
    }

    #[test]
    fn successful_qualification_reads_actual_document_after_exactly_one_send() {
        let document = std::cell::RefCell::new(String::new());
        let sends = Cell::new(0);
        let outcome = qualify_paste(
            &editor(1),
            "synthetic citation",
            Duration::ZERO,
            |owner| {
                Ok(Reading {
                    editor: owner.clone(),
                    text: document.borrow().clone(),
                })
            },
            || Ok(()),
            |ready| {
                if !ready() {
                    return false;
                }
                sends.set(sends.get() + 1);
                *document.borrow_mut() = "synthetic citation".into();
                true
            },
        );
        assert_eq!(outcome, Ok(()));
        assert_eq!(&*document.borrow(), "synthetic citation");
        assert_eq!(sends.get(), 1);
    }

    #[test]
    fn provider_failure_and_safety_refusal_never_send_or_disclose_provider_payloads() {
        let failure = Failure {
            code: Some(-2147467259),
            ..Failure::new(Stage::Read)
        };
        let outcome = qualify_paste(
            &editor(1),
            "synthetic citation",
            Duration::ZERO,
            |_| Err(failure),
            || Ok(()),
            |_| panic!("failed I/O must not send"),
        );
        assert_eq!(outcome, Err(failure));
        let outcome = qualify_paste(
            &editor(1),
            "synthetic citation",
            Duration::ZERO,
            |owner| {
                Ok(Reading {
                    editor: owner.clone(),
                    text: String::new(),
                })
            },
            || Err(Failure::new(Stage::Password)),
            |_| panic!("refusal must not send"),
        );
        assert_eq!(outcome.map_err(|failure| failure.stage), Err(Stage::Password));
        for stage in [
            Stage::Query,
            Stage::Automation,
            Stage::Window,
            Stage::Document,
            Stage::Foreground,
            Stage::Editor,
            Stage::Identity,
            Stage::Focus,
            Stage::Password,
            Stage::Pattern,
            Stage::Range,
            Stage::Read,
            Stage::NotEmpty,
            Stage::Input,
            Stage::Readback,
        ] {
            let diagnostic = format!("{:?}", Failure::new(stage));
            assert!(diagnostic.len() < 120);
            assert!(!diagnostic.contains("synthetic citation"));
            assert!(diagnostic.contains("owned: None") && diagnostic.contains("focused: None"));
        }
    }

    #[test]
    fn unknown_or_unbounded_editor_identity_never_qualifies() {
        assert!(EditorIdentity::new(0, vec![1]).is_none());
        assert!(EditorIdentity::new(7, Vec::new()).is_none());
        assert!(EditorIdentity::new(7, vec![1; 129]).is_none());
    }

    #[test]
    fn failed_final_readback_cannot_turn_successful_input_into_a_pass() {
        let document = std::cell::RefCell::new(String::new());
        let sends = Cell::new(0);
        let failure = Failure {
            code: Some(-2147467259),
            owned: Some(true),
            ..Failure::new(Stage::Read)
        };
        let outcome = qualify_paste(
            &editor(1),
            "synthetic citation",
            Duration::ZERO,
            |owner| {
                if sends.get() > 0 {
                    return Err(failure);
                }
                Ok(Reading {
                    editor: owner.clone(),
                    text: document.borrow().clone(),
                })
            },
            || Ok(()),
            |ready| {
                if !ready() {
                    return false;
                }
                sends.set(sends.get() + 1);
                *document.borrow_mut() = "synthetic citation".into();
                true
            },
        );
        assert_eq!(outcome, Err(failure));
        assert_eq!(&*document.borrow(), "synthetic citation");
        assert_eq!(sends.get(), 1);
    }

    #[test]
    fn qualification_refuses_preexisting_or_added_whitespace_instead_of_trimming_it() {
        for (before, after, expected_sends) in [
            (" ", " synthetic citation", 0),
            ("", " synthetic citation", 1),
            ("", "synthetic citation\n", 1),
        ] {
            let document = std::cell::RefCell::new(before.to_owned());
            let sends = Cell::new(0);
            let outcome = qualify_paste(
                &editor(1),
                "synthetic citation",
                Duration::ZERO,
                |owner| {
                    Ok(Reading {
                        editor: owner.clone(),
                        text: document.borrow().clone(),
                    })
                },
                || Ok(()),
                |ready| {
                    if !ready() {
                        return false;
                    }
                    sends.set(sends.get() + 1);
                    *document.borrow_mut() = after.into();
                    true
                },
            );
            assert!(outcome.is_err(), "whitespace changed the actual document bytes");
            assert_eq!(sends.get(), expected_sends);
        }
    }

    #[test]
    fn capture_uses_unique_document_semantics_not_first_same_process_text_field() {
        for kind in [CandidateKind::ModernDocument, CandidateKind::ClassicRootEdit] {
            let candidates = [
                Candidate {
                    kind: Some(CandidateKind::Other),
                    editor: Some(editor(2)),
                },
                Candidate {
                    kind: Some(kind),
                    editor: Some(editor(1)),
                },
            ];
            let captured = select_document(&candidates).unwrap_or_else(|_| panic!("positive synthetic document"));
            assert!(captured == editor(1), "same-process search field captured as document");
            let document = std::cell::RefCell::new(String::new());
            let outcome = qualify_paste(
                &captured,
                "synthetic citation",
                Duration::ZERO,
                |_| {
                    Ok(Reading {
                        editor: editor(1),
                        text: document.borrow().clone(),
                    })
                },
                || Ok(()),
                |ready| {
                    if !ready() {
                        return false;
                    }
                    *document.borrow_mut() = "synthetic citation".into();
                    true
                },
            );
            assert_eq!(outcome, Ok(()));
            assert_eq!(&*document.borrow(), "synthetic citation");
        }
    }

    #[test]
    fn ambiguous_unknown_or_unidentified_document_candidates_never_reach_input() {
        for candidates in [
            vec![
                Candidate {
                    kind: Some(CandidateKind::ModernDocument),
                    editor: Some(editor(1)),
                },
                Candidate {
                    kind: Some(CandidateKind::ModernDocument),
                    editor: Some(editor(2)),
                },
            ],
            vec![Candidate {
                kind: None,
                editor: Some(editor(1)),
            }],
            vec![Candidate {
                kind: Some(CandidateKind::ModernDocument),
                editor: None,
            }],
            vec![Candidate {
                kind: Some(CandidateKind::Other),
                editor: Some(editor(1)),
            }],
            vec![],
        ] {
            let sends = Cell::new(0);
            let outcome = select_document(&candidates).and_then(|owner| {
                qualify_paste(
                    &owner,
                    "synthetic citation",
                    Duration::ZERO,
                    |identity| {
                        Ok(Reading {
                            editor: identity.clone(),
                            text: String::new(),
                        })
                    },
                    || Ok(()),
                    |ready| {
                        if ready() {
                            sends.set(sends.get() + 1);
                            true
                        } else {
                            false
                        }
                    },
                )
            });
            assert!(outcome.is_err());
            assert_eq!(sends.get(), 0);
        }
    }

    #[test]
    fn delayed_activation_finishes_without_alt() {
        let checks = Cell::new(0);
        let focused = Cell::new(false);
        assert!(activate(
            Duration::from_millis(1),
            || {},
            || {
                // The request returned before the target input queue handled it.
                checks.set(checks.get() + 1);
                checks.get() > 1
            },
            || panic!("ALT must not race an activation that is still completing"),
            |used_alt| {
                assert!(!used_alt);
                assert!(checks.get() > 1);
                focused.set(true);
            }
        ));
        assert!(focused.get());
    }

    #[test]
    fn alt_menu_mode_is_recovered_after_activation() {
        let attempts = Cell::new(0);
        let menu = Cell::new(false);
        let foreground = Cell::new(false);
        let focused = Cell::new(false);
        assert!(activate(
            Duration::ZERO,
            || {
                attempts.set(attempts.get() + 1);
                foreground.set(attempts.get() == 2);
            },
            || foreground.get(),
            || {
                assert_eq!(attempts.get(), 1);
                menu.set(true);
            },
            |used_alt| {
                assert!(used_alt);
                assert_eq!(attempts.get(), 2);
                assert!(menu.replace(false));
                focused.set(true);
            }
        ));
        assert!(!menu.get());
        assert!(focused.get());
    }

    #[test]
    fn denied_activation_does_not_focus_another_app() {
        let tapped = Cell::new(false);
        assert!(!activate(
            Duration::ZERO,
            || {},
            || false,
            || tapped.set(true),
            |_| panic!("must not focus or dismiss menus in another app")
        ));
        assert!(tapped.get());
    }

    #[test]
    fn cleanup_leaves_launcher_and_handed_off_process_alive() {
        let spawn = || {
            Command::new(std::env::current_exe().unwrap_or_else(|error| panic!("test executable: {error}")))
                .args(["notepad_process_child", "--ignored", "--nocapture"])
                .env("IBIS_NOTEPAD_TEST_CHILD", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap_or_else(|error| panic!("test child: {error}"))
        };
        let mut launcher = spawn();
        let mut shared = spawn();
        let pipes = [&mut launcher, &mut shared].map(|child| (child.stdin.take(), child.stdout.take()));
        let session = Session {
            launcher,
            process: Some(shared),
        };
        assert!(session.process.is_some());
        drop(session);
        for (input, output) in pipes {
            let mut input = input.unwrap_or_else(|| panic!("child stdin"));
            writeln!(input, "synthetic-document-still-open")
                .unwrap_or_else(|error| panic!("child was killed: {error}"));
            drop(input);
            let mut reply = String::new();
            output
                .unwrap_or_else(|| panic!("child stdout"))
                .read_to_string(&mut reply)
                .unwrap_or_else(|error| panic!("child reply: {error}"));
            assert!(reply.contains("preserved: synthetic-document-still-open"));
        }
    }

    #[test]
    #[ignore = "subprocess fixture for the lifecycle regression test"]
    fn notepad_process_child() {
        if std::env::var("IBIS_NOTEPAD_TEST_CHILD").as_deref() != Ok("1") {
            return;
        }
        let mut message = String::new();
        std::io::stdin()
            .read_line(&mut message)
            .unwrap_or_else(|error| panic!("child input: {error}"));
        println!("preserved: {}", message.trim());
    }
}
