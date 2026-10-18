# Fails if a running Ibis process, or any process it started (such as the
# WebView2 runtime), holds a TCP connection to a non-loopback address.
# PRIVACY.md promises no network use while Ibis is running (#201).
param(
  [Parameter(Mandatory)] [int] $AppId,
  [int] $Seconds = 60,
  [int] $Interval = 3
)

$ErrorActionPreference = 'Stop'

function Get-Tree([int] $Root) {
  $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId, Name
  $ids = [System.Collections.Generic.HashSet[int]]::new()
  [void] $ids.Add($Root)
  do {
    $added = $false
    foreach ($p in $all) {
      if ($ids.Contains([int] $p.ParentProcessId) -and $ids.Add([int] $p.ProcessId)) { $added = $true }
    }
  } while ($added)
  $all | Where-Object { $ids.Contains([int] $_.ProcessId) }
}

function Test-Local([string] $Address) {
  $parsed = $null
  if (-not [System.Net.IPAddress]::TryParse($Address, [ref] $parsed)) { return $false }
  if ($parsed.IsIPv4MappedToIPv6) { $parsed = $parsed.MapToIPv4() }
  [System.Net.IPAddress]::IsLoopback($parsed) -or
    $parsed.Equals([System.Net.IPAddress]::Any) -or
    $parsed.Equals([System.Net.IPAddress]::IPv6Any)
}

function Invoke-OfflineCheck([int] $AppId, [int] $Seconds, [int] $Interval) {
  $seen = @{}
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    if (-not (Get-Process -Id $AppId -ErrorAction SilentlyContinue)) { throw 'Ibis exited during the offline check.' }
    $tree = @(Get-Tree $AppId)
    $names = @{}
    foreach ($p in $tree) { $names[[int] $p.ProcessId] = $p.Name }
    $connections = Get-NetTCPConnection -ErrorAction Stop |
      Where-Object { $names.ContainsKey([int] $_.OwningProcess) -and $_.State -ne 'Listen' -and -not (Test-Local $_.RemoteAddress) }
    foreach ($c in $connections) {
      $key = "$($names[[int] $c.OwningProcess]) -> $($c.RemoteAddress):$($c.RemotePort)"
      if (-not $seen.ContainsKey($key)) {
        $seen[$key] = $c.RemoteAddress
        Write-Output "Connection: $key ($($c.State))"
      }
    }
    Start-Sleep -Seconds $Interval
  }

  Write-Output "Processes checked: $(@(Get-Tree $AppId).Count)"
  if ($seen.Count -gt 0) {
    $addresses = @($seen.Values | Sort-Object -Unique)
    $hosts = Get-DnsClientCache -ErrorAction SilentlyContinue | Where-Object { $addresses -contains $_.Data } |
      Select-Object -ExpandProperty Entry -Unique
    if ($hosts) { Write-Output "Host names for those addresses: $($hosts -join ', ')" }
    throw "Ibis made $($seen.Count) network connection(s) while running; PRIVACY.md promises none."
  }
  Write-Output "No network connections from Ibis in $Seconds seconds."
}

# Tests dot-source the functions without starting a live process check.
if ($MyInvocation.InvocationName -ne '.') {
  Invoke-OfflineCheck $AppId $Seconds $Interval
}
