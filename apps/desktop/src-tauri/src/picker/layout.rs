//! macOS registers a shortcut's physical key, so a letter is first moved to the
//! key that types it on the active layout, including its Command layer: ⌘⇧Y
//! follows Y on QWERTZ, AZERTY, Dvorak and Colemak (#196). Other keys pass through.

use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

/// Every printable key position: the macOS virtual key global-hotkey registers
/// for each code, and what it types on a US keyboard.
#[rustfmt::skip]
const POSITIONS: [(Code, u16, char); 47] = [
    (Code::KeyA, 0x00, 'a'), (Code::KeyS, 0x01, 's'), (Code::KeyD, 0x02, 'd'), (Code::KeyF, 0x03, 'f'),
    (Code::KeyH, 0x04, 'h'), (Code::KeyG, 0x05, 'g'), (Code::KeyZ, 0x06, 'z'), (Code::KeyX, 0x07, 'x'),
    (Code::KeyC, 0x08, 'c'), (Code::KeyV, 0x09, 'v'), (Code::KeyB, 0x0B, 'b'), (Code::KeyQ, 0x0C, 'q'),
    (Code::KeyW, 0x0D, 'w'), (Code::KeyE, 0x0E, 'e'), (Code::KeyR, 0x0F, 'r'), (Code::KeyY, 0x10, 'y'),
    (Code::KeyT, 0x11, 't'), (Code::Digit1, 0x12, '1'), (Code::Digit2, 0x13, '2'), (Code::Digit3, 0x14, '3'),
    (Code::Digit4, 0x15, '4'), (Code::Digit6, 0x16, '6'), (Code::Digit5, 0x17, '5'), (Code::Equal, 0x18, '='),
    (Code::Digit9, 0x19, '9'), (Code::Digit7, 0x1A, '7'), (Code::Minus, 0x1B, '-'), (Code::Digit8, 0x1C, '8'),
    (Code::Digit0, 0x1D, '0'), (Code::BracketRight, 0x1E, ']'), (Code::KeyO, 0x1F, 'o'), (Code::KeyU, 0x20, 'u'),
    (Code::BracketLeft, 0x21, '['), (Code::KeyI, 0x22, 'i'), (Code::KeyP, 0x23, 'p'), (Code::KeyL, 0x25, 'l'),
    (Code::KeyJ, 0x26, 'j'), (Code::Quote, 0x27, '\''), (Code::KeyK, 0x28, 'k'), (Code::Semicolon, 0x29, ';'),
    (Code::Backslash, 0x2A, '\\'), (Code::Comma, 0x2B, ','), (Code::Slash, 0x2C, '/'), (Code::KeyN, 0x2D, 'n'),
    (Code::KeyM, 0x2E, 'm'), (Code::Period, 0x2F, '.'), (Code::Backquote, 0x32, '`'),
];

/// The shortcut whose key types `shortcut`'s letter, given what each key
/// position types now; `None` when no key types it. Only letters move. Query
/// Command when configured, excluding modifiers that alter text or control codes.
pub(super) fn translate(shortcut: Shortcut, typed: impl Fn(u16, Modifiers) -> Option<char>) -> Option<Shortcut> {
    let Some(&(_, own, letter)) = POSITIONS
        .iter()
        .find(|(code, _, us)| *code == shortcut.key && us.is_ascii_lowercase())
    else {
        return Some(shortcut);
    };
    let modifiers = shortcut.mods.intersection(Modifiers::SUPER);
    if typed(own, modifiers) == Some(letter) {
        return Some(shortcut);
    }
    POSITIONS
        .iter()
        .find(|(_, key, _)| typed(*key, modifiers) == Some(letter))
        .map(|&(code, ..)| Shortcut::new(Some(shortcut.mods), code))
}

/// The macOS virtual key that types `code`'s letter with Command on the active
/// layout, for posting ⌘V (#27): V is the period key on Dvorak.
pub(crate) fn command_key(code: Code, typed: impl Fn(u16, Modifiers) -> Option<char>) -> Option<u16> {
    let moved = translate(Shortcut::new(Some(Modifiers::SUPER), code), typed)?;
    position(moved.key)
}

fn position(code: Code) -> Option<u16> {
    POSITIONS.iter().find(|(c, ..)| *c == code).map(|&(_, key, _)| key)
}

/// Set a private key event’s Command layer before AppKit translates it.
/// characters() keeps Command (Dvorak–QWERTY ⌘); ignoring modifiers loses it.
/// These APIs predate macOS 10.13, unlike charactersByApplyingModifiers (10.15).
/// Layout queries need the main thread; these events are never posted.
#[cfg(target_os = "macos")]
pub(crate) fn typing() -> impl Fn(u16, Modifiers) -> Option<char> {
    use objc2_app_kit::NSEvent;
    use objc2_core_graphics::{CGEvent, CGEventFlags, CGEventSource, CGEventSourceStateID};
    let source = CGEventSource::new(CGEventSourceStateID::Private);
    move |key, modifiers: Modifiers| {
        let event = CGEvent::new_keyboard_event(source.as_deref(), key, true)?;
        let flags = if modifiers.contains(Modifiers::SUPER) {
            CGEventFlags::MaskCommand
        } else {
            CGEventFlags::empty()
        };
        CGEvent::set_flags(Some(&event), flags);
        let text = NSEvent::eventWithCGEvent(&event)?.characters()?.to_string();
        let mut chars = text.chars().flat_map(char::to_lowercase);
        match (chars.next(), chars.next()) {
            (Some(letter), None) => Some(letter),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout as the US characters whose keys type something else.
    fn layout(changes: &'static [(char, char)]) -> impl Fn(u16, Modifiers) -> Option<char> {
        move |key, _modifiers| {
            let us = POSITIONS.iter().find(|(_, k, _)| *k == key).map(|&(.., us)| us)?;
            Some(changes.iter().find(|(from, _)| *from == us).map_or(us, |&(_, to)| to))
        }
    }

    const MODS: Modifiers = Modifiers::SUPER.union(Modifiers::SHIFT);
    const QWERTZ: &[(char, char)] = &[('y', 'z'), ('z', 'y')];
    const AZERTY: &[(char, char)] = &[('a', 'q'), ('q', 'a'), ('z', 'w'), ('w', 'z'), (';', 'm'), ('m', ',')];
    const TURKISH_Q: &[(char, char)] = &[('i', 'ı'), ('\'', 'i')];
    #[rustfmt::skip]
    const COLEMAK: &[(char, char)] = &[
        ('e', 'f'), ('r', 'p'), ('t', 'g'), ('y', 'j'), ('u', 'l'), ('i', 'u'), ('o', 'y'), ('p', ';'),
        ('s', 'r'), ('d', 's'), ('f', 't'), ('g', 'd'), ('j', 'n'), ('k', 'e'), ('l', 'i'), (';', 'o'), ('n', 'k'),
    ];
    #[rustfmt::skip]
    const DVORAK: &[(char, char)] = &[
        ('q', '\''), ('w', ','), ('e', '.'), ('r', 'p'), ('t', 'y'), ('y', 'f'), ('u', 'g'), ('i', 'c'), ('o', 'r'),
        ('p', 'l'), ('[', '/'), (']', '='), ('s', 'o'), ('d', 'e'), ('f', 'u'), ('g', 'i'), ('h', 'd'), ('j', 'h'),
        ('k', 't'), ('l', 'n'), (';', 's'), ('\'', '-'), ('z', ';'), ('x', 'q'), ('c', 'j'), ('v', 'k'), ('b', 'x'),
        ('n', 'b'), (',', 'w'), ('.', 'v'), ('/', 'z'), ('-', '['), ('=', ']'),
    ];

    fn key(code: Code) -> Shortcut {
        Shortcut::new(Some(MODS), code)
    }

    #[test]
    fn the_us_layout_keeps_every_key_where_it_is() {
        for &(code, ..) in &POSITIONS {
            assert_eq!(translate(key(code), layout(&[])), Some(key(code)));
        }
    }

    #[test]
    fn a_letter_moves_to_the_key_that_types_it_on_whole_layouts() {
        for (changes, letter, place) in [
            (QWERTZ, Code::KeyY, Code::KeyZ),
            (AZERTY, Code::KeyM, Code::Semicolon),
            (AZERTY, Code::KeyA, Code::KeyQ),
            (DVORAK, Code::KeyY, Code::KeyT),
            (COLEMAK, Code::KeyY, Code::KeyO),
            (DVORAK, Code::KeyS, Code::Semicolon),
            (DVORAK, Code::KeyW, Code::Comma),
            (DVORAK, Code::KeyV, Code::Period),
            (DVORAK, Code::KeyZ, Code::Slash),
            // Turkish Q types dotted i with the US quote key and dotless ı with the US I key.
            (TURKISH_Q, Code::KeyI, Code::Quote),
        ] {
            assert_eq!(translate(key(letter), layout(changes)), Some(key(place)), "{letter:?}");
        }
        // Every letter has a key on each of these layouts, so none falls back.
        for changes in [QWERTZ, AZERTY, DVORAK, COLEMAK, TURKISH_Q] {
            let typed = layout(changes);
            for &(code, ..) in POSITIONS.iter().filter(|(.., us)| us.is_ascii_lowercase()) {
                let moved = translate(key(code), &typed).unwrap_or_else(|| panic!("{code:?} has no key"));
                let place = POSITIONS.iter().find(|(c, ..)| *c == moved.key).map(|&(_, k, _)| k);
                let letter = POSITIONS.iter().find(|(c, ..)| *c == code).map(|&(.., us)| us);
                assert_eq!(place.and_then(|key| typed(key, MODS)), letter);
            }
        }
    }

    #[test]
    fn a_hybrid_layout_uses_its_command_layer_only_for_command_shortcuts() {
        let hybrid = |key, modifiers: Modifiers| {
            assert!(
                modifiers.difference(Modifiers::SUPER).is_empty(),
                "text/control modifiers must be excluded"
            );
            layout(if modifiers.contains(Modifiers::SUPER) {
                &[]
            } else {
                DVORAK
            })(key, modifiers)
        };
        for modifiers in [
            MODS,
            Modifiers::SUPER | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT,
        ] {
            let shortcut = Shortcut::new(Some(modifiers), Code::KeyY);
            assert_eq!(
                translate(shortcut, hybrid),
                Some(shortcut),
                "Command uses QWERTY's Y, never T"
            );
        }
        for modifiers in [
            Modifiers::empty(),
            Modifiers::SHIFT,
            Modifiers::CONTROL | Modifiers::ALT,
        ] {
            let shortcut = Shortcut::new(Some(modifiers), Code::KeyY);
            assert_eq!(
                translate(shortcut, hybrid),
                Some(Shortcut::new(Some(modifiers), Code::KeyT))
            );
        }
    }

    #[test]
    fn paste_uses_the_key_that_types_v_with_command() {
        // US, German (QWERTZ), AZERTY and Dvorak.
        for (changes, place) in [(&[][..], 0x09), (QWERTZ, 0x09), (AZERTY, 0x09), (DVORAK, 0x2F)] {
            assert_eq!(command_key(Code::KeyV, layout(changes)), Some(place));
        }
        assert_eq!(command_key(Code::KeyV, |_, _| Some('я')), None);
    }

    #[test]
    fn a_layout_without_the_letter_says_so_and_other_keys_pass_through() {
        let cyrillic = |_, _| Some('я');
        assert_eq!(translate(key(Code::KeyY), cyrillic), None);
        assert_eq!(translate(key(Code::KeyY), |_, _| None), None);
        for other in [Code::Space, Code::Digit1, Code::Semicolon] {
            assert_eq!(translate(key(other), cyrillic), Some(key(other)));
        }
    }
}
