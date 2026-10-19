# Fails if a running Ibis process, or any process it started (such as the
# WebView2 runtime), holds a TCP connection to a non-loopback address.
# PRIVACY.md promises no network use while Ibis is running (#201).
param(
  [Parameter(Mandatory)] [int] $AppId,
  [int] $Seconds = 60,
  [int] $Interval = 3
)

$ErrorActionPreference = 'Stop'

function Get-CreationIdentity($Process, [string] $Failure) {
  # Reject the original default value before local-to-UTC conversion: west of
  # UTC, an unspecified MinValue becomes a positive UTC tick count.
  if ($Process.CreationDate -isnot [DateTime] -or $Process.CreationDate.Ticks -le 0 -or
      $Process.CreationDate.ToUniversalTime().Ticks -le 0) { throw $Failure }
  $Process.CreationDate.ToUniversalTime().Ticks
}

function Get-Tree([int] $Root, [long] $RootCreated = 0) {
  $all = @(Get-CimInstance Win32_Process -ErrorAction Stop | Select-Object ProcessId, ParentProcessId, Name, CreationDate)
  $byId = @{}
  foreach ($p in $all) {
    if ($byId.ContainsKey([int] $p.ProcessId)) { throw 'Process enumeration returned duplicate identities.' }
    $byId[[int] $p.ProcessId] = $p
  }
  if (-not $byId.ContainsKey($Root)) { throw 'Ibis exited during the offline check.' }
  $created = @{}
  $created[$Root] = Get-CreationIdentity $byId[$Root] 'Ibis creation identity is unavailable.'
  if ($RootCreated -ne 0 -and $created[$Root] -ne $RootCreated) { throw 'Ibis process identity changed during the offline check.' }
  $ids = [System.Collections.Generic.HashSet[int]]::new()
  [void] $ids.Add($Root)
  do {
    $added = $false
    foreach ($p in $all) {
      if (-not $ids.Contains([int] $p.ProcessId) -and $ids.Contains([int] $p.ParentProcessId)) {
        $identity = Get-CreationIdentity $p 'Descendant creation identity is unavailable.'
        # A Windows parent PID can outlive its original process. An older
        # orphan cannot be a child of the newer process reusing that number.
        if ($identity -ge $created[[int] $p.ParentProcessId]) {
          $created[[int] $p.ProcessId] = $identity
          if ($ids.Add([int] $p.ProcessId)) { $added = $true }
        }
      }
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
  $rootCreated = 0L
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    $tree = @(Get-Tree $AppId $rootCreated)
    if ($rootCreated -eq 0) {
      $rootCreated = Get-CreationIdentity ($tree | Where-Object { $_.ProcessId -eq $AppId }) 'Ibis creation identity is unavailable.'
    }
    $names = @{}
    foreach ($p in $tree) { $names[[int] $p.ProcessId] = $p.Name }
    $connections = @(Get-NetTCPConnection -ErrorAction Stop |
      Where-Object { $names.ContainsKey([int] $_.OwningProcess) -and $_.State -ne 'Listen' -and -not (Test-Local $_.RemoteAddress) })
    if ($connections.Count) {
      # TCP enumeration is not atomic with the process snapshot. Revalidate
      # candidate owners before attributing traffic to a possibly reused PID.
      try { $fresh = @(Get-Tree $AppId $rootCreated) }
      catch { throw 'Process identity is unavailable while sampling network connections.' }
      $confirmed = @{}
      foreach ($p in $fresh) { $confirmed[[int] $p.ProcessId] = $p }
      foreach ($c in $connections) {
        $owner = [int] $c.OwningProcess
        $original = $tree | Where-Object { $_.ProcessId -eq $owner }
        if (-not $confirmed.ContainsKey($owner) -or
            (Get-CreationIdentity $original 'Process identity is unavailable while sampling network connections.') -ne
            (Get-CreationIdentity $confirmed[$owner] 'Process identity is unavailable while sampling network connections.')) {
          throw 'Process identity changed while sampling network connections.'
        }
      }
    }
    foreach ($c in $connections) {
      $key = "$($names[[int] $c.OwningProcess]) -> $($c.RemoteAddress):$($c.RemotePort)"
      if (-not $seen.ContainsKey($key)) {
        $seen[$key] = $c.RemoteAddress
        Write-Output "Connection: $key ($($c.State))"
      }
    }
    Start-Sleep -Seconds $Interval
  }

  Write-Output "Processes checked: $(@(Get-Tree $AppId $rootCreated).Count)"
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
