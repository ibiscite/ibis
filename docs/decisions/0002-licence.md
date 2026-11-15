# 0002 Licence

The repository is AGPL-3.0-or-later. Contributions use the Developer Certificate
of Origin (`git commit -s`), with no contributor licence agreement or planned
relicensing. The published engine contract and thin clients are MIT
([0039](0039-extending-ibis-from-outside.md)).

CSL styles and locales ship unmodified under their own licences, credited to
citationstyles.org. `cargo deny` checks dependency licences. The NCSA exception
for `libfuzzer-sys` exactly `0.4.13` applies only to the test-only `ibis-fuzz`
crate; it must never enter shipped artifacts.
