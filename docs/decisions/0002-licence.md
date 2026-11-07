# 0002 Licence

AGPL-3.0-or-later for the whole repository. Contributions come under the
Developer Certificate of Origin (`git commit -s`); no contributor licence
agreement. CSL styles and locales ship unmodified, under their own licence,
credited to citationstyles.org. `cargo deny` keeps dependency licences
compatible. The published engine contract and thin clients are MIT
(maintainer, 2026-11-05, decision 0039).

The maintainer allows NCSA for `libfuzzer-sys` exactly `0.4.13` so the test-only
`ibis-fuzz` crate can use libFuzzer (2026-10-29). The exception applies only to
the fuzz crate; this dependency must never enter shipped artifacts.
