# Name and logo

The name "Ibis" and its logo identify the official citation tool and are owned
by Diyanko Bhowmik. The code is free software.

## How to recognise official Ibis

- GitHub organisation: [ibiscite](https://github.com/ibiscite).
- Website: [ibiscite.org](https://ibiscite.org).
- Homebrew tap: `ibiscite/tap`.
- App identifier: `com.ibiscite.ibis`.

[GitHub release downloads](https://github.com/ibiscite/ibis/releases) include
build provenance. Verify a downloaded file with:

```sh
gh attestation verify <file> --repo ibiscite/ibis
```

Updates are signed; the app checks them against its built-in public keys.
The stable public key lives at `plugins.updater.pubkey` in
[tauri.conf.json](apps/desktop/src-tauri/tauri.conf.json). Stable and Nightly
have [separate update keys](RELEASING.md#update-keys).

## Using the name

You may, without asking:

- Use the name to refer to the project, e.g. "works with Ibis" or "a guide to Ibis".
- Redistribute unmodified official releases under the same name.
- Package Ibis for a Linux distribution or package manager with only packaging changes needed for that platform.

For extensions, "X for Ibis" is fine; "Ibis X" suggests an official product.
Modified builds with different functionality use another name and icon.
Credit like "based on Ibis" is welcome.

Present your own products under their own names. Endorsement and official
status come from the Ibis project. In package listings, "Ibis (ibiscite)"
helps people recognise the project.

For name and logo questions, contact
[diyanko@ibiscite.org](mailto:diyanko@ibiscite.org).
