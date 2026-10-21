{ runCommand, desktop-file-utils, ibis }:

# Inspect the installed output; this does not claim a graphical runtime test.
runCommand "ibis-package-check" { nativeBuildInputs = [ desktop-file-utils ]; } ''
  test -x ${ibis}/bin/ibis-desktop
  desktops=(${ibis}/share/applications/*.desktop)
  test "''${#desktops[@]}" -eq 1
  desktop=''${desktops[0]}
  desktop-file-validate "$desktop"
  grep -Fx 'Name=Ibis' "$desktop"
  grep -Fx 'Exec=${ibis}/bin/ibis-desktop' "$desktop"
  icon=$(sed -n 's/^Icon=//p' "$desktop")
  case "$icon" in "" | */*) exit 1 ;; esac
  test -n "$(find ${ibis}/share/icons/hicolor -type f \( -name "$icon.png" -o -name "$icon.svg" \) -print -quit)"
  mkdir "$out"
''
