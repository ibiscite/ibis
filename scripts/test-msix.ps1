# Preview's Store package check (preview.yml), from the repository root, after
# scripts/build-msix.ps1: install a signed copy of store/Ibis-Windows.msix the
# way the Store would, run the ibis command through its alias, start the app
# from Start's entry and check it runs from the package, then remove it all.
# The throwaway certificate stands in for the Store's signature; the kept
# package stays unsigned.
param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'

function Get-SdkTool([string]$Name) {
  $tool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\10.*\x64\$Name" |
    Sort-Object { [version]$_.Directory.Parent.Name } | Select-Object -Last 1
  if (-not $tool) { throw "The Windows SDK's $Name was not found." }
  $tool.FullName
}

$work = Join-Path $env:RUNNER_TEMP 'msix-test'
New-Item -ItemType Directory -Force $work | Out-Null
$package = Join-Path $work 'Ibis.msix'
Copy-Item store/Ibis-Windows.msix $package

# The certificate's subject must be the manifest's Publisher.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [IO.Compression.ZipFile]::OpenRead($package)
try {
  $reader = [IO.StreamReader]::new($zip.GetEntry('AppxManifest.xml').Open())
  [xml]$manifest = $reader.ReadToEnd()
  $reader.Dispose()
} finally { $zip.Dispose() }
$publisher = $manifest.Package.Identity.Publisher
$certificate = New-SelfSignedCertificate -Type Custom -Subject $publisher -KeyUsage DigitalSignature `
  -CertStoreLocation Cert:\CurrentUser\My -FriendlyName 'Ibis package check' `
  -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3', '2.5.29.19={text}')
$trusted = $null
$installed = $null
try {
  & (Get-SdkTool 'signtool.exe') sign /fd SHA256 /sha1 $certificate.Thumbprint $package
  if ($LASTEXITCODE -ne 0) { throw 'signtool could not sign the package.' }
  $cer = Join-Path $work 'check.cer'
  Export-Certificate -Cert $certificate -FilePath $cer | Out-Null
  $trusted = Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\TrustedPeople

  Add-AppxPackage -Path $package
  $installed = Get-AppxPackage -Name $manifest.Package.Identity.Name
  if (-not $installed) { throw 'The package did not install.' }
  "Installed $($installed.PackageFullName)"

  # The alias Windows adds for the package, where Store installs put it on PATH.
  $alias = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\ibis.exe'
  $said = & $alias --version
  if ($LASTEXITCODE -ne 0 -or "$said" -notlike "*$Version*") { throw "The ibis alias said '$said', not $Version." }
  "The ibis alias: $said"

  # Start's entry, as a person opens it; a login start has its own task.
  Start-Process explorer.exe "shell:AppsFolder\$($installed.PackageFamilyName)!Ibis"
  $deadline = (Get-Date).AddSeconds(60)
  do {
    Start-Sleep -Seconds 2
    $app = Get-Process ibis-desktop -ErrorAction SilentlyContinue |
      Where-Object { $_.Path -like "$($installed.InstallLocation)\*" } | Select-Object -First 1
  } until ($app -or (Get-Date) -gt $deadline)
  if (-not $app) { throw 'The app did not start from its package.' }
  Start-Sleep -Seconds 10
  if ($app.HasExited) { throw "The app exited with code $($app.ExitCode)." }
  "The app runs from the package: $($app.Path)"
  Stop-Process -Id $app.Id -Force
} finally {
  if ($installed) { Remove-AppxPackage -Package $installed.PackageFullName }
  if ($trusted) { Remove-Item "Cert:\LocalMachine\TrustedPeople\$($trusted.Thumbprint)" }
  Remove-Item "Cert:\CurrentUser\My\$($certificate.Thumbprint)"
}
