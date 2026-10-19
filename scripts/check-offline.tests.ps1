$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/check-offline.ps1" -AppId $PID

foreach ($address in @('127.0.0.1', '127.1.2.3', '::1', '::ffff:127.0.0.1', '0.0.0.0', '::')) {
  if (-not (Test-Local $address)) { throw "Expected local socket endpoint: $address" }
}
foreach ($address in @('fe80::1%12', '192.168.0.2', '10.0.0.1', '2001:db8::1', '::ffff:192.168.0.2', 'not-an-address')) {
  if (Test-Local $address) { throw "External or invalid endpoint incorrectly permitted: $address" }
}

$created = [DateTime]::new(2026,10,19,0,0,0,[DateTimeKind]::Utc)
function Process([int] $Id, [int] $Parent, $Created) {
  [pscustomobject]@{ProcessId=$Id;ParentProcessId=$Parent;CreationDate=$Created;Name='synthetic-process'}
}
function Get-CimInstance {
  [CmdletBinding()] param([string] $ClassName)
  if ($null -ne $script:sequence) { return $script:sequence.Dequeue() }
  $script:processes
}
$script:processes = @(
  (Process 10 1 $created),
  (Process 11 10 $created.AddSeconds(-1)), # older orphan under a reused parent PID
  (Process 12 10 $created.AddSeconds(1)),
  (Process 13 12 $created.AddSeconds(2)),
  (Process 14 12 $created), # older orphan under a reused descendant PID
  (Process 15 11 $created.AddSeconds(3))
)
$tree = @(Get-Tree 10)
if ((@($tree.ProcessId | Sort-Object) -join ',') -ne '10,12,13') {
  throw 'PID reuse incorrectly attributed an older orphan or dropped a fresh descendant.'
}
$script:processes = @($script:processes[5..0]) + @(
  (Process 16 10 $created), # timestamp resolution can give a real child the same creation time
  (Process 99 100 $null) # unrelated protected/unknown process is not an Ibis exemption
)
if ((@((Get-Tree 10).ProcessId | Sort-Object) -join ',') -ne '10,12,13,16') {
  throw 'Creation identity traversal lost equal-time or out-of-order genuine descendants.'
}

function Reject([scriptblock] $Operation, [string] $Expected) {
  $rejected = $false
  $output = [System.Collections.Generic.List[string]]::new()
  try { & $Operation | ForEach-Object { $output.Add([string] $_) } } catch { $rejected = $_.Exception.Message -eq $Expected }
  if (-not $rejected) { throw 'Expected a fixed process identity failure.' }
  if ($output -match '^Connection:' -or $output -match '^No network connections') { throw 'Unverified identity produced a network verdict.' }
}
foreach ($invalid in @($null, '2026-10-19', [DateTime]::MinValue)) {
  $script:processes = @((Process 10 1 $invalid))
  Reject { Get-Tree 10 } 'Ibis creation identity is unavailable.'
  $script:processes = @((Process 10 1 $created), (Process 12 10 $invalid))
  Reject { Get-Tree 10 } 'Descendant creation identity is unavailable.'
}
# This fixed west-of-UTC control is independent of the runner's own timezone.
# A normalized-ticks-only check accepts the default value demonstrated here.
$west = [TimeZoneInfo]::CreateCustomTimeZone('synthetic-west', [TimeSpan]::FromHours(-5), 'synthetic-west', 'synthetic-west')
$default = [DateTime]::SpecifyKind([DateTime]::MinValue, [DateTimeKind]::Unspecified)
if ($default.Ticks -ne 0 -or [TimeZoneInfo]::ConvertTimeToUtc($default, $west).Ticks -le 0) {
  throw 'The timezone fixture did not reproduce a positive normalized default date.'
}
$script:processes = @((Process 10 1 $default))
Reject { Get-Tree 10 } 'Ibis creation identity is unavailable.'
$script:processes = @((Process 10 1 $created), (Process 12 10 $default))
Reject { Get-Tree 10 } 'Descendant creation identity is unavailable.'
$script:processes = @((Process 10 1 $created.AddSeconds(1)))
Reject { Get-Tree 10 $created.Ticks } 'Ibis process identity changed during the offline check.'
$script:processes = @()
Reject { Get-Tree 10 $created.Ticks } 'Ibis exited during the offline check.'
$script:processes = @((Process 10 1 $created), (Process 10 1 $created))
Reject { Get-Tree 10 } 'Process enumeration returned duplicate identities.'

# Drive the real polling loop without waiting or making OS/network requests.
$script:clock = $created
function Get-Date { $script:clock }
function Start-Sleep { param($Seconds) $script:clock = $script:clock.AddSeconds(1) }
function Get-NetTCPConnection { [CmdletBinding()] param() $script:connections }
function Get-DnsClientCache { [CmdletBinding()] param() @() }
function Samples($Values) {
  $script:sequence = [System.Collections.Generic.Queue[object]]::new()
  foreach ($value in $Values) { $script:sequence.Enqueue($value) }
  $script:clock = $created
}
$initial = @((Process 10 1 $created), (Process 12 10 $created.AddSeconds(1)))
$replacement = @((Process 10 1 $created.AddSeconds(1)))
$script:connections = @()
Samples @($initial, $replacement)
Reject { Invoke-OfflineCheck 10 2 0 } 'Ibis process identity changed during the offline check.'
Samples @($initial, $replacement)
Reject { Invoke-OfflineCheck 10 1 0 } 'Ibis process identity changed during the offline check.' # final snapshot

$script:connections = @([pscustomobject]@{OwningProcess=12;State='Established';RemoteAddress='203.0.113.1';RemotePort=443})
foreach ($changed in @(
    @((Process 10 1 $created), (Process 12 10 $created.AddSeconds(2))),
    @((Process 10 1 $created), (Process 12 99 $created.AddSeconds(1))),
    @((Process 10 1 $created))
)) {
  Samples @($initial, $changed)
  Reject { Invoke-OfflineCheck 10 1 0 } 'Process identity changed while sampling network connections.'
}
$unknown = @((Process 10 1 $created), (Process 12 10 $null))
Samples @($initial, $unknown)
Reject { Invoke-OfflineCheck 10 1 0 } 'Process identity is unavailable while sampling network connections.'

# A genuine stable new descendant's external connection must still fail.
Samples @($initial, $initial, $initial)
$rejected = $false
try { Invoke-OfflineCheck 10 1 0 | Out-Null } catch {
  $rejected = $_.Exception.Message -eq 'Ibis made 1 network connection(s) while running; PRIVACY.md promises none.'
}
if (-not $rejected) { throw 'A genuine descendant connection escaped monitoring.' }
$older = @((Process 10 1 $created), (Process 11 10 $created.AddSeconds(-1)),
           (Process 12 10 $created.AddSeconds(1)), (Process 99 100 $null))
$script:connections = @(
  [pscustomobject]@{OwningProcess=11;State='Established';RemoteAddress='203.0.113.1';RemotePort=443},
  [pscustomobject]@{OwningProcess=99;State='Established';RemoteAddress='203.0.113.2';RemotePort=443}
)
Samples @($older, $older)
$clean = @(Invoke-OfflineCheck 10 1 0)
if ($clean -match '^Connection:' -or $clean -notcontains 'No network connections from Ibis in 1 seconds.') {
  throw 'An older orphan or unrelated unknown process was blamed on Ibis.'
}
$script:sequence = $null

# An inaccessible/broken TCP provider must fail, never print a clean result.
function Get-Tree([int] $Root) { [pscustomobject]@{ ProcessId = $Root; Name = 'synthetic-ibis'; CreationDate = $created } }
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
