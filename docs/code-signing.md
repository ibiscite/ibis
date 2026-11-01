# Code signing policy

Ibis is free, open-source software under the AGPL-3.0-or-later licence. Its
source, reviews and build workflows are public in the
[Ibis repository](https://github.com/ibiscite/ibis).

## Current status

Windows installers and macOS apps are not yet signed with a certificate the
operating system trusts, so it may warn on first install.

macOS builds can use a long-lived self-signed identity to keep the same
signing requirement across updates ([#1242](https://github.com/ibiscite/ibis/issues/1242)).
This is not Apple Developer ID signing or notarization, and does not remove
Gatekeeper's first-install warning. Permission survival still needs the
hands-on update check below; equal signing requirements alone do not prove it.

The `sign-macos` job in `.github/workflows/preview.yml` runs on a fresh runner,
without dependency installation or executing the app. It verifies source-bound
package provenance before importing the identity, signs with the pinned
certificate, checks the signature and designated requirement, then removes its
temporary keychain. It re-attests the changed packages before installed-package
checks and updater signing. Ibis and Ibis Nightly retain different identifiers.
Without the repository pin, signing is disabled: packages stay ad-hoc signed
and the job does not create or access the signing environment. Once the pin is
set, missing secrets fail instead of silently falling back.

Ibis's updater signatures are separate: the app accepts an update only when
its signature matches the Ibis update key built into the app and authenticates
the advertised version within the installed channel
([decision 0019](decisions.md#0019-updates)). An updater signature does not establish
a Windows publisher or remove the first-install warning.

## Roles and review

The current single maintainer is [Diyanko](https://github.com/diyanko), responsible
for the committer and reviewer roles and the planned signing approver role.
Contributors and delegated agents work under the rules below; a delegated merge
does not authorize signing or publication.

- **Contributors** propose changes through public pull requests and disclose
  AI assistance in commits and pull requests ([AGENTS.md](../AGENTS.md)).
- **Reviewers** check the exact proposed commit. All required checks must pass,
  changes in a risky area ([decision 0027](decisions.md#0027-process-until-10-with-0011-0015-and-0017)) need an
  independent review with every P1 and P2 finding fixed, and other changes need
  the author's recorded adversarial check
  ([contribution guide](../CONTRIBUTING.md#pull-requests)). Authorized agents may merge
  changes under those rules; AI assistance does not bypass them.
- **The maintainer** has final responsibility for the project and explicitly
  authorizes releases, tags and publication. Agents cannot grant themselves release
  or signing approvals.

## Privacy

[PRIVACY.md](../PRIVACY.md) describes local library access, optional update checks
and installer network use. Ibis does not track users or upload their libraries.

## Planned signing

SignPath Foundation declined Ibis's application for free Windows signing in
October 2026 ([#299](https://github.com/ibiscite/ibis/issues/299)); Ibis may
apply again once it is established. Windows installers remain unsigned.
1.0 ships without paid signing (maintainer, 2026-11-01). Apple Developer ID,
notarization and Windows signing wait for money or a free programme
([ROADMAP.md](../ROADMAP.md)); a paid service or account needs the maintainer's
decision. The [README](../README.md) gives first-launch steps.

Whichever service signs, it signs installers built by the public GitHub Actions
release workflow after the checks and release approval in
[RELEASING.md](../RELEASING.md), and the operating-system signature is applied
before the Ibis updater signature, so the updater verifies the exact installer
that will run. Self-signed macOS identity also applies to Nightly when configured.

## One-time macOS identity preparation

The maintainer chose a free self-signed identity in
[#1242](https://github.com/ibiscite/ibis/issues/1242#issuecomment-6431327885).
Prepare it only after the proposed scripts have independent final-head review.
On the maintainer's Mac, from that reviewed checkout:

```sh
bash scripts/macos-signing-setup.sh /path/to/new-private-backup-directory
```

This is **local preparation only**: a new directory, readable by its owner,
holds an encrypted PKCS#12 identity, its password, the public certificate and
its SHA-1 pin. The system LibreSSL creates a 20-year RSA code-signing certificate.
It calls no GitHub APIs, imports no identity and changes no system trust.
Never put this directory in the repository or share its password/PKCS#12 in
logs or issues. Back it up in private offline storage before configuring GitHub.
If preparation fails, retain the directory and inspect it; do not overwrite an
existing identity or casually regenerate one. Losing or rotating the certificate
means users must grant paste permission again.

## GitHub configuration requiring maintainer authorization

The exact proposed effects are:

1. Create or configure the **macos-signing** environment in **ibiscite/ibis**.
   Allow only branch `main` and tags `v*`, using selected branches and tags.
   Inspect an existing environment first; preserve its reviewers and other
   protections, and stop for review if it already holds a signing identity.
   No bypass, reviewer deletion or update-environment change is needed.
2. In that environment, set secrets **MACOS_SIGNING_P12** (base64 of the
   prepared `.p12`) and **MACOS_SIGNING_P12_PASSWORD** (the prepared password).
3. Last, set repository variable **MACOS_SIGNING_LEAF** to `leaf.txt`'s
   lowercase 40-digit fingerprint. This enables the signing job; setting it
   before the environment and both secrets are ready makes builds fail.
   Verify both allowed-ref rules and all three names before the first run.
   Never print the secret values to confirm them. Removing the repository pin
   disables signing and returns future packages to ad-hoc signatures.

Only these two secrets and this public pin are added; no account, paid service,
tag or release is created. The environment permits automatic Nightly signing
from main and signing of existing release-tag builds; the stable publishing
job still requires its separate maintainer approval. The identity is shared
between the two macOS channels, whose bundle identifiers remain separate.
An administrator with access to this identity can sign either identifier;
this certificate is not an additional stable-versus-Nightly trust boundary.

## Qualification before claiming permission survival

After approved configuration, use the existing **Preview builds** manual run on
main with **Publish verified packages** unchecked. Record its exact source SHA,
run URL, package checksums and `package-proof-<target>` artifacts. Each proof
contains the source, checksum list and provenance bundle; signed macOS proofs
also retain the pre-signing bundle and add signing provenance and the public
pin. These small artifacts are inspectable evidence, not an independent download
and hash of the package bytes. Intel macOS is build-only on hosted Apple Silicon
runners; Windows/macOS installed startup is not the complete native smoke matrix.
[#227](https://github.com/ibiscite/ibis/issues/227) records remaining qualification.

Check the DMG-installed app and update archive have the same expected identifier,
valid signature, pin and designated requirement using `scripts/macos-sign.sh
requirement <app> <identifier> <leaf>`. Confirm a second build has the same
requirement. Keep all publishing gates and channel update signatures intact.

Finally, using the same installed channel and two reviewed signed builds N and
N+1, grant paste permission on N, perform an actual update through Ibis's updater,
relaunch, and paste into an owned blank writing document without another grant.
Record OS, versions/source SHAs and passed/failed/not tested for each step.
A fresh install, equal requirements, package swap or passing updater unit test
cannot stand in for this check. A dry run creates no update feed, so the actual
update acceptance waits for two appropriately qualified Nightly builds; it does
not authorize a stable publication. [#79](https://github.com/ibiscite/ibis/issues/79)
keeps personal hardware/editor acceptance separate.
