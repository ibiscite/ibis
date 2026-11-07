#!/usr/bin/env bash
# Preview's "Install, start and remove" step (preview.yml), from the repository
# root: install each package in dist/ the way a user would, start the installed
# app and remove it again, so a broken installer or a package that will not
# start is never published. The app's name is read from what the package
# installs, never assumed. A failed command and the end of its output become an
# annotation, so the cause shows on the run summary.
# RUNNER_OS, RUNNER_TEMP; IDENTIFIER: the app identity the build chose.
set -Eeo pipefail
trap 'echo "::error::Failed at line $LINENO: $BASH_COMMAND"' ERR
show() {
  local out="$RUNNER_TEMP/show.log" rc=0
  "$@" > "$out" 2>&1 || rc=$?
  cat "$out"
  if [ "$rc" -ne 0 ]; then
    echo "::error::exit $rc from $*%0A$(tail -n 40 "$out" | tr -d '\r' | sed 's/%/%25/g' | awk '{printf "%s%%0A", $0}')"
  fi
  return "$rc"
}
stays_up=scripts/stays-up.py

linux() {
  sudo apt-get update
  # Only test tools here; the .deb must bring its own runtime libraries.
  sudo apt-get install -y --no-install-recommends xvfb xauth xdotool x11-utils openbox xterm python3-pyatspi xclip at-spi2-core fonts-noto-core fonts-noto-cjk
  show sudo apt-get install -y ./dist/Ibis-Linux.deb
  package=$(dpkg-deb --field dist/Ibis-Linux.deb Package)
  app=$(dpkg -L "$package" | grep -m1 '^/usr/bin/')
  echo "installed $package at $app"
  display() { xvfb-run -a -s '-screen 0 1280x800x24' dbus-run-session -- "$@"; }
  show display /usr/bin/python3 scripts/smoke-native.py --app "$app" --output screenshots/install-Linux --identifier "$IDENTIFIER" --skip-performance
  show display /usr/bin/python3 "$stays_up" "$app"
  sudo apt-get remove -y "$package"
  test ! -e "$app"
  # The AppImage needs FUSE 2, which Ubuntu 24.04 lacks by default;
  # extract-and-run starts the same bundled files without it.
  chmod +x dist/Ibis-Linux.AppImage
  APPIMAGE_EXTRACT_AND_RUN=1 show display /usr/bin/python3 "$stays_up" "$PWD/dist/Ibis-Linux.AppImage"
  # NixOS has no libgpg-error outside the package that needs it.
  (cd "$RUNNER_TEMP" && "$OLDPWD/dist/Ibis-Linux.AppImage" --appimage-extract usr/lib/libgpg-error.so.0 > /dev/null)
  test -s "$RUNNER_TEMP/squashfs-root/usr/lib/libgpg-error.so.0"
}

macos() {
  mount="$RUNNER_TEMP/ibis-dmg"
  apps="$RUNNER_TEMP/Applications"
  mkdir -p "$apps"
  hdiutil attach -nobrowse -readonly -mountpoint "$mount" dist/Ibis-macOS-Apple-silicon.dmg
  bundle=$(find "$mount" -maxdepth 1 -name '*.app' | head -n 1)
  test -n "$bundle" || { echo "::error::The disk image holds no app"; exit 1; }
  installed="$apps/$(basename "$bundle")"
  ditto "$bundle" "$installed"
  hdiutil detach "$mount"
  codesign --verify --deep --strict "$installed"
  app=$(find "$installed/Contents/MacOS" -type f -perm -u+x | head -n 1)
  echo "installed at $app"
  show python3 "$stays_up" "$app"
  rm -rf "$installed"
}

windows() {
  uninstall_keys() { pwsh -NoProfile -Command '(Get-ChildItem HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall).PSChildName' | tr -d '\r' | sort; }
  before=$(uninstall_keys)
  # The setup executable installs for the current user without prompts.
  show ./dist/Ibis-Windows-setup.exe //S
  # Find the install folder from the uninstall entry it registered.
  key=$(comm -13 <(echo "$before") <(uninstall_keys) | head -n 1)
  test -n "$key" || { echo "::error::The setup executable registered no uninstall entry"; exit 1; }
  uninstaller=$(KEY="$key" pwsh -NoProfile -Command '(Get-ItemProperty ("HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\" + $env:KEY)).UninstallString' | tr -d '"\r')
  dir=$(dirname "$(cygpath -u "$uninstaller")")
  app=$(find "$dir" -maxdepth 1 -iname '*.exe' ! -iname 'uninstall*' | head -n 1)
  test -n "$app" || { echo "::error::The setup executable installed no app in $dir"; exit 1; }
  echo "installed at $app"
  show python "$stays_up" "$app"
  "$dir/uninstall.exe" //S
  # The uninstaller copies itself and returns early; wait for removal.
  for _ in $(seq 30); do [ -e "$app" ] || break; sleep 2; done
  test ! -e "$app" || { echo "::error::The uninstaller left $app behind"; exit 1; }
  # Builds without an MSI skip the rest.
  [ -e dist/Ibis-Windows.msi ] || return 0
  # The MSI installs for all users; the runner has administrator rights.
  # The install folder is read from the installer's own log.
  msiexec() {
    MSI="$(cygpath -w "$PWD/dist/Ibis-Windows.msi")" LOG="$(cygpath -w "$RUNNER_TEMP/msi-$1.log")" MODE="$1" pwsh -NoProfile -Command '
      $p = Start-Process msiexec.exe -Wait -PassThru -ArgumentList "/$env:MODE", "`"$env:MSI`"", "/qn", "/norestart", "/l*v", "`"$env:LOG`""
      exit $p.ExitCode'
  }
  msi_log() { tr -d '\000\r' < "$RUNNER_TEMP/msi-$1.log"; }
  if ! msiexec i; then
    show sh -c 'tail -n 40; exit 1' msiexec < <(msi_log i)
  fi
  installdir=$(msi_log i | sed -n 's/^.*Property(S): INSTALLDIR = //p')
  installdir=${installdir%%$'\n'*}
  if [ -z "$installdir" ]; then
    show sh -c 'grep -a "Property(S): .*\(DIR\|FOLDER\) = " | tail -n 40; exit 1' msiexec < <(msi_log i)
  fi
  dir=$(cygpath -u "$installdir")
  app=$(find "$dir" -maxdepth 1 -iname '*.exe' ! -iname 'uninstall*' | head -n 1)
  test -n "$app" || { echo "::error::The MSI installed no app in $dir"; exit 1; }
  echo "installed at $app"
  show python "$stays_up" "$app"
  if ! msiexec x; then
    show sh -c 'tail -n 40; exit 1' msiexec < <(msi_log x)
  fi
  test ! -e "$app"
}

case "$RUNNER_OS" in
  Linux) linux ;;
  macOS) macos ;;
  Windows) windows ;;
  *) echo "::error::No install check for $RUNNER_OS"; exit 1 ;;
esac
