# Code signing policy

Ibis is free, open-source software under the AGPL-3.0-or-later licence. Its
source, reviews and build workflows are public in the
[Ibis repository](https://github.com/ibiscite/ibis).

## Current status

Windows installers and macOS apps are not yet signed with an operating-system
code signing certificate. The operating system may warn on first install.
Ibis's updater signatures are separate: the app accepts an update only when
its signature matches the Ibis update key built into the app and authenticates
the advertised version within the installed channel
([ADR 0019](adr/0019-update-checks.md)). An updater signature does not establish
a Windows publisher or remove the first-install warning.

## Roles and review

The current single maintainer is [Diyanko](https://github.com/diyanko), responsible
for the committer and reviewer roles and the planned signing approver role.
Contributors and delegated agents work under the rules below; a delegated merge
does not authorize signing or publication.

- **Contributors** propose changes through public pull requests and disclose
  AI assistance in commits and pull requests under
  [ADR 0011](adr/0011-ai-contributions.md), as amended by
  [ADR 0015](adr/0015-autonomous-agents.md).
- **Reviewers** check the exact proposed commit. All required checks must pass,
  and product code needs an independent review with every P1 and P2 finding
  fixed. The documentation and test-only exceptions are in
  [development.md](development.md#review-and-merge). Authorized agents may merge
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
apply again once it is established. Until 1.0, installers on Windows and macOS
stay unsigned and the [README](../README.md) gives first-launch steps. Signing is part of 1.0
([ROADMAP.md](../ROADMAP.md)); choosing a signing service is the maintainer's
decision because it may cost money.

Whichever service signs, it signs installers built by the public GitHub Actions
release workflow after the checks and release approval in
[RELEASING.md](../RELEASING.md), and the operating-system signature is applied
before the Ibis updater signature, so the updater verifies the exact installer
that will run. Ibis Nightly stays unsigned.
