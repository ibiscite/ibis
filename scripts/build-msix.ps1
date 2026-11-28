# Preview's Store package step (preview.yml), from the repository root, after
# the Windows packages are built: the NSIS installer's files, Store images and
# apps/desktop/src-tauri/packaging/msix/AppxManifest.xml in one unsigned MSIX,
# store/Ibis-Windows.msix. The Store signs what Partner Center receives.
param(
  [Parameter(Mandatory)][string]$Target,
  # The release version, such as 0.4.0 or 0.4.1-nightly.20261128000000.
  [Parameter(Mandatory)][string]$Version,
  [Parameter(Mandatory)][ValidateSet('Ibis', 'Ibis Nightly')][string]$Product
)
$ErrorActionPreference = 'Stop'

# The product identity Partner Center reserved for the name Ibis
# (Product management > Product identity). Nightly is never listed.
$Identity = @{
  Name = 'Ibiscite.Ibis'
  Publisher = 'CN=Ibiscite'
  PublisherName = 'Ibiscite'
}

function Get-SdkTool([string]$Name) {
  $tool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\$Name" |
    Sort-Object { [version]$_.Directory.Parent.Name } | Select-Object -Last 1
  if (-not $tool) { throw "The Windows SDK's $Name was not found." }
  $tool.FullName
}

function Invoke-Tool([string]$Tool, [string[]]$Arguments) {
  & $Tool @Arguments
  if ($LASTEXITCODE -ne 0) { throw "$(Split-Path -Leaf $Tool) failed with exit code $LASTEXITCODE." }
}

# The Store takes Major.Minor.Build.0; a nightly keeps its numbered version.
if (-not ($Version -match '^(\d+)\.(\d+)\.(\d+)(-.*)?$')) { throw "Version $Version is not X.Y.Z." }
$packageVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3]).0"
$name = if ($Product -eq 'Ibis') { $Identity.Name } else { "$($Identity.Name)Nightly" }

$setup = Get-ChildItem "target/$Target/release/bundle/nsis/*-setup.exe" | Select-Object -First 1
if (-not $setup) { throw "No NSIS installer in target/$Target/release/bundle/nsis." }
$stage = Join-Path $env:RUNNER_TEMP 'msix'
Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
& 7z x -y "-o$stage" $setup.FullName | Out-Null
if ($LASTEXITCODE -ne 0) { throw '7-Zip could not open the installer.' }
if (Test-Path "$stage/`$INSTDIR") { $stage = Join-Path $stage '$INSTDIR' }
# What only the installer uses: its plug-ins, uninstaller and PATH helper.
Get-ChildItem $stage -Force | Where-Object { $_.Name -like '$*' -or $_.Name -in 'uninstall.exe', 'packaging' } |
  Remove-Item -Recurse -Force
foreach ($file in 'ibis-desktop.exe', 'ibis.exe', 'resources/samples.csljson', 'notices/LICENSE') {
  if (-not (Test-Path (Join-Path $stage $file))) { throw "The installer holds no $file." }
}

$source = 'apps/desktop/src-tauri'
Copy-Item "$source/icons/windows" (Join-Path $stage 'Assets') -Recurse
$manifest = (Get-Content "$source/packaging/msix/AppxManifest.xml" -Raw).
  Replace('{{name}}', $name).
  Replace('{{publisher}}', $Identity.Publisher).
  Replace('{{publisher_name}}', $Identity.PublisherName).
  Replace('{{version}}', $packageVersion).
  Replace('{{product}}', $Product)
if ($manifest -match '\{\{') { throw 'The manifest still has a value to fill.' }
Set-Content (Join-Path $stage 'AppxManifest.xml') $manifest -Encoding utf8NoBOM

# Index the scale and target-size images, so Windows picks the right one.
$makepri = Get-SdkTool 'makepri.exe'
$config = Join-Path $env:RUNNER_TEMP 'priconfig.xml'
Invoke-Tool $makepri @('createconfig', '/cf', $config, '/dq', 'en-US', '/pv', '10.0.0', '/o')
# One package holds every image: no separate resource packs per scale.
[xml]$pri = Get-Content $config -Raw
$pri.resources.packaging | ForEach-Object { $_.ParentNode.RemoveChild($_) | Out-Null }
$pri.Save($config)
Invoke-Tool $makepri @('new', '/pr', $stage, '/cf', $config, '/of', (Join-Path $stage 'resources.pri'), '/o')

New-Item -ItemType Directory -Force store | Out-Null
Invoke-Tool (Get-SdkTool 'makeappx.exe') @('pack', '/d', $stage, '/p', 'store/Ibis-Windows.msix', '/o')
"Built store/Ibis-Windows.msix: $name $packageVersion"
