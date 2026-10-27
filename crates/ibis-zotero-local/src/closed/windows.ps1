# Runs only with Windows PowerShell 5.1's .NET Framework APIs. Never print paths,
# exceptions or library data. Paths arrive as values, never executable text.
# Fixed protocol bytes let Rust distinguish process startup from script/action
# execution without retaining native exception text or any path.
[Console]::Out.Write([char] 1)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

function Assert-Drive([string] $Path) {
    if ($Path -notmatch '^[A-Za-z]:[\\/]') { throw 'not local' }
    $root = $Path.Substring(0, 2) + '\'
    # DriveType queries the drive root before GetAttributes could touch a
    # disconnected mapped share. Unknown, unavailable and remote drives fail.
    $kind = [System.IO.DriveInfo]::new($root).DriveType
    if ($kind -notin @('Fixed', 'Removable', 'CDRom', 'Ram')) { throw 'not local' }
    return $root
}

function Assert-Local([string] $Path) {
    $root = Assert-Drive $Path
    $parts = $Path.Substring(3).Split([char[]] '\/', [System.StringSplitOptions]::RemoveEmptyEntries)
    $current = $root
    foreach ($part in @('') + $parts) {
        if ($part -ne '') {
            if ($part -in @('.', '..') -or $part -match '[:\x00-\x1f]' -or $part.EndsWith('.') -or $part.EndsWith(' ')) { throw 'invalid path' }
            $current = [System.IO.Path]::Combine($current, $part)
        }
        $attributes = [System.IO.File]::GetAttributes($current)
        # Reject the ancestor itself before probing anything underneath it.
        # Also reject dehydrated cloud files rather than recalling their data.
        if (([int] $attributes -band 0x00441400) -ne 0) { throw 'not local' }
    }
}

function Assert-Private([string] $Path, [bool] $Protected) {
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
    $sections = [System.Security.AccessControl.AccessControlSections] 'Access, Owner'
    $attributes = [System.IO.File]::GetAttributes($Path)
    if (([int] $attributes -band 0x400) -ne 0) { throw 'reparse point' }
    if (($attributes -band [System.IO.FileAttributes]::Directory) -ne 0) {
        $acl = [System.IO.Directory]::GetAccessControl($Path, $sections)
    } else {
        $acl = [System.IO.File]::GetAccessControl($Path, $sections)
    }
    if (-not $acl.GetOwner([System.Security.Principal.SecurityIdentifier]).Equals($sid)) { throw 'wrong owner' }
    if ($Protected -and -not $acl.AreAccessRulesProtected) { throw 'inherited access' }
    $rules = @($acl.GetAccessRules($true, $true, [System.Security.Principal.SecurityIdentifier]))
    if ($rules.Count -ne 1) { throw 'unexpected access' }
    $rule = $rules[0]
    if (-not $rule.IdentityReference.Equals($sid) -or $rule.AccessControlType -ne 'Allow' -or $rule.FileSystemRights -ne 'FullControl') { throw 'unexpected access' }
}

$staging = $null
$owned = $false
$success = $false
try {
    $path = $env:IBIS_CLOSED_PATH
    [Console]::Out.Write([char] 2)
    switch ($env:IBIS_CLOSED_ACTION) {
        'closed' {
            $all = @(Get-Process -ErrorAction Stop)
            if ($all.Count -eq 0 -or @($all | Where-Object { $_.Id -eq $PID }).Count -ne 1) { throw 'process check' }
            if (@($all | Where-Object { $_.ProcessName -imatch '^zotero$' }).Count -gt 0) {
                [Console]::Out.Write('running')
            } else { [Console]::Out.Write('closed') }
        }
        'space' {
            Assert-Local $path
            $root = Assert-Drive $path
            if ([System.IO.DriveInfo]::new($root).AvailableFreeSpace -lt [UInt64]::Parse($env:IBIS_CLOSED_SPACE)) { throw 'space' }
        }
        'backup' {
            Assert-Local $path
            # Windows Backup excludes %TEMP%\* /s by default. Require the
            # actual user Temp beneath LocalAppData rather than an arbitrary
            # caller-supplied/environment-redirected cache path.
            # .NET Framework's GetFullPath expands existing 8.3 components;
            # normalize all three values without launching a C# compiler.
            function Long-Path([string] $Value) {
                return [System.IO.Path]::GetFullPath($Value).TrimEnd('\')
            }
            $local = [Environment]::GetFolderPath('LocalApplicationData')
            $temp = Long-Path ([System.IO.Path]::Combine($local, 'Temp'))
            $configured = Long-Path ([System.IO.Path]::GetTempPath())
            $expanded = Long-Path $path
            if (-not $temp.Equals($configured, [StringComparison]::OrdinalIgnoreCase)) { throw 'temp redirected' }
            if (-not $expanded.StartsWith($temp + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'not temp' }
            [System.IO.File]::SetAttributes($path, [System.IO.File]::GetAttributes($path) -bor [System.IO.FileAttributes]::NotContentIndexed)
        }
        'local' { Assert-Local $path }
        'verify' {
            Assert-Local $path
            Assert-Private $path $false
        }
        'private' {
            Assert-Local $path
            Assert-Private $path $true
        }
        'create' {
            $parent = [System.IO.Path]::GetDirectoryName($path)
            Assert-Local $parent
            if ([System.IO.Directory]::Exists($path) -or [System.IO.File]::Exists($path)) { throw 'already exists' }
            $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
            $acl = [System.Security.AccessControl.DirectorySecurity]::new()
            $acl.SetAccessRuleProtection($true, $false)
            $acl.SetOwner($sid)
            $rule = [System.Security.AccessControl.FileSystemAccessRule]::new($sid, 'FullControl', 'ContainerInherit, ObjectInherit', 'None', 'Allow')
            $acl.AddAccessRule($rule)
            $staging = [System.IO.Path]::Combine($parent, 'ibis-zotero-private-' + [System.Guid]::NewGuid().ToString('N'))
            # The ACL is part of directory creation, before any content exists.
            [void] [System.IO.Directory]::CreateDirectory($staging, $acl)
            Assert-Private $staging $true
            $owned = $true
            # Rename in the same directory refuses an existing destination.
            [System.IO.Directory]::Move($staging, $path)
            $staging = $null
            Assert-Local $path
            Assert-Private $path $true
        }
        default { throw 'unknown action' }
    }
    $success = $true
} catch {
    # The Rust caller reports fixed text; native errors contain private paths.
} finally {
    if ($owned -and $null -ne $staging) {
        try { [System.IO.Directory]::Delete($staging, $false) } catch { }
    }
}
if ($success) { exit 0 }
exit 1
