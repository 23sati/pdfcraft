# Internal child of test-native-dialogs.ps1. Authoring preview only: this never
# opens an installation session, executes a sequence or activates any control.
# Windows PowerShell supplies the .NET Framework STA message pump.
param([Parameter(Mandatory)] [string] $Msi,
      [Parameter(Mandatory)] [string] $Dialog,
      [Parameter(Mandatory)] [string] $StopFile)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
  throw 'Native MSI UI previews are restricted to isolated GitHub-hosted runners.'
}
if ([Threading.Thread]::CurrentThread.GetApartmentState() -ne 'STA') { throw 'Preview requires STA' }
Add-Type -AssemblyName System.Windows.Forms
$installer = $null
$database = $null
$preview = $null
try {
  $installer = New-Object -ComObject WindowsInstaller.Installer
  $database = $installer.OpenDatabase((Resolve-Path -LiteralPath $Msi).Path, 0)
  $preview = $database.EnableUIPreview()
  [void] $preview.ViewDialog($Dialog)
  $deadline = [DateTime]::UtcNow.AddSeconds(40)
  while (-not (Test-Path -LiteralPath $StopFile)) {
    if ([DateTime]::UtcNow -ge $deadline) { throw 'Native MSI preview exceeded its 40-second deadline' }
    [Windows.Forms.Application]::DoEvents()
    Start-Sleep -Milliseconds 25
  }
} finally {
  # Closing the preview releases only authoring windows; it does not trigger an
  # MSI button event or an installation/custom action.
  foreach ($item in @($preview, $database, $installer)) {
    if ($null -ne $item) { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($item) }
  }
}
