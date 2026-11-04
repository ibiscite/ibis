# Recorded editor requests

Requests that real Cite as You Write clients send, replayed against the editor
endpoint's parser by `crates/ibis-engine/tests/cayw_clients.rs`. The token is
invented and the port is written `{port}`.

`vscode-zotero-citation-picker.json` holds what Node's HTTP client sends when it
is configured as Citation Picker for Zotero 0.2.1 configures it (`src/client.ts`,
commit `fe4d6bbfc6` of mblode/vscode-zotero): one request per format the
extension offers. `record.mjs` recorded it on `127.0.0.1` and can record it again:

```sh
node fixtures/cayw/record.mjs > fixtures/cayw/vscode-zotero-citation-picker.json
```

A replay proves Ibis reads the request a client sends; it does not prove that
the client inserts the answer. That needs the client itself, against Ibis.
