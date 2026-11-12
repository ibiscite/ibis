param([ValidateSet('Add', 'Remove', 'Rollback')][string]$Action, [string]$Directory = '', [string]$Transaction = '')
$ErrorActionPreference = 'Stop'

function Get-IbisPathLockName {
  # One owner for this user's Stable and Nightly installers, across sessions.
  'Global\ibiscite.Ibis.CommandPath.' + [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
}

function Invoke-IbisPathUpdate {
  param([ValidateSet('Add', 'Remove', 'Rollback')][string]$Action, [string]$Directory, [string]$Transaction = '',
        $Registry, [string]$MutexName, [string]$MachinePath = '')
  $Directory = [IO.Path]::GetFullPath($Directory)
  if ($Directory.Length -gt 3) { $Directory = $Directory.TrimEnd('\') }
  if ($Transaction -ne '' -and $Transaction -notmatch '^\{[0-9a-fA-F-]{36}\}$') { throw 'Invalid installer transaction.' }
  if ($Action -eq 'Rollback' -and $Transaction -eq '') { throw 'Rollback needs an installer transaction.' }
  $receiptsKey = 'Software\ibiscite\Ibis\CommandPath'
  $transactionsKey = 'Software\ibiscite\Ibis\CommandPathTransactions'
  $mutex = [Threading.Mutex]::new($false, $MutexName)
  $locked = $false
  $key = $receipts = $transactions = $null
  try {
    try { $locked = $mutex.WaitOne(60000) }
    catch [Threading.AbandonedMutexException] { $locked = $true }
    if (-not $locked) { throw 'Another Ibis installer is updating your PATH. Try again.' }
    # Lock before opening any key, and hold it through empty-key cleanup.
    $key = $Registry.CreateSubKey('Environment')
    $raw = $key.GetValue('Path', $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $kind = if ($null -eq $raw) { [Microsoft.Win32.RegistryValueKind]::ExpandString } else { $key.GetValueKind('Path') }
    $receipts = $Registry.CreateSubKey($receiptsKey)
    $hash = [Security.Cryptography.SHA256]::Create()
    try { $receipt = [BitConverter]::ToString($hash.ComputeHash([Text.Encoding]::UTF8.GetBytes($Directory.ToUpperInvariant()))).Replace('-', '') }
    finally { $hash.Dispose() }
    $saved = $receipts.GetValue($receipt, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $parts = [Collections.Generic.List[string]]::new()
    if ($null -ne $raw -and $raw -ne '') { foreach ($part in $raw.Split(';')) { $parts.Add($part) } }
    $normalize = { param($entry) $entry.Trim('"').TrimEnd('\') }
    $index = -1
    for ($i = $parts.Count - 1; $i -ge 0; $i--) {
      if ((& $normalize $parts[$i]) -ieq (& $normalize $Directory)) { $index = $i; break }
    }
    $value = $raw
    $valueKind = $kind
    $nextReceipt = $saved
    if ($Transaction -ne '') { $transactions = $Registry.CreateSubKey($transactionsKey) }
    if ($Action -eq 'Rollback') {
      $snapshot = $transactions.GetValue($Transaction, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
      if ($null -ne $snapshot) {
        $snapshot = $snapshot | ConvertFrom-Json
        if ($snapshot.Directory -cne $Directory) { throw 'The PATH transaction belongs to another installation.' }
        # A newer receipt owns its own change. Do not consume it on rollback.
        if ($saved -ceq $snapshot.AfterReceipt) {
          if ($snapshot.Path -cne $snapshot.After) {
            if ($raw -ceq $snapshot.After -and $kind.ToString() -ceq $snapshot.AfterKind) {
              $value = $snapshot.Path
              $valueKind = [Microsoft.Win32.RegistryValueKind]$snapshot.Kind
            } elseif ($snapshot.Action -eq 'Add' -and $index -ge 0) {
              # Another installer/user edited PATH after us: undo only our entry.
              $parts.RemoveAt($index)
              $value = $parts -join ';'
            } elseif ($snapshot.Action -eq 'Remove' -and $index -lt 0 -and $snapshot.RemovedIndex -ge 0) {
              $parts.Insert([Math]::Min([int]$snapshot.RemovedIndex, $parts.Count), [string]$snapshot.RemovedEntry)
              $value = $parts -join ';'
            }
          }
          $nextReceipt = $snapshot.Receipt
        }
      }
    } elseif ($Action -eq 'Add' -and $index -lt 0) {
      $value = if ($null -eq $raw -or $raw -eq '') { $Directory } else { $raw + ';' + $Directory }
      $record = @{ Directory=$Directory; Before=$raw; Kind=$kind.ToString(); After=$value }
      $nextReceipt = $record | ConvertTo-Json -Compress
    } elseif ($Action -eq 'Remove' -and $null -ne $saved) {
      $record = $saved | ConvertFrom-Json
      if ($record.Directory -cne $Directory) { throw 'The PATH receipt belongs to another installation.' }
      if ($raw -ceq $record.After) {
        $value = $record.Before
        $valueKind = [Microsoft.Win32.RegistryValueKind]$record.Kind
      } elseif ($index -ge 0) {
        $parts.RemoveAt($index)
        $value = $parts -join ';'
      }
      $nextReceipt = $null
    }
    # Keep registry data unexpanded. Before adding/restoring an entry, validate
    # machine + user PATH and reserve one UTF-16 character for its null. Removal
    # must still be able to shorten a PATH another installer made oversized.
    $adding = $Action -eq 'Add' -or ($Action -eq 'Rollback' -and $snapshot.Action -eq 'Remove')
    if ($adding -and $value -cne $raw -and $null -ne $value) {
      $effective = if ($MachinePath -and $value) { $MachinePath + ';' + $value } else { $MachinePath + $value }
      if ([Environment]::ExpandEnvironmentVariables($effective).Length -ge 32767) { throw 'The expanded PATH is too long to add the ibis command.' }
    }
    if ($null -ne $transactions -and $Action -ne 'Rollback') {
      # Record both sides before publishing anything. Rollback restores the
      # exact snapshot only while this transaction's result is still current.
      $snapshot = @{ Directory=$Directory; Action=$Action; Path=$raw; Kind=$kind.ToString(); Receipt=$saved;
                     After=$value; AfterKind=$valueKind.ToString(); AfterReceipt=$nextReceipt; RemovedIndex=$index;
                     RemovedEntry=$(if ($index -ge 0) { $raw.Split(';')[$index] } else { $null }) }
      $transactions.SetValue($Transaction, ($snapshot | ConvertTo-Json -Compress), [Microsoft.Win32.RegistryValueKind]::String)
    }
    try {
      if ($nextReceipt -cne $saved) {
        if ($null -eq $nextReceipt) { $receipts.DeleteValue($receipt, $false) }
        else { $receipts.SetValue($receipt, $nextReceipt, [Microsoft.Win32.RegistryValueKind]::String) }
      }
      if ($value -cne $raw -or $valueKind -ne $kind) {
        if ($null -eq $value) { $key.DeleteValue('Path', $false) }
        else { $key.SetValue('Path', $value, $valueKind) }
      }
    } catch {
      # A refused registry write must not leave a new ownership receipt.
      if ($nextReceipt -cne $saved) {
        if ($null -eq $saved) { $receipts.DeleteValue($receipt, $false) }
        else { $receipts.SetValue($receipt, $saved, [Microsoft.Win32.RegistryValueKind]::String) }
      }
      throw
    }
    if ($Action -eq 'Rollback') { $transactions.DeleteValue($Transaction, $false) }
  } finally {
    try {
      if ($null -ne $transactions) {
        $empty = $transactions.GetValueNames().Count -eq 0
        $transactions.Dispose()
        if ($empty) { $Registry.DeleteSubKey($transactionsKey, $false) }
      }
      if ($null -ne $receipts) {
        $empty = $receipts.GetValueNames().Count -eq 0
        $receipts.Dispose()
        if ($empty) { $Registry.DeleteSubKey($receiptsKey, $false) }
      }
      if ($null -ne $key) { $key.Dispose() }
    } finally {
      if ($locked) { $mutex.ReleaseMutex() }
      $mutex.Dispose()
    }
  }
}

# Dot sourcing exposes the same updater to isolated registry-shaped test stores.
if ($MyInvocation.InvocationName -ne '.') {
  if ($Directory -eq '' -or $Action -eq '') { throw 'An action and installation directory are required.' }
  $machine = [Environment]::GetEnvironmentVariable('Path', [EnvironmentVariableTarget]::Machine)
  Invoke-IbisPathUpdate -Action $Action -Directory $Directory -Transaction $Transaction -Registry ([Microsoft.Win32.Registry]::CurrentUser) -MutexName (Get-IbisPathLockName) -MachinePath $machine
  # Notify apps without changing this process or requiring elevation.
  Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class IbisEnvironment {
  [DllImport("user32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wparam, string text, uint flags, uint timeout, out UIntPtr result);
}
'@
  $result = [UIntPtr]::Zero
  [void][IbisEnvironment]::SendMessageTimeout([IntPtr]0xffff, 0x1a, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
}
