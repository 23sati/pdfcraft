<#
.SYNOPSIS
  Explains why the File Explorer preview of PDFs is blank, keeps loading, or shows
  "This file can't be previewed because of an error in the Linkco PDF Preview Handler".

.DESCRIPTION
  Finds the LinkcoPdfPreviewHandler.dll that Explorer actually uses (HKCU overrides HKLM), then prints
  [LinkcoPdfPreview.LinkcoPdfPreviewHandler]::Diagnose(): the registration, which handler Explorer
  picks for .pdf, a COM activation test (as the preview host does it), an engine self-test, the tail of
  preview.log and recent preview-host crashes from the Windows Application log. With -Pdf it also
  explains the security zone / Mark of the Web situation for that file.

  With -Repair it first fixes what it can for the current user: unblocks the DLL and pdfcraft-cli.exe,
  re-registers the handler (per user, with its own preview host process) for the installed DLL, and
  stops running preview hosts so Explorer loads the fresh registration; then it prints the diagnosis.

  Needs Windows PowerShell 5.1 (powershell.exe), not PowerShell 7: the handler is a .NET Framework DLL.
  No administrator rights needed. Without -Repair it changes nothing.

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File packaging\windows\diagnose-preview.ps1 -Pdf "C:\Users\me\Documents\file.pdf"

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File packaging\windows\diagnose-preview.ps1 -Repair
#>
param(
  [string] $Pdf = "",
  [string] $Dll = "",
  [switch] $Repair
)
$ErrorActionPreference = 'Stop'

if ($PSVersionTable.PSEdition -eq 'Core') {
  Write-Output "Run this with Windows PowerShell 5.1 (powershell.exe), not PowerShell 7 (pwsh)."
  exit 1
}

function Get-RegisteredDll {
  $clsid = '{D4E7B6A2-4C91-4E3A-9B12-7A8F5C3E1D20}'
  foreach ($hive in 'HKCU:', 'HKLM:') {
    $key = "$hive\Software\Classes\CLSID\$clsid\InprocServer32"
    $codeBase = (Get-ItemProperty -LiteralPath $key -Name CodeBase -ErrorAction SilentlyContinue).CodeBase
    if ($codeBase) {
      # .NET loads CodeBase without percent-decoding it, so "Program%20Files" (written before 1.0.0's
      # fix) points at a folder that does not exist and every preview fails with 0x80070002.
      $path = $codeBase -replace '^file:///', '' -replace '^file:', '' -replace '/', '\'
      $escaped = $codeBase -match '%[0-9A-Fa-f]{2}'
      return [pscustomobject]@{ Hive = $hive.TrimEnd(':'); Path = $path; Escaped = $escaped; Decoded = [Uri]::UnescapeDataString($path) }
    }
  }
  return $null
}

$registered = Get-RegisteredDll
$installed = @(
  "$env:ProgramFiles\Linkco\Linkco PDF Editor\LinkcoPdfPreviewHandler.dll",
  "$env:LOCALAPPDATA\Programs\Linkco\Linkco PDF Editor\LinkcoPdfPreviewHandler.dll")
if (-not $Dll -and $Repair) {
  # Repair against the installed copy, not whatever (possibly stale) copy is registered.
  foreach ($candidate in $installed) {
    if (Test-Path -LiteralPath $candidate) { $Dll = $candidate; break }
  }
}
if (-not $Dll) {
  if ($registered -and (Test-Path -LiteralPath $registered.Path)) {
    $Dll = $registered.Path
  } elseif ($registered -and $registered.Escaped -and (Test-Path -LiteralPath $registered.Decoded)) {
    $Dll = $registered.Decoded
  } else {
    foreach ($candidate in $installed) {
      if (Test-Path -LiteralPath $candidate) { $Dll = $candidate; break }
    }
  }
}

Write-Output "Windows:          $([Environment]::OSVersion.VersionString) ($((Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion' -ErrorAction SilentlyContinue).DisplayVersion))"
if ($registered) {
  $state = if (Test-Path -LiteralPath $registered.Path) { 'exists' }
    elseif ($registered.Escaped) { 'BROKEN - percent-escaped CodeBase, .NET cannot find the DLL; run this script with -Repair, or start Linkco PDF Editor once' }
    else { 'MISSING - Explorer cannot load the preview handler' }
  Write-Output "Registered DLL:   $($registered.Hive) -> $($registered.Path) ($state)"
} else {
  Write-Output "Registered DLL:   (the preview handler is not registered - start Linkco PDF Editor once, or reinstall)"
}
if (-not $Dll -or -not (Test-Path -LiteralPath $Dll)) {
  Write-Output "LinkcoPdfPreviewHandler.dll was not found. Reinstall Linkco PDF Editor, or pass -Dll <path>."
  exit 1
}
Write-Output "Using DLL:        $Dll"
Write-Output ""

# Load from bytes so this script does not lock the DLL. (The COM activation test inside Diagnose()
# loads the registered DLL the way Explorer does; the lock ends when this script exits.)
$type = [Reflection.Assembly]::Load([IO.File]::ReadAllBytes($Dll)).GetType('LinkcoPdfPreview.LinkcoPdfPreviewHandler')

if ($Repair) {
  Write-Output "Repairing for $env:USERNAME ..."
  foreach ($file in @($Dll, (Join-Path (Split-Path -Parent $Dll) 'pdfcraft-cli.exe'))) {
    if (Test-Path -LiteralPath $file) { Unblock-File -LiteralPath $file -ErrorAction SilentlyContinue }
  }
  try {
    $type::RegisterPreviewHandler($Dll, $false)
    Write-Output "  Registered $Dll for this user (own preview host process)."
  } catch {
    Write-Output "  Registration failed: $($_.Exception.Message)"
  }
  Get-Process -Name prevhost -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Write-Output "  Stopped running preview hosts; select the PDF in Explorer again."
  Write-Output ""
}

Write-Output ($type::Diagnose())

if ($Pdf) {
  Write-Output ""
  Write-Output ($type::DiagnoseFile($Pdf))
}
