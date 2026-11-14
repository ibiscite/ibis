# Search from a launcher

On macOS, an installed Ibis app handles `ibis://search?q=QUERY`. It opens the
picker with the decoded query, whether Ibis is running or starting. The newest
accepted link replaces any search still waiting for the interface. Taking a
request consumes it once; closing and reopening does not replay it.

Encode only the query as UTF-8 URL data. `%20` or `+` means a space; encode a
literal plus as `%2B`, an ampersand as `%26`, and a hash as `%23`. Encoding is
decoded once. For example, this searches for `雪 & café?`:

```text
ibis://search?q=%E9%9B%AA%20%26%20caf%C3%A9%3F
```

Queries must contain non-whitespace text and at most 200 Unicode characters,
the same bound as the Mac's Search with Ibis service. Malformed escapes,
invalid UTF-8, control characters, line separators, duplicate `q`, other
options, fragments, paths and other actions are refused. A path encoded in
the query is only search text; it is never opened as a file.

A link only fills Search. It cannot copy, paste, choose a library or output,
change settings, or supply a delivery target. Enter remains the person's
action. Opening a link revokes an in-flight delivery and does not capture a
new target; an existing picker target keeps its existing delivery policy.

## Shortcuts

Create a Mac shortcut named **Search Ibis** with these actions:

1. **Ask for Input**: type Text, prompt “Search your references”.
2. **URL Encode**: encode the Provided Input.
3. **Text**: type `ibis://search?q=` and insert the URL Encoded Text variable
   immediately after the equals sign.
4. **Open URLs**: use that Text.

Run it from Shortcuts, add a keyboard shortcut in its Details, or find its
name in Spotlight. Do not encode the complete `ibis://` link. Try
`雪 & café?` to check that punctuation stays in the search.

## Alfred

In Alfred's **Features → Web Search**, choose **Add Custom Search**:

- Search URL: `ibis://search?q={query}`
- Title: `Search Ibis for {query}`
- Keyword: `ibis`

Use Alfred's query URL encoding (spaces may be encoded as `+`). Test with
`雪 & café?`, then save. Type `ibis` followed by a space and your search in
Alfred. This uses Alfred's built-in custom search; no workflow or extension
is required.

## Scope and native verification

This slice registers the scheme in the macOS bundle and consumes Tauri's
native URL-open event. Windows and Linux protocol registration, CLI URL
arguments, other link actions and launcher extensions are outside this slice.
It adds no network requests or launcher dependencies.

Focused scenarios cover decoding/refusal, the native pending slot (Tauri mock
runtime), and query handoff (jsdom). Hook cleanup covers unmount, not native
hiding. These scenarios do not exercise Launch Services, native focus,
delivery revocation or either launcher. Run from the integrated repository root:

```sh
RUSTC_WRAPPER= CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-engine --locked links::tests
RUSTC_WRAPPER= CARGO_TARGET_DIR="$PWD/target" cargo test -p ibis-desktop --locked links::tests
NODE_OPTIONS= NODE_USE_SYSTEM_CA=0 pnpm --config.verify-deps-before-run=false -C apps/desktop exec vitest run src/features/picker/useNativeRequests.test.tsx --maxWorkers=1
```

Native qualification remains pending. With the built app installed and
registered, quit it and run:

```sh
open 'ibis://search?q=%E9%9B%AA%20%26%20caf%C3%A9%3F'
```

Check cold launch, then a running app with the picker visible and hidden:
Search must focus and read `雪 & café?` exactly. Run
`open 'ibis://search?q=older'` followed quickly by
`open 'ibis://search?q=newest'`; Search must end at `newest`.
Close and reopen: the consumed link must not replay. Repeat after hiding
for over a minute. Try `ibis://search?q=one&q=two` and
`ibis://paste?q=bad`: neither may change Search. Confirm that opening a link
does not copy, paste or capture the launcher's app as a new delivery target,
including while a previous delivery is pending. Finally run both recipes
above with `雪 & café?`; their encoding and execution remain unverified.

## Handoff

Changes remain uncommitted. Verification is blocked on orchestration merging
`origin/engine-s5` forward into `v04-search-links`, preserving the staged work
and `engine-s3s4` ancestry. This sandbox cannot write the shared Git metadata;
no dependency files were copied and no reset was performed. Settings and startup
remain the dependency owner's responsibility.

Next: orchestration performs that integration, then run the three commands
above (Node 24). No integrated test pass is claimed. Launch Services, Shortcuts
and Alfred execution with invented queries remain pending, along with native
focus, close/reopen and delivery qualification. Independent read-only review
found no P1/P2 in the parser or serialized consumer; native presentation was
not exercised.
