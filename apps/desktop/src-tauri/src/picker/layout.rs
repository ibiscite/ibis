//! macOS registers a shortcut's physical key, so a letter is first moved to the
//! key that types it on the active layout: ⌘⇧Y stays the key labelled Y on
//! QWERTZ, AZERTY, Dvorak and Colemak (#196). Other keys pass through.

use tauri_plugin_global_shortcut::{Code, Shortcut};

/// Each letter's US key position: the macOS virtual key global-hotkey registers for it.
#[rustfmt::skip]
const LETTERS: [(Code, u16, char); 26] = [
    (Code::KeyA, 0x00, 'a'), (Code::KeyS, 0x01, 's'), (Code::KeyD, 0x02, 'd'), (Code::KeyF, 0x03, 'f'),
    (Code::KeyH, 0x04, 'h'), (Code::KeyG, 0x05, 'g'), (Code::KeyZ, 0x06, 'z'), (Code::KeyX, 0x07, 'x'),
    (Code::KeyC, 0x08, 'c'), (Code::KeyV, 0x09, 'v'), (Code::KeyB, 0x0B, 'b'), (Code::KeyQ, 0x0C, 'q'),
    (Code::KeyW, 0x0D, 'w'), (Code::KeyE, 0x0E, 'e'), (Code::KeyR, 0x0F, 'r'), (Code::KeyY, 0x10, 'y'),
    (Code::KeyT, 0x11, 't'), (Code::KeyO, 0x1F, 'o'), (Code::KeyU, 0x20, 'u'), (Code::KeyI, 0x22, 'i'),
    (Code::KeyP, 0x23, 'p'), (Code::KeyL, 0x25, 'l'), (Code::KeyJ, 0x26, 'j'), (Code::KeyK, 0x28, 'k'),
    (Code::KeyN, 0x2D, 'n'), (Code::KeyM, 0x2E, 'm'),
];

/// The shortcut whose key types `shortcut`'s letter, given what each US key
/// position types now; `None` when no letter key types it.
pub(super) fn translate(shortcut: Shortcut, typed: impl Fn(u16) -> Option<char>) -> Option<Shortcut> {
    let Some(&(_, own, letter)) = LETTERS.iter().find(|(code, ..)| *code == shortcut.key) else {
        return Some(shortcut);
    };
    if typed(own) == Some(letter) {
        return Some(shortcut);
    }
    LETTERS
        .iter()
        .find(|(_, key, _)| typed(*key) == Some(letter))
        .map(|&(code, ..)| Shortcut::new(Some(shortcut.mods), code))
}

/// What the key at a US position types on the active layout, without modifiers.
/// Keyboard layout queries must run on the main thread.
#[cfg(target_os = "macos")]
pub(super) fn typed(key: u16) -> Option<char> {
    let event = objc2_core_graphics::CGEvent::new_keyboard_event(None, key, true)?;
    let text = objc2_app_kit::NSEvent::eventWithCGEvent(&event)?
        .charactersIgnoringModifiers()?
        .to_string();
    let mut chars = text.chars().flat_map(char::to_lowercase);
    match (chars.next(), chars.next()) {
        (Some(letter), None) => Some(letter),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Modifiers;

    fn layout(swaps: &'static [(char, char)]) -> impl Fn(u16) -> Option<char> {
        move |key| {
            let us = LETTERS.iter().find(|(_, k, _)| *k == key).map(|&(.., letter)| letter)?;
            Some(swaps.iter().find(|(from, _)| *from == us).map_or(us, |&(_, to)| to))
        }
    }

    const MODS: Modifiers = Modifiers::SUPER.union(Modifiers::SHIFT);

    #[test]
    fn the_us_layout_keeps_every_letter_where_it_is() {
        for &(code, ..) in &LETTERS {
            let shortcut = Shortcut::new(Some(MODS), code);
            assert_eq!(translate(shortcut, layout(&[])), Some(shortcut));
        }
    }

    #[test]
    fn a_letter_moves_to_the_key_that_types_it() {
        let qwertz = layout(&[('y', 'z'), ('z', 'y')]);
        let wanted = Shortcut::new(Some(MODS), Code::KeyY);
        assert_eq!(translate(wanted, &qwertz), Some(Shortcut::new(Some(MODS), Code::KeyZ)));
        // Dvorak types y with the US T key.
        let dvorak = layout(&[('t', 'y'), ('y', 'f'), ('f', 'u')]);
        assert_eq!(translate(wanted, dvorak), Some(Shortcut::new(Some(MODS), Code::KeyT)));
    }

    #[test]
    fn a_layout_without_the_letter_says_so_and_other_keys_pass_through() {
        let cyrillic = |_| Some('я');
        assert_eq!(translate(Shortcut::new(Some(MODS), Code::KeyY), cyrillic), None);
        let space = Shortcut::new(Some(Modifiers::ALT), Code::Space);
        assert_eq!(translate(space, cyrillic), Some(space));
        assert_eq!(translate(Shortcut::new(Some(MODS), Code::KeyY), |_| None), None);
    }
}
