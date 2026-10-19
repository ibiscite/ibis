# UI Automation reads/invokes controls; SendInput exercises the actual shortcut
# and typing path. Called by smoke-native.py, not a test-mode application API.
param([string] $Action, [int] $AppId, [int] $EditorId, [string] $Text)
$ErrorActionPreference = 'Stop'
[Console]::Error.WriteLine('NATIVE HELPER STAGE windows-encoding-start')
# JSON crosses a pipe to Python; neither end may inherit an OEM/ANSI code page.
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
$OutputEncoding = [Console]::OutputEncoding
[Console]::Error.WriteLine('NATIVE HELPER STAGE windows-encoding-ready')
[Console]::Error.WriteLine('NATIVE HELPER STAGE windows-uia-assembly-start')
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
[Console]::Error.WriteLine('NATIVE HELPER STAGE windows-uia-assembly-ready')
[Console]::Error.WriteLine('NATIVE HELPER STAGE windows-input-compile-start')
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class IbisSmoke {
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public INPUTUNION u; }
  [StructLayout(LayoutKind.Explicit)] public struct INPUTUNION {
    [FieldOffset(0)] public KEYBDINPUT ki;
    [FieldOffset(0)] public MOUSEINPUT mi;
  }
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT {
    public ushort vk, scan; public uint flags, time; public UIntPtr extra;
  }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT {
    public int x,y; public uint data,flags,time; public UIntPtr extra;
  }
  [DllImport("user32.dll")] public static extern uint SendInput(uint n, INPUT[] inputs, int size);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr w, out uint pid);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr w);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr w);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr w);
  public static uint OwnerPid(IntPtr w) {
    uint pid; GetWindowThreadProcessId(w, out pid); return pid;
  }
  public static uint ForegroundPid() {
    uint pid; GetWindowThreadProcessId(GetForegroundWindow(), out pid); return pid;
  }
  public static void Key(ushort vk, ushort scan, uint flags) {
    var input = new INPUT { type = 1, u = new INPUTUNION { ki = new KEYBDINPUT { vk=vk, scan=scan, flags=flags } } };
    if (SendInput(1, new INPUT[] {input}, Marshal.SizeOf(typeof(INPUT))) != 1)
      throw new Exception("SendInput failed");
  }
}
'@
[Console]::Error.WriteLine('NATIVE HELPER STAGE windows-input-compile-ready')
function Get-AppWindow([int] $Id) {
  $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::ProcessIdProperty, $Id)
  $candidates = [System.Windows.Automation.AutomationElement]::RootElement.FindAll([System.Windows.Automation.TreeScope]::Children, $condition)
  if ($candidates.Count -gt 20) { throw 'Native picker window enumeration exceeded bound' }
  $search = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, 'Search references')
  foreach ($candidate in $candidates) {
    $controls = $candidate.FindAll([System.Windows.Automation.TreeScope]::Descendants, $search)
    if ($controls.Count -gt 4000) { throw 'Named search enumeration exceeded bound' }
    # The label and editable control can expose the same accessible name.
    # A preceding static label must not exclude its actual picker window.
    foreach ($control in $controls) {
      if ($control.Current.ControlType -eq [System.Windows.Automation.ControlType]::Edit -or
          $control.Current.ControlType -eq [System.Windows.Automation.ControlType]::ComboBox) { return $candidate }
    }
  }
  return $null
}
function Get-Tree {
  if ($Text -and $Text -ne '0') {
    $handle = [IntPtr]::new([long]::Parse($Text, [System.Globalization.CultureInfo]::InvariantCulture))
    if (![IbisSmoke]::IsWindow($handle) -or [IbisSmoke]::OwnerPid($handle) -ne $AppId) { throw 'Verified picker window identity was lost' }
    if (![IbisSmoke]::IsWindowVisible($handle)) { return @{nodes=@();visible=$false;foreground=$false;picker_handle=$handle.ToInt64()} }
    $window = [System.Windows.Automation.AutomationElement]::FromHandle($handle)
  } else {
    $window = Get-AppWindow $AppId
  }
  if (!$window) { return @{nodes=@();visible=$false;foreground=$false} }
  $handle = [IntPtr] $window.Current.NativeWindowHandle
  if ($handle -eq [IntPtr]::Zero) { throw 'Native picker window handle unavailable' }
  # Retained UIA state and unrelated same-PID tray windows cannot establish
  # visibility. Keep querying the verified picker HWND's actual OS bit.
  $visible = [IbisSmoke]::IsWindowVisible($handle)
  if (!$visible) { return @{nodes=@();visible=$false;foreground=$false;picker_handle=$handle.ToInt64()} }
  $elements = $window.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
  if ($elements.Count -gt 4000) { throw 'Accessibility tree exceeded smoke traversal bound' }
  $nodes = @()
  foreach ($element in $elements) {
    $current = $element.Current
    $value = ''
    $pattern = $null
    if ($element.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref] $pattern)) { $value = $pattern.Current.Value }
    elseif ($element.TryGetCurrentPattern([System.Windows.Automation.TextPattern]::Pattern, [ref] $pattern)) {
      # Root documents can include results, recovery paragraphs and the footer.
      # A silent 500-character prefix can omit the very message being asserted.
      $value = $pattern.DocumentRange.GetText(16385)
      if ($value.Length -gt 16384) { throw 'Native accessibility document exceeded smoke text bound' }
    }
    $nodes += @{role=$current.ControlType.ProgrammaticName;name=$current.Name;value=$value;focused=$current.HasKeyboardFocus}
  }
  @{nodes=$nodes;visible=$visible;foreground=([IbisSmoke]::ForegroundPid() -eq $AppId);picker_handle=$handle.ToInt64()}
}
function Send-Chord([string] $Chord) {
  $map = @{ctrl=0x11;shift=0x10;alt=0x12;super=0x5b;space=0x20;escape=0x1b;return=0x0d}
  $keys = @($Chord.ToLowerInvariant().Split('+') | ForEach-Object {
    if ($map.ContainsKey($_)) { [ushort] $map[$_] }
    elseif ($_.Length -eq 1 -and $_ -cmatch '[a-z]') { [ushort][char] $_.ToUpperInvariant() }
    else { throw 'Unsupported smoke input key' }
  })
  foreach ($key in $keys) { [IbisSmoke]::Key($key, 0, 0) }
  [array]::Reverse($keys)
  foreach ($key in $keys) { [IbisSmoke]::Key($key, 0, 2) }
}
function Get-OwnedEditor {
  $editor = Get-Process -Id $EditorId -ErrorAction SilentlyContinue
  if (!$editor) { return $null }
  if ($editor.ProcessName -ne 'Notepad' -or !$Text -or
      $editor.StartTime.ToUniversalTime().Ticks -ne [long]::Parse($Text, [System.Globalization.CultureInfo]::InvariantCulture)) {
    throw 'Synthetic editor process identity was lost'
  }
  return $editor
}
try {
  [Console]::Error.WriteLine('NATIVE HELPER STAGE windows-dispatch')
  $result = switch ($Action) {
    'probe' { if (![Environment]::UserInteractive) { throw 'No interactive desktop' }; $true }
    'config-dir' {
      $directory = [Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData)
      if (!$directory) { throw 'Native configuration directory unavailable' }
      $directory
    }
    'editor' {
      if (Get-Process Notepad -ErrorAction SilentlyContinue) { throw 'Use a disposable desktop without any existing Notepad process' }
      $baseline = [System.Collections.Generic.HashSet[int]]::new()
      foreach ($process in Get-Process -ErrorAction Stop) { [void] $baseline.Add([int] $process.Id) }
      $editor = Start-Process notepad.exe -PassThru
      $deadline = [DateTime]::UtcNow.AddSeconds(15)
      do {
        Start-Sleep -Milliseconds 100
        # Windows 11 Notepad can redirect the launcher to a packaged process.
        $actual = Get-Process Notepad -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
      } while (!$actual -and [DateTime]::UtcNow -lt $deadline)
      if (!$actual) { throw 'Notepad has no interactive window' }
      if ($baseline.Contains([int] $actual.Id)) { throw 'Editor activation selected a preexisting process' }
      @{pid=[int] $actual.Id;started=[long] $actual.StartTime.ToUniversalTime().Ticks}
    }
    'snapshot' { Get-Tree }
    'key' { Send-Chord $Text; $true }
    'text' {
      foreach ($character in $Text.ToCharArray()) {
        [IbisSmoke]::Key(0, [ushort] $character, 4)
        [IbisSmoke]::Key(0, [ushort] $character, 6)
      }
      $true
    }
    'click' {
      $window = Get-AppWindow $AppId
      if (!$window) { throw 'Native picker window unavailable' }
      $condition = [System.Windows.Automation.PropertyCondition]::new([System.Windows.Automation.AutomationElement]::NameProperty, $Text)
      $element = $window.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $condition)
      if (!$element) { throw 'Named accessibility control missing' }
      # Chromium gives menu buttons (aria-haspopup) ExpandCollapse instead of Invoke.
      $pattern = $null
      if ($element.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref] $pattern)) {
        $pattern.Invoke()
      } elseif ($element.TryGetCurrentPattern([System.Windows.Automation.ExpandCollapsePattern]::Pattern, [ref] $pattern)) {
        $pattern.Expand()
      } else { throw 'Named accessibility control cannot be pressed' }
      $true
    }
    'focus-editor' {
      $facts = @{stage='identity';owned=$false;window_valid=$false;request_accepted=$false;current_foreground=$false}
      try {
        $editor = Get-OwnedEditor
        if (!$editor) { throw 'Synthetic editor process exited' }
        $facts.owned = $true
        $facts.stage = 'window'
        $handle = [IntPtr] $editor.MainWindowHandle
        if ($handle.ToInt64() -le 0 -or ![IbisSmoke]::IsWindow($handle) -or [IbisSmoke]::OwnerPid($handle) -ne $EditorId) {
          throw 'Synthetic editor window identity was lost'
        }
        $facts.window_valid = $true
        $facts.stage = 'foreground-request'
        $facts.current_foreground = [IbisSmoke]::ForegroundPid() -eq $EditorId
        if (!$facts.current_foreground) {
          $facts.request_accepted = [IbisSmoke]::SetForegroundWindow($handle)
        }
        $facts.current_foreground = [IbisSmoke]::ForegroundPid() -eq $EditorId
        # A declined request is not a focus verdict. Python independently
        # waits for fresh editor-focused observations before proceeding.
        @{owned=$facts.owned;window_valid=$facts.window_valid;request_accepted=$facts.request_accepted;current_foreground=$facts.current_foreground}
      } finally {
        [Console]::Error.WriteLine('NATIVE EDITOR FOCUS ' + (ConvertTo-Json -InputObject $facts -Compress))
      }
    }
    'editor-focused' { $null -ne (Get-OwnedEditor) -and [IbisSmoke]::ForegroundPid() -eq $EditorId }
    'clipboard' { Get-Clipboard -Raw }
    'write-clipboard' { Set-Clipboard -Value $Text; $true }
    'memory' {
      $all = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
      $ids = [System.Collections.Generic.HashSet[int]]::new()
      [void] $ids.Add($AppId)
      do {
        $added = $false
        foreach ($process in $all) {
          if ($ids.Contains([int] $process.ParentProcessId) -and $ids.Add([int] $process.ProcessId)) { $added = $true }
        }
      } while ($added)
      [long] (@(Get-Process -Id @($ids) -ErrorAction Stop) | Measure-Object WorkingSet64 -Sum).Sum
    }
    'close-editor' {
      $editor = Get-OwnedEditor
      if ($editor) { Stop-Process -Id $editor.Id -ErrorAction Stop }
      $true
    }
    default { throw 'Unknown smoke action' }
  }
  ConvertTo-Json -InputObject $result -Depth 10 -Compress
} catch {
  # Never serialize exception text: UIA exceptions can contain accessible text.
  [Console]::Error.WriteLine("Windows native smoke action unavailable: $Action")
  exit 1
}
