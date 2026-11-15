//! Explicit, bounded key input and advisory spelling suggestions.
use super::Failure;
use ibis_core::{contract::*, syntax::MAX_CITATION_ITEMS};
use std::{
    io::{BufRead, Read},
    path::Path,
};

pub(super) fn read(input: &mut impl BufRead) -> Result<Vec<ibis_engine::request::KeyItem>, Failure> {
    let mut bytes = Vec::new();
    input
        .take(ibis_engine::protocol::MAX_REQUEST_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| {
            Failure::new(
                ErrorCode::BadInput,
                "the key list could not be read; give UTF-8 text with one key per line.",
            )
        })?;
    if bytes.len() > ibis_engine::protocol::MAX_REQUEST_BYTES {
        return Err(Failure::new(
            ErrorCode::ResourceLimit,
            "the key list is more than 1 MiB; send fewer keys at a time.",
        ));
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        Failure::new(
            ErrorCode::BadInput,
            "the key list must be UTF-8 text, with one key per line.",
        )
    })?;
    let mut keys = Vec::new();
    for key in text
        .lines()
        .map(str::trim)
        .filter(|key| !key.is_empty() && !key.starts_with('#'))
    {
        if keys.len() == MAX_CITATION_ITEMS {
            return Err(Failure::new(
                ErrorCode::BadInput,
                format!("one citation can hold at most {MAX_CITATION_ITEMS} keys."),
            ));
        }
        keys.push(ibis_engine::request::KeyItem::from(key));
    }
    if keys.is_empty() {
        return Err(Failure::new(
            ErrorCode::BadInput,
            "the key list is empty; give at least one citation key.",
        ));
    }
    Ok(keys)
}

pub(super) fn file(path: &Path) -> Result<Vec<ibis_engine::request::KeyItem>, Failure> {
    let path = path.to_owned();
    super::bounded(move || {
        if !std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_file()) {
            return Err(Failure::new(
                ErrorCode::SourceUnavailable,
                "the key file is missing, unreadable or not a regular file; check its path, or use - for stdin.",
            ));
        }
        let file = std::fs::File::open(&path).map_err(|_| {
            Failure::new(
                ErrorCode::SourceUnavailable,
                "the key file could not be opened; check its path and permissions.",
            )
        })?;
        if !file.metadata().is_ok_and(|metadata| metadata.is_file()) {
            return Err(Failure::new(
                ErrorCode::BadInput,
                "--keys-from needs a regular file, or - for stdin.",
            ));
        }
        read(&mut std::io::BufReader::new(file))
    })
    .unwrap_or_else(|| {
        Err(Failure::new(
            ErrorCode::SourceUnavailable,
            "the key file could not be read in time; try again.",
        ))
    })
}

/// Cap objects, keys and key lengths as well as the edit-distance band. This is
/// advice from the already open snapshot, never a second lookup or selection.
pub(super) fn suggestions(engine: &ibis_engine::Engine, key: &str) -> Vec<String> {
    if key.len() > 512 || key.chars().count() > 128 {
        return vec![];
    }
    let Ok(snapshot) = engine.snapshot() else {
        return vec![];
    };
    let target: Vec<_> = key.chars().collect();
    let max = (target.len() / 4).clamp(1, 3);
    let mut close = Vec::new();
    for item in snapshot.objects().iter().take(10_000) {
        let LibraryObject::Record(record) = &item.object else {
            continue;
        };
        for candidate in record.citation_keys.iter().take(4).map(|key| &key.value) {
            if candidate.len() > 512 || candidate.chars().count() > 128 {
                continue;
            }
            let chars: Vec<_> = candidate.chars().collect();
            if let Some(distance) = distance(&target, &chars, max) {
                close.push((distance, candidate.clone()));
            }
        }
    }
    close.sort();
    close.dedup();
    close.into_iter().take(3).map(|(_, key)| key).collect()
}

fn distance(a: &[char], b: &[char], max: usize) -> Option<usize> {
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    let mut previous: Vec<_> = (0..=b.len()).collect();
    let mut row = vec![max + 1; b.len() + 1];
    for (i, left) in a.iter().enumerate() {
        row.fill(max + 1);
        row[0] = i + 1;
        for j in (i + 1).saturating_sub(max).max(1)..=b.len().min(i + 1 + max) {
            row[j] = (previous[j] + 1)
                .min(row[j - 1] + 1)
                .min(previous[j - 1] + usize::from(*left != b[j - 1]));
        }
        if row.iter().copied().min().unwrap_or(max + 1) > max {
            return None;
        }
        std::mem::swap(&mut previous, &mut row);
    }
    (previous[b.len()] <= max).then_some(previous[b.len()])
}
