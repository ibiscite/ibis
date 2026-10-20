//! Explicit BibLaTeX name-particle instructions survive into CSL export.

use ibis_core::bibtex::load_str;

#[test]
fn explicit_useprefix_preserves_non_dropping_particles_in_every_name_role() {
    let loaded = load_str(
        "synthetic",
        "@book{explicit, title={Synthetic Work},
         author={family=Beethoven, given=Ludwig, prefix=van, useprefix=true},
         editor={family=Gogh, given=Vincent, prefix=van, useprefix=true},
         translator={family=Meer, given=Jan, prefix=van der, useprefix=true}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.references.len(), 1);
    let csl = loaded.references[0].to_csl_json();
    for (role, particle) in [("author", "van"), ("editor", "van"), ("translator", "van der")] {
        assert_eq!(csl[role][0]["non-dropping-particle"], particle, "{role}: {csl}");
        assert!(csl[role][0].get("dropping-particle").is_none(), "{role}: {csl}");
    }
    assert_eq!(loaded.references[0].authors[0].display(), "Ludwig van Beethoven");
}

#[test]
fn false_and_unspecified_useprefix_keep_existing_dropping_particle_behavior() {
    let loaded = load_str(
        "synthetic",
        "@book{explicit, title={Synthetic Work},
         author={family=Beethoven, given=Ludwig, prefix=van, useprefix=false}}
         @book{unspecified, title={Synthetic Work},
         author={family=Beethoven, given=Ludwig, prefix=van}}
         @book{traditional, title={Synthetic Work}, author={van Beethoven, Ludwig}}",
    );
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.references.len(), 3);
    for reference in &loaded.references {
        let csl = reference.to_csl_json();
        assert_eq!(csl["author"][0]["dropping-particle"], "van", "{csl}");
        assert!(csl["author"][0].get("non-dropping-particle").is_none(), "{csl}");
    }
}
