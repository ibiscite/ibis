# The Windows eyes of smoke-native.py --journey: one UI Automation look at
# Ibis's own windows, one JSON line out. It reads only the windows of the one
# process it is given; Notepad, keys and the clipboard are Win32 calls in Python.
param([Parameter(Mandatory)][string]$Command, [Parameter(Mandatory)][int]$Id)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
try {
  if ($Command -cne 'snapshot' -or $Id -le 0) { throw 'Unknown command' }
  Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
  $A = [System.Windows.Automation.AutomationElement]
  $S = [System.Windows.Automation.TreeScope]
  $condition = [System.Windows.Automation.PropertyCondition]::new($A::ProcessIdProperty, $Id)
  $windows = @($A::RootElement.FindAll($S::Children, $condition))
  if ($windows.Count -gt 20) { throw 'Too many windows' }
  $answer = [ordered]@{ windows = @(); searches = 0; focused = $false; value = $null; strings = @() }
  $strings = [System.Collections.Generic.List[string]]::new()
  foreach ($window in $windows) {
    if ($window.Current.ProcessId -ne $Id -or $window.Current.IsOffscreen) { continue }
    $title = $window.Current.Name
    $answer.windows += $title
    # Elements inside the web view belong to its renderer process, so they are
    # scoped by the owned window they are found in, as update-e2e.py does.
    $items = @($window.FindAll($S::Descendants, [System.Windows.Automation.Condition]::TrueCondition))
    if ($items.Count -gt 4000) { throw 'Too many elements' }
    foreach ($item in $items) {
      $name = $item.Current.Name
      if ($name -and $strings.Count -lt 400) { $strings.Add($name) }
      if ($title -ceq 'Ibis' -and $name -ceq 'Search references' -and
          $item.Current.ControlType.ProgrammaticName -cin @('ControlType.ComboBox', 'ControlType.Edit')) {
        $answer.searches++
        if ($item.Current.HasKeyboardFocus) {
          $answer.focused = $true
          $pattern = $null
          if ($item.TryGetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern, [ref]$pattern)) {
            $answer.value = $pattern.Current.Value
          }
        }
      }
    }
  }
  $answer.strings = @($strings)
  $answer | ConvertTo-Json -Compress -Depth 3
} catch {
  # Only the exception's type name: its message can name paths or windows.
  @{ error = $_.Exception.GetType().Name } | ConvertTo-Json -Compress
  exit 1
}
