pub(super) fn number(path: &str, name: &str) -> usize {
    path.split(['?', '&'])
        .find_map(|part| part.strip_prefix(name))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}
