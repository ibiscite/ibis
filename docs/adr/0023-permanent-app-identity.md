# 0023. Use the owned domain for the app identity and preserve channel settings

Status: accepted (2026-10-22); supersedes the settings location in 0016

## Context

The maintainer owns ibiscite.com and chose `com.ibiscite.ibis` in
[#695](https://github.com/ibiscite/ibis/issues/695). A bundle identifier affects
configuration folders, desktop integration and system permissions. Changing it
after distribution must preserve settings rather than start from defaults.
ADR 0019 keeps Stable and Nightly independent. Login-item state is held by the
operating system, not settings JSON; Apple's
[main-app service](https://developer.apple.com/documentation/servicemanagement/smappservice/mainapp)
controls the running app's registration. The reviewed migration paths can leave
the former macOS login choice unknown. The maintainer
[accepted visible one-time reapproval on 2026-10-22](https://github.com/ibiscite/ibis/issues/695#issuecomment-6345031681),
replacing automatic preservation of an unknown macOS login choice while keeping
libraries, shortcuts and other migrated settings intact.

## Decision

- Stable uses `com.ibiscite.ibis`; Nightly uses `com.ibiscite.ibis.nightly`.
  Product names and update feed addresses remain unchanged. Development and
  unknown identifiers never migrate another app's settings.
- Continue using Tauri's `app_config_dir`: Application Support on macOS,
  roaming AppData on Windows, and XDG config on Linux. Replace only the final
  identifier component. No library files or remembered paths move.
- On first use of a new identity, copy `settings.json` and its existing
  `settings.json.bak` from the matching old `io.github.ibiscite.ibis` directory
  (or its `.nightly` directory), only into an absent or empty destination.
  Copy bytes without parsing, converting or dropping unknown fields. Retain
  the old files. Do not copy temporary files or unrelated directory contents.
- An existing new primary or backup always wins, including an empty, invalid
  or newer file. Existing recovery and save-refusal rules remain unchanged.
  A missing old primary means there are no active settings to migrate.
- Stage and flush the complete copy before publishing it. An interrupted copy
  must not expose a partial new primary or lose the old recovery pair. Never
  replace an unrelated nonempty destination. On a failed migration, continue
  reading and saving the old settings and show a fixed notice; retry on a later
  launch. Do not log file contents, paths or operating-system error payloads.
- Preserve each channel's actual start-at-login choice on Windows and Linux.
  Windows retains the product-named Run entry. Linux replaces only the
  corresponding Ibis-owned autostart entry after preparing its replacement.
- On macOS, after matching legacy settings migration, show a visible one-time
  notice explaining the new app identity and how to turn on Open at Login in
  More. Leave unknown prior startup consent off. Register the new main-app
  service only after the person explicitly requests On, then report its actual
  enabled, approval-required or refused result. A copied JSON file is never
  evidence of consent.
- Keep that notice once per channel. Record acknowledgement only after
  successful display/dismissal. If delivery or safe acknowledgement persistence
  fails, keep it pending across reopening rather than suppressing it.
- On macOS publish `macos-login-reapproval-v1.pending`, containing the fixed
  `macos-open-at-login-reapproval-v1` line, with the staged settings pair.
  Flush the receipt and staging directory before publication; failure leaves
  the old settings in use. The receipt records an owed notice, never consent.
  Do not copy a receipt from the old directory or claim an existing destination.
  Notice delivery and acknowledgement remain separate prerequisites.
- Do not switch the shipped identifiers until these platform transitions and
  notice handling are implemented and tested. Installed upgrade, direct
  replacement and restart/login acceptance remain required; this amendment
  does not establish them. Existing system permissions may need approval again.

Special files are unreadable settings, not streams to open: keep regular backup
recovery and refuse writes over a special primary, backup or temporary file.
Ordinary file symlinks retain their existing behavior. Metadata preflight bounds
this failure case; it is not a lock against concurrent external path changes.

## Consequences

The identity no longer depends on the hosting organization. Stable and Nightly
keep their own settings and update feeds. Downgrading leaves the old files
available; changes made under the new identity do not flow back automatically.
Settings format, unknown-field preservation and backup recovery remain governed
by ADRs 0018, 0020 and 0022. Identity migration does not reset system privacy
permissions or enable login items from an unknown state.
