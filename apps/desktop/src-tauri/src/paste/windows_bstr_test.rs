//! Exact owned BSTR access for the Windows qualification test only (ADR 0029).
#![cfg(test)]

use crate::paste::notepad_test::{Failure, Stage};
use windows::{
    Win32::{
        Foundation::SysStringByteLen,
        System::Variant::{VARIANT, VT_BSTR},
    },
    core::BSTR,
};

/// Only owned UTF8 text leaves this borrowing boundary.
pub(super) fn owned_text(value: &VARIANT) -> Result<String, Failure> {
    if value.vt() != VT_BSTR {
        return Err(Failure::new(Stage::Read));
    }
    // SAFETY: The exact tag above excludes arrays and by-reference variants, so
    // bstrVal is the active owned member. The caller retains this valid VARIANT
    // immutably for this whole call on its read worker. This borrow neither
    // moves nor frees the ManuallyDrop<BSTR>, and never escapes its owner.
    let text: &BSTR = unsafe { &value.Anonymous.Anonymous.Anonymous.bstrVal };
    // SAFETY: text is that same live, immutable, SDK-owned BSTR (possibly null).
    // The SDK only reads its length prefix; no pointer or ownership changes.
    let bytes = unsafe { SysStringByteLen(text) };
    let units = text.len();
    if units > 4096 || bytes as usize != units * 2 {
        return Err(Failure::new(Stage::Read));
    }
    String::try_from(text).map_err(|_| Failure::new(Stage::Read))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        Win32::Foundation::{SysAllocStringByteLen, SysStringByteLen},
        core::BSTR,
    };

    #[test]
    fn exact_boundaries_null_replacement_character_and_repeated_ownership() {
        for text in [
            String::new(),
            "\u{fffd}".into(),
            "a\0b".into(),
            "x".repeat(4096),
            "🪶".repeat(2048),
        ] {
            let value = VARIANT::from(BSTR::from(text.as_str()));
            for _ in 0..8 {
                assert_eq!(owned_text(&value), Ok(text.clone()));
            }
        }
        let null = VARIANT::from(BSTR::new());
        assert_eq!(owned_text(&null), Ok(String::new()));
        for text in ["x".repeat(4097), format!("{}x", "🪶".repeat(2048))] {
            let value = VARIANT::from(BSTR::from(text.as_str()));
            assert!(owned_text(&value).is_err());
        }
    }

    #[test]
    fn sdk_owned_odd_byte_bstr_is_verified_and_refused_before_decoding() {
        let bytes = [b'a', 0, b'b'];
        // SAFETY: The three-byte initialized slice remains live for the whole
        // call and fits u32. The SDK allocates/copies its own BSTR; the returned
        // Rust BSTR owns that allocation and will free it exactly once.
        let value = unsafe { SysAllocStringByteLen(Some(&bytes)) };
        // SAFETY: This is the same live SDK-owned BSTR, not yet moved or freed.
        // SysStringByteLen reads its allocation's byte count without mutation.
        let byte_count = unsafe { SysStringByteLen(&value) };
        assert_eq!(byte_count, 3, "the fixture must really contain an odd byte count");
        let value = VARIANT::from(value);
        for _ in 0..8 {
            assert!(owned_text(&value).is_err());
        }
    }
}
