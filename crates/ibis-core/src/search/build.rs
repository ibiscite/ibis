//! Building an index without retaining obsolete background work.

use super::*;

impl OwnedIndex {
    pub fn new(references: Vec<Reference>) -> Self {
        // The stop predicate never returns true.
        Self::new_until(references, &|| false).unwrap_or_default()
    }

    /// Build while the request remains current; cancellation publishes no index.
    pub fn new_until(references: Vec<Reference>, stop: &dyn Fn() -> bool) -> Option<Self> {
        let mut fields = Vec::new();
        let mut identifiers = Vec::new();
        let mut source_counts = BTreeMap::new();
        for reference in &references {
            if stop() {
                return None;
            }
            fields.push(prepare(reference));
            identifiers.push(identifier::of_reference(reference));
            *source_counts.entry(reference.id.source.clone()).or_default() += 1;
        }
        if stop() {
            return None;
        }
        Some(Self {
            references,
            fields,
            identifiers,
            source_counts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn an_obsolete_build_stops_before_the_next_reference() {
        let source = (0..10)
            .map(|i| format!("@misc{{synthetic{i},title={{Synthetic}}}}\n"))
            .collect::<String>();
        let items = crate::bibtex::load_str("synthetic", &source).references;
        let calls = Cell::new(0);
        assert!(
            OwnedIndex::new_until(items, &|| {
                calls.set(calls.get() + 1);
                calls.get() == 3
            })
            .is_none()
        );
        assert_eq!(calls.get(), 3);
    }
}
