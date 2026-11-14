# 0006 Sources

Library readers are read-only. BibTeX and BibLaTeX are parsed entry by entry;
broken entries, duplicate keys and undefined macros produce warnings with line
numbers. Every entry with a repeated key is unavailable, including as an
inheritance parent, because Ibis cannot know which one was meant. Repeated
CSL `id` values likewise refuse every affected record. A CSL `id` is not a
citation key. File libraries are parsed fresh.

Library IDs are opaque tokens keyed by a private local secret. They reveal
neither the source kind nor a guessed path. The command line may create that
identity key and save explicitly requested Ibis settings, never source data.

Running Zotero is read through its local API; closed Zotero through a private
copy of its database and journal. Readers support Zotero 7–10 and exclude
trashed items. Native, Extra and Better BibTeX citation keys must agree for key
output. Local API requests bypass proxies, send no browser `Origin` and check
`Zotero-API-Version`. Windows process, drive, attribute and ACL checks use
Win32 directly so they also work under AppLocker and Constrained Language Mode.
Native boundaries are in [0029](0029-native-calls.md).

A writing's optional `.bib` is read only to check emitted keys, never searched
or edited. It is checked when chosen, at launch and when the picker opens,
without polling. Only its filename, key states and line numbers reach the
webview. Copy Entry sends the original library entry directly from native code
to the clipboard, including its `file` and `note` fields; this explicit local
copy is described in [PRIVACY.md](../../PRIVACY.md). Sharing instead uses the
field filter in [0051](0051-references-in-and-out.md).
