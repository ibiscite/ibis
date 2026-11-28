# 0046 Official names and listings

- **Official package names.** Use `ibiscite` on crates.io and PyPI, and the
  `@ibiscite` namespace on npm. Each package is a real, useful MIT client of the
  engine contract ([decision 0039](0039-extending-ibis-from-outside.md)), never
  an empty placeholder.
- **Official listings.** Flathub `com.ibiscite.ibis` (verified domain), winget,
  the MCP Registry under the `ibiscite` namespace, the Homebrew tap and Nix all
  come before 1.0.
- **Microsoft Store (2026-11-28).** The Windows app is also listed in the
  Microsoft Store, as an MSIX package of the installer's files that the Store
  signs: free for an individual developer, and Store installs carry no
  SmartScreen warning. The Store copy updates through the Store and starts at
  login through its package's startup task; write virtualization is off, so it
  reads and writes the same files as the installer. The listing waits for the
  owner's Partner Center account ([RELEASING.md](../../RELEASING.md#the-microsoft-store)).
- **Code signing.** Signing stays a 1.0 question under
  [decision 0032](0032-release-gates.md). Meanwhile, downloads are verifiable
  with [build provenance](../../TRADEMARKS.md#how-to-recognise-official-ibis).
- **Name and logo.** The owner is Diyanko Bhowmik; the contact for name and logo
  questions is [diyanko@ibiscite.org](mailto:diyanko@ibiscite.org), as in
  [TRADEMARKS.md](../../TRADEMARKS.md).
