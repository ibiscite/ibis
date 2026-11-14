param([string]$Worker = '', [string]$Store = '', [string]$MutexName = '', [string]$Ready = '', [string]$Release = '', [string]$Probe = '')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../apps/desktop/src-tauri/packaging/path.ps1"

# A registry-shaped store, entirely in memory or in the test's temporary folder.
# The production updater and its real named mutex run unchanged in every process.
class PathTestRegistry {
  [hashtable]$Memory = @{}
  [string]$File = ''
  [string]$Ready = ''
  [string]$Release = ''
  [bool]$PauseCleanup = $false
  [bool]$FailPathWrite = $false
  # JSON stores nested maps and scalar registry values. Rebuild those maps on
  # both PowerShell versions, preserving nulls and empty strings verbatim.
  hidden static [hashtable] ToHashtable([object]$Object) {
    $data = @{}
    foreach ($property in $Object.PSObject.Properties) {
      $value = $property.Value
      if ($value -is [pscustomobject]) { $value = [PathTestRegistry]::ToHashtable($value) }
      $data[$property.Name] = $value
    }
    return $data
  }
  [hashtable] Read() {
    if ($this.File) {
      # WriteAllText uses UTF-8 without a BOM; 5.1 otherwise reads ANSI.
      return [PathTestRegistry]::ToHashtable((Get-Content -Raw -Encoding UTF8 -LiteralPath $this.File | ConvertFrom-Json))
    }
    return $this.Memory
  }
  [void] Write([hashtable]$Data) {
    if ($this.File) { [IO.File]::WriteAllText($this.File, ($Data | ConvertTo-Json -Depth 20 -Compress)) }
    else { $this.Memory = $Data }
  }
  [void] Pause() {
    [IO.File]::WriteAllText($this.Ready, 'paused')
    $deadline = [Diagnostics.Stopwatch]::StartNew()
    while (-not [IO.File]::Exists($this.Release)) {
      if ($deadline.Elapsed.TotalSeconds -gt 15) { throw 'Timed out releasing the paused registry operation.' }
      Start-Sleep -Milliseconds 10
    }
  }
  [object] CreateSubKey([string]$Name) {
    if ($this.Ready -and -not $this.PauseCleanup -and $Name -eq 'Environment') { $this.Pause() }
    $data = $this.Read()
    if (-not $data.ContainsKey($Name)) { $data[$Name] = @{}; $this.Write($data) }
    return [PathTestKey]::new($this, $Name)
  }
  [void] DeleteSubKey([string]$Name, [bool]$MissingIsError) {
    $data = $this.Read()
    if ($data.ContainsKey($Name) -and $data[$Name].Count -gt 0) { throw 'Deleting a nonempty registry key.' }
    if ($this.PauseCleanup -and $Name -eq 'Software\ibiscite\Ibis\CommandPath') { $this.Pause() }
    $data.Remove($Name)
    $this.Write($data)
  }
}
class PathTestKey {
  [object]$Root
  [string]$Name
  PathTestKey([object]$Root, [string]$Name) { $this.Root = $Root; $this.Name = $Name }
  [object] GetValue([string]$Name, [object]$Default, [object]$Options) {
    $data = $this.Root.Read()
    if ($data[$this.Name].ContainsKey($Name)) { return $data[$this.Name][$Name].Value }
    return $Default
  }
  [object] GetValueKind([string]$Name) {
    $data = $this.Root.Read()
    return [Microsoft.Win32.RegistryValueKind]$data[$this.Name][$Name].Kind
  }
  [string[]] GetValueNames() { return @($this.Root.Read()[$this.Name].Keys) }
  [void] SetValue([string]$Name, [object]$Value, [object]$Kind) {
    if ($this.Name -eq 'Environment' -and $Name -eq 'Path' -and $this.Root.FailPathWrite) { throw 'Synthetic PATH write failure.' }
    $data = $this.Root.Read()
    $data[$this.Name][$Name] = @{ Value=$Value; Kind=$Kind.ToString() }
    $this.Root.Write($data)
  }
  [void] DeleteValue([string]$Name, [bool]$MissingIsError) {
    $data = $this.Root.Read()
    $data[$this.Name].Remove($Name)
    $this.Root.Write($data)
  }
  [void] Dispose() {}
}

function Equal($Actual, $Expected, [string]$Message) {
  if ($Actual -cne $Expected) { throw $Message }
}
function Reject([scriptblock]$Operation, [string]$Message) {
  $rejected = $false
  try { & $Operation } catch { $rejected = $_.Exception.Message -like "*$Message*" }
  if (-not $rejected) { throw "Expected refusal: $Message" }
}
function Raw($Registry) {
  $key = $Registry.CreateSubKey('Environment')
  $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
}
function Put($Registry, $Value, $Kind = [Microsoft.Win32.RegistryValueKind]::ExpandString) {
  $Registry.CreateSubKey('Environment').SetValue('Path', $Value, $Kind)
}
function Wait-File([string]$Path) {
  $deadline = [Diagnostics.Stopwatch]::StartNew()
  while (-not [IO.File]::Exists($Path)) {
    if ($deadline.Elapsed.TotalSeconds -gt 15) { throw 'Timed out waiting for a PATH test worker.' }
    Start-Sleep -Milliseconds 10
  }
}

if ($Worker) {
  $registry = [PathTestRegistry]::new()
  $registry.File = $Store
  if ($Worker -in @('Stable', 'Remove')) {
    $registry.Ready = $Ready; $registry.Release = $Release
    $registry.PauseCleanup = $Worker -eq 'Remove'
  }
  if ($Worker -eq 'Nightly') {
    $probeLock = [Threading.Mutex]::new($false, $MutexName)
    try {
      if ($probeLock.WaitOne(0)) {
        $probeLock.ReleaseMutex()
        throw 'The first channel accessed the registry without owning the cross-process mutex.'
      }
      [IO.File]::WriteAllText($Probe, 'blocked')
    } finally { $probeLock.Dispose() }
  }
  $action = if ($Worker -eq 'Remove') { 'Remove' } else { 'Add' }
  $folder = if ($Worker -eq 'Remove') { 'Stable' } else { $Worker }
  Invoke-IbisPathUpdate -Action $action -Directory (Join-Path ([IO.Path]::GetDirectoryName($Store)) $folder) -Registry $registry -MutexName $MutexName -MachinePath ''
  exit 0
}

$base = Join-Path ([IO.Path]::GetTempPath()) ('ibis-path-tests-' + [guid]::NewGuid().ToString('N'))
[void][IO.Directory]::CreateDirectory($base)
$stable = Join-Path $base 'Stable'
$nightly = Join-Path $base 'Nightly'
$foreign = Join-Path $base 'Foreign'
$mutex = 'ibis-path-tests-' + [guid]::NewGuid().ToString('N')
$receipts = 'Software\ibiscite\Ibis\CommandPath'
function Update($Registry, [string]$Action, [string]$Directory = $stable, [string]$Machine = '') {
  Invoke-IbisPathUpdate -Action $Action -Directory $Directory -Registry $Registry -MutexName $mutex -MachinePath $Machine
}

try {
  # Install and uninstall preserve an originally absent value and registry type.
  foreach ($initial in @($null, '', 'invented;;%IBIS_TEST_ROOT%;')) {
    $registry = [PathTestRegistry]::new()
    if ($null -ne $initial) { Put $registry $initial ([Microsoft.Win32.RegistryValueKind]::String) }
    Update $registry Add $stable
    Update $registry Remove $stable
    Equal (Raw $registry) $initial 'Uninstall did not restore the exact original PATH.'
    if ($null -ne $initial) { Equal ($registry.CreateSubKey('Environment').GetValueKind('Path')) ([Microsoft.Win32.RegistryValueKind]::String) 'Uninstall changed the registry type.' }
    Equal ($registry.Read()[$receipts]) $null 'Uninstall left an ownership receipt.'
  }

  $registry = [PathTestRegistry]::new()
  Put $registry "original;$stable"
  Update $registry Add $stable
  Update $registry Remove
  Equal (Raw $registry) "original;$stable" 'Removal consumed a preexisting PATH entry.'
  Equal ($registry.Read()[$receipts]) $null 'A preexisting entry acquired an ownership receipt.'

  $registry = [PathTestRegistry]::new()
  Put $registry 'original;'
  Update $registry Add $stable
  Update $registry Add $nightly
  Put $registry ((Raw $registry) + ';' + $foreign)
  Update $registry Remove $stable
  Equal (Raw $registry) "original;;$nightly;$foreign" 'Removal erased a later installer or user PATH entry.'
  Update $registry Remove $stable
  Equal (Raw $registry) "original;;$nightly;$foreign" 'Repeated removal changed PATH.'
  Update $registry Remove $nightly
  Equal (Raw $registry) "original;;$foreign" 'Removal erased a foreign PATH entry.'
  Write-Output 'PASS: removal preserves later PATH edits and other channels.'

  # Expansion and the machine PATH both count toward the effective value.
  $oldExpansion = [Environment]::GetEnvironmentVariable('IBIS_PATH_TEST_EXPANSION')
  try {
    [Environment]::SetEnvironmentVariable('IBIS_PATH_TEST_EXPANSION', ('x' * 32000))
    foreach ($initial in @('%IBIS_PATH_TEST_EXPANSION%', ('x' * 32000))) {
      $registry = [PathTestRegistry]::new()
      Put $registry $initial
      Reject { Update $registry Add $stable ('m' * 1000) } 'PATH is too long'
      Equal (Raw $registry) $initial 'Overflow changed PATH.'
      Equal ($registry.Read()[$receipts]) $null 'Overflow wrote a receipt.'
    }
    foreach ($length in @(32766, 32767)) {
      $registry = [PathTestRegistry]::new()
      $initial = 'x' * ($length - $stable.Length - 1)
      Put $registry $initial
      if ($length -eq 32766) {
        Update $registry Add
        Equal ((Raw $registry).Length) $length 'A valid boundary PATH was refused or truncated.'
        Update $registry Remove
        Equal (Raw $registry) $initial 'Boundary removal did not restore PATH.'
      } else {
        Reject { Update $registry Add } 'PATH is too long'
        Equal (Raw $registry) $initial 'The terminating-null boundary changed PATH.'
      }
    }
  } finally { [Environment]::SetEnvironmentVariable('IBIS_PATH_TEST_EXPANSION', $oldExpansion) }
  Write-Output 'PASS: expanded machine plus user PATH is bounded before any ownership write.'

  $registry = [PathTestRegistry]::new()
  Put $registry 'original'
  Update $registry Add $stable
  $later = 'x' * 32767
  Put $registry ((Raw $registry) + ';' + $later)
  Update $registry Remove $stable
  Equal (Raw $registry) "original;$later" 'An oversized later edit prevented removal of only Ibis.'

  $registry = [PathTestRegistry]::new()
  Put $registry 'original'
  $registry.FailPathWrite = $true
  Reject { Update $registry Add } 'Synthetic PATH write failure'
  Equal (Raw $registry) 'original' 'Failed I/O changed PATH.'
  Equal ($registry.Read()[$receipts]) $null 'Failed I/O left an ownership receipt.'
  $registry.FailPathWrite = $false
  Update $registry Add
  Update $registry Remove
  Equal (Raw $registry) 'original' 'Could not retry after failed I/O.'

  # Pause Stable at its first registry access, and separately at empty receipt
  # cleanup. Nightly must observe the mutex held in both cases, then complete.
  foreach ($removing in @($false, $true)) {
    $stage = if ($removing) { 'cleanup' } else { 'read' }
    $store = Join-Path $base ($stage + '-registry.json')
    [IO.File]::WriteAllText($store, '{}')
    if ($removing) {
      $registry = [PathTestRegistry]::new()
      $registry.File = $store
      Update $registry Add
    }
    $readyFile = Join-Path $base ($stage + '-ready')
    $releaseFile = Join-Path $base ($stage + '-release')
    $probeFile = Join-Path $base ($stage + '-probe')
    $first = if ($removing) { 'Remove' } else { 'Stable' }
    $workers = @()
    try {
      foreach ($role in @($first, 'Nightly')) {
        $start = [Diagnostics.ProcessStartInfo]::new()
        $start.FileName = (Get-Process -Id $PID).Path
        $start.UseShellExecute = $false
        # Windows PowerShell 5.1, which installers run, has no ArgumentList: quote each argument instead.
        $start.Arguments = (@('-NoProfile', '-NonInteractive', '-File', $PSCommandPath, '-Worker', $role, '-Store', $store,
            '-MutexName', $mutex, '-Ready', $readyFile, '-Release', $releaseFile, '-Probe', $probeFile) |
          ForEach-Object { '"' + ($_ -replace '"', '\"') + '"' }) -join ' '
        $workers += [Diagnostics.Process]::Start($start)
        if ($role -eq $first) { Wait-File $readyFile }
      }
      Wait-File $probeFile
      [IO.File]::WriteAllText($releaseFile, 'continue')
      foreach ($workerProcess in $workers) {
        if (-not $workerProcess.WaitForExit(15000)) { throw 'Timed out joining a PATH test worker.' }
        Equal $workerProcess.ExitCode 0 'A concurrent channel update failed.'
      }
      $registry = [PathTestRegistry]::new()
      $registry.File = $store
      if (-not $removing) {
        Equal (Raw $registry) "$stable;$nightly" 'Concurrent channels lost an installed command.'
        Update $registry Remove $stable
      }
      Equal (Raw $registry) $nightly 'Removing Stable lost Nightly.'
      Update $registry Remove $nightly
      $expected = if ($removing) { $null } else { '' }
      Equal (Raw $registry) $expected 'Removing both channels left an installed command.'
      Equal ($registry.Read()[$receipts]) $null 'Concurrent-channel receipt cleanup failed.'
    } finally {
      foreach ($workerProcess in $workers) {
        if (-not $workerProcess.HasExited) { $workerProcess.Kill() }
        $workerProcess.Dispose()
      }
    }
  }
  Write-Output 'PASS: cross-process channel updates and receipt cleanup are serialized.'
} finally { Remove-Item -LiteralPath $base -Recurse -Force }
