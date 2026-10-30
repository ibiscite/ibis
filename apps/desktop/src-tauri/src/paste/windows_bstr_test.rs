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

/// A real owned array, without the crate's invalid BSTR insertion indirection.
pub(super) fn bstr_array_fixture() -> uiautomation::variants::Variant {
    use std::mem::ManuallyDrop;
    use uiautomation::variants::SafeArray;
    use windows::Win32::System::Variant::{VARENUM, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_ARRAY};

    let array = SafeArray::new_vector(VT_BSTR, 1).unwrap_or_else(|_| panic!("synthetic BSTR array allocation"));
    assert_eq!(array.get_var_type().ok(), Some(VT_BSTR));
    assert_eq!(array.get_dim(), 1);
    assert_eq!(array.get_lower_bound(1).ok(), Some(0));
    assert_eq!(array.get_upper_bound(1).ok(), Some(0));
    // The verified type matches T, as get_element requires. This safe OUT
    // reader copies the SDK-initialized null slot; no PutElement is called.
    let element = array
        .get_element::<BSTR>(0)
        .unwrap_or_else(|_| panic!("synthetic BSTR array element"));
    assert_eq!(String::try_from(&element), Ok(String::new()));

    let tag = VARENUM(VT_ARRAY.0 | VT_BSTR.0);
    // SafeArray::into marks its wrapper unowned. The initialized VARIANT is
    // now the sole owner; its ordinary VariantClear drop frees the array.
    // ManuallyDrop wraps only the VARIANT metadata, never a borrowed BSTR.
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: tag,
                Anonymous: VARIANT_0_0_0 { parray: array.into() },
                ..Default::default()
            }),
        },
    }
    .into()
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
