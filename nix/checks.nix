{ runCommand, desktop-file-utils, ibis-desktop }:

# Inspect the installed output; this does not claim a graphical runtime test.
runCommand "ibis-desktop-package-check" { nativeBuildInputs = [ desktop-file-utils ]; } ''
  test -x ${ibis-desktop}/bin/ibis-desktop
  desktops=(${ibis-desktop}/share/applications/*.desktop)
  test "''${#desktops[@]}" -eq 1
  desktop=''${desktops[0]}
  desktop-file-validate "$desktop"
  grep -Fx 'Name=Ibis' "$desktop"
  grep -Fx 'Exec=${ibis-desktop}/bin/ibis-desktop' "$desktop"
  icon=$(sed -n 's/^Icon=//p' "$desktop")
  case "$icon" in "" | */*) exit 1 ;; esac
  test -n "$(find ${ibis-desktop}/share/icons/hicolor -type f \( -name "$icon.png" -o -name "$icon.svg" \) -print -quit)"
  mkdir "$out"
''
