# Preview's "Capture the picker (Windows)" step (preview.yml), from the
# repository root: start the release app, watch it stay offline for 60 seconds,
# including delayed WebView2 background services, then take a screenshot.
# TARGET: the Rust target.
$ErrorActionPreference = 'Stop'
./scripts/check-offline.tests.ps1
New-Item -ItemType Directory -Force screenshots | Out-Null
$app = Start-Process "target/$env:TARGET/release/ibis-desktop.exe" -NoNewWindow -PassThru -RedirectStandardOutput screenshots/startup-Windows.log -RedirectStandardError screenshots/stderr-Windows.log
try {
  ./scripts/check-offline.ps1 -AppId $app.Id -Seconds 60
  if ($app.HasExited) { throw 'The picker exited during startup; see the startup logs.' }
  try {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $bitmap = [System.Drawing.Bitmap]::new($bounds.Width, $bounds.Height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    try {
      $graphics.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
      $bitmap.Save("$PWD/screenshots/picker-Windows.png")
    } finally { $graphics.Dispose(); $bitmap.Dispose() }
  } catch { Write-Output "::warning::Windows screenshot capture failed: $($_.Exception.Message)" }
} finally { if (!$app.HasExited) { Stop-Process -Id $app.Id -Force } }
