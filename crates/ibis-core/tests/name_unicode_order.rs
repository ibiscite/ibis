//! Canonically equivalent Korean names keep the same reading order.

use ibis_core::model::Name;
use unicode_normalization::UnicodeNormalization;

#[test]
fn decomposed_korean_names_keep_family_first_without_rewriting_name_text() {
    for (family, given) in [("김", "민"), ("김", "민"), ("김", "민"), ("김", "민")] {
        let name = Name {
            family: Some(family.into()),
            given: Some(given.into()),
            ..Name::default()
        };
        let displayed = name.display();
        assert_eq!(displayed, format!("{family}{given}"));
        assert_eq!(displayed.nfc().collect::<String>(), "김민");
    }
}

#[test]
fn korean_family_names_with_latin_given_names_keep_existing_reading_order() {
    for family in ["김", "김"] {
        let name = Name {
            family: Some(family.into()),
            given: Some("Alex".into()),
            ..Name::default()
        };
        assert_eq!(name.display(), format!("Alex {family}"));
    }
}
