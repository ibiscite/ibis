//! Swift renders these short, catalogue-owned labels before the daemon opens.
use crate::catalogue::Language;
use std::collections::BTreeMap;

pub fn shell_words(language: Language) -> BTreeMap<&'static str, String> {
    let fixed = [
        ("app", "picker-native-app"),
        ("panel", "picker-native-panel"),
        ("open", "picker-native-open"),
        ("quit", "picker-native-quit"),
        ("allowPaste", "picker-native-allow-paste"),
        ("openLibrary", "picker-native-open-library"),
        ("missingShortcut", "picker-native-shortcut-unavailable"),
        ("search", "picker-search-label"),
        ("format", "picker-native-format"),
        ("citation", "picker-native-citation"),
        ("shorthandExample", "picker-native-shorthand-example"),
        ("best", "picker-best-matches"),
        ("closest", "picker-closest-matches"),
        ("pasted", "picker-pasted-list"),
        ("references", "picker-references"),
        ("resultHint", "picker-native-results-hint"),
        ("emptyLibrary", "picker-type-to-find"),
        ("emptyLibraryHint", "picker-native-search-hint"),
        ("noMatches", "picker-no-matches"),
        ("noMatchesHint", "picker-no-matches-hint"),
        ("clear", "picker-clear-search"),
        ("openZotero", "picker-open-zotero"),
        ("choose", "picker-choose-library-menu"),
        ("chooseTitle", "picker-native-choose-title"),
        ("chooseHint", "picker-native-choose-hint"),
        ("missingHint", "picker-native-missing-hint"),
        ("opening", "library-opening-unnamed"),
        ("reload", "picker-reload-library"),
        ("library", "picker-library-region"),
        ("untitled", "picker-native-untitled"),
        ("cited", "picker-native-cited"),
        ("formatting", "picker-preview-pending"),
        ("showAs", "citation-details-form"),
        ("locatorType", "picker-native-locator-type"),
        ("locatorValue", "picker-native-locator-value"),
        ("before", "citation-details-prefix"),
        ("after", "citation-details-suffix"),
        ("copy", "key-copy"),
        ("add", "key-add-to-citation"),
        ("remove", "key-remove-from-citation"),
        ("more", "picker-more"),
        ("returnKey", "picker-native-return"),
        ("typeCitation", "picker-native-type-citation"),
        ("details", "picker-citation-details"),
        ("peek", "palette-peek"),
        ("openSource", "picker-native-open-source"),
        ("keepOrder", "picker-keep-my-order"),
        ("undo", "picker-undo"),
        ("textLarger", "picker-native-text-larger"),
        ("textSmaller", "picker-native-text-smaller"),
        ("density", "picker-native-density"),
        ("compact", "general-density-compact"),
        ("comfortable", "general-density-comfortable"),
        ("appearance", "look-appearance"),
        ("system", "look-system"),
        ("light", "look-light"),
        ("dark", "look-dark"),
        ("copied", "picker-native-copied"),
        ("copiedPermission", "picker-native-copied-permission"),
        ("copyFailed", "picker-native-copy-failed"),
        ("disconnected", "picker-native-engine-disconnected"),
        ("invalidFrame", "picker-native-engine-invalid"),
        ("wrongPeer", "picker-native-engine-peer"),
        ("newerEngine", "picker-native-engine-newer"),
        ("noEngine", "picker-native-engine-launch"),
        ("busy", "picker-native-engine-busy"),
        ("changed", "picker-native-engine-changed"),
        ("requestFailed", "picker-native-request-failed"),
    ];
    const VALUE: &str = "{IBIS_VALUE}";
    let with_args: [(&str, &str, &str, &str); 7] = [
        ("citedHere", "picker-recent-heading", "state", "cited"),
        ("recentlyAdded", "picker-recent-heading", "state", "added"),
        ("paste", "picker-bar-paste", "app", VALUE),
        ("findLibrary", "picker-native-find-library", "name", VALUE),
        ("token", "picker-native-token", "citation", VALUE),
        ("removeToken", "picker-remove-named", "name", VALUE),
        ("formatValue", "picker-native-format-value", "format", VALUE),
    ];
    fixed
        .into_iter()
        .map(|(key, id)| (key, super::word(language, id, &[])))
        .chain(
            with_args
                .into_iter()
                .map(|(key, id, name, value)| (key, super::word(language, id, &[(name, value)]))),
        )
        .collect()
}
