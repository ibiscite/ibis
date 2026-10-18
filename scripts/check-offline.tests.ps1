$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/check-offline.ps1" -AppId $PID

foreach ($address in @('127.0.0.1', '127.1.2.3', '::1', '::ffff:127.0.0.1', '0.0.0.0', '::')) {
  if (-not (Test-Local $address)) { throw "Expected local socket endpoint: $address" }
}
foreach ($address in @('fe80::1%12', '192.168.0.2', '10.0.0.1', '2001:db8::1', '::ffff:192.168.0.2', 'not-an-address')) {
  if (Test-Local $address) { throw "External or invalid endpoint incorrectly permitted: $address" }
}

# An inaccessible/broken TCP provider must fail, never print a clean result.
function Get-Tree([int] $Root) { [pscustomobject]@{ ProcessId = $Root; Name = 'synthetic-ibis' } }
function Get-NetTCPConnection {
  [CmdletBinding()] param()
  Write-Error 'synthetic provider failure'
}
try {
  Invoke-OfflineCheck $PID 1 0
  throw 'Expected the provider failure to stop the check.'
} catch {
  if ($_.Exception.Message -ne 'synthetic provider failure') { throw }
}
Write-Output 'Offline classifier and provider-failure regressions passed.'
