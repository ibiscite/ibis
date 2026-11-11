# Code signing

Operating-system signing and Ibis updater signatures serve different purposes.
An updater signature does not establish an Apple or Microsoft publisher.
[Installation help](compatibility.md#installation-blocks) covers first launch.

## Current status

Windows installers are unsigned. macOS packages can use a pinned, long-lived
self-signed identity. This is not Developer ID or notarization and does not
remove Gatekeeper warnings. The source supports both configured signing and
ad-hoc signing; it does not establish which repository secrets are set.

Preview's `sign-macos` job verifies source-bound package provenance before
importing the identity into a temporary keychain. It signs with the pinned
certificate, verifies signatures/designated requirements, removes the
keychain and re-attests changed packages. It installs no dependencies and
does not execute the app while holding the identity.

Without `MACOS_SIGNING_LEAF`, the job is disabled and packages stay ad-hoc
signed. With the pin, missing signing secrets fail the job. Stable and Nightly
use distinct bundle IDs even when they share the certificate. The certificate
is not a boundary between channels; [updater keys](../RELEASING.md#update-keys)
are separate.

## Roles and review

[AGENTS.md](../AGENTS.md#merging) defines independent review and merge rules.
The maintainer controls signing configuration and stable publication approval.
Nightly publication is automatic. Signing changes do not waive those gates.

## One-time macOS identity preparation

The reviewed helper creates an identity in a new private backup directory:

```sh
bash scripts/macos-signing-setup.sh /path/to/new-private-backup-directory
```

It creates an encrypted PKCS#12 file, password, public certificate and SHA-1
pin using a 20-year RSA certificate. It does not import the identity, change
system trust or call GitHub. Keep the directory out of the repository and
logs; back it up privately. Losing or rotating the identity can require new
paste permission grants.

## GitHub configuration requiring maintainer authorization

The workflow reads these settings:

| Setting | Location |
| --- | --- |
| `MACOS_SIGNING_P12`, `MACOS_SIGNING_P12_PASSWORD` | Secrets in the `macos-signing` environment. |
| `MACOS_SIGNING_LEAF` | Repository variable: lowercase 40-digit fingerprint. Set last to enable signing. |

That environment is for `main` and `v*` tags. Preserve existing reviewers and
protections when configuring it. Removing the pin disables signing. Stable
publishing still uses its separate `updates` approval.

## Qualification before claiming permission survival

`bash scripts/macos-sign.sh requirement APP IDENTIFIER LEAF` verifies an app's
signature, pin and requirement. Preview retains package checksums and
source/provenance proofs, including signing provenance when configured.
These checks do not prove permissions survive an update.

That claim requires two signed builds in the same installed channel: grant
permission on N, update through Ibis to N+1, relaunch and paste into an invented
blank document without another grant. Record OS, versions/source SHAs and
outcomes. Equal requirements or a fresh install cannot replace this check.
Paid signing and outstanding qualification are in [ROADMAP.md](../ROADMAP.md).
