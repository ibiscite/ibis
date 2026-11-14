use crate::{SourceWarning, WarningCode, diagnostic_label};
use ibis_core::source::IdentityClaims;
use std::collections::BTreeMap;

/// Extract all claims before payload validation; only unambiguous candidates
/// can leave this collector. Rejected payloads still own their claims.
pub(super) struct Candidates<T> {
    claims: IdentityClaims<String, Option<usize>>,
    candidates: Vec<(Vec<String>, T)>,
}

impl<T> Default for Candidates<T> {
    fn default() -> Self {
        Self {
            claims: IdentityClaims::default(),
            candidates: Vec::new(),
        }
    }
}

impl<T> Candidates<T> {
    pub(super) fn collect<R>(
        rows: impl IntoIterator<Item = R>,
        mut extract: impl FnMut(R) -> (Vec<(String, Option<usize>)>, T),
    ) -> Self {
        let mut result = Self::default();
        for row in rows {
            result.push(row, &mut extract);
        }
        result
    }

    pub(super) fn push<R>(&mut self, row: R, extract: impl FnOnce(R) -> (Vec<(String, Option<usize>)>, T)) {
        let (identities, candidate) = extract(row);
        let keys: Vec<_> = identities
            .into_iter()
            .map(|(key, line)| {
                self.claims.claim(key.clone(), line);
                key
            })
            .collect();
        if keys.iter().all(|key| self.claims.is_unique(key)) {
            self.candidates.push((keys, candidate));
        }
    }

    pub(super) fn finish(
        self,
        keys: &mut BTreeMap<String, Vec<SourceWarning>>,
        warnings: &mut Vec<SourceWarning>,
    ) -> Vec<T> {
        let mut duplicates: Vec<_> = self
            .claims
            .duplicates()
            .map(|(key, line)| {
                let key = key.strip_prefix("record:").unwrap_or(key);
                let warning = SourceWarning {
                    key: Some(diagnostic_label(key)),
                    ..SourceWarning::at(WarningCode::DuplicateKey, *line)
                };
                keys.entry(key.to_owned()).or_default().push(warning.clone());
                warning
            })
            .collect();
        duplicates.sort_by(|a, b| (&a.line, &a.key).cmp(&(&b.line, &b.key)));
        warnings.extend(duplicates);
        self.candidates
            .into_iter()
            .filter(|(keys, _)| keys.iter().all(|key| self.claims.is_unique(key)))
            .map(|(_, candidate)| candidate)
            .collect()
    }
}
