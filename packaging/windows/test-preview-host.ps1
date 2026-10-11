<#
.SYNOPSIS
  End-to-end test of the File Explorer PDF preview handler on a real Windows machine: builds
  LinkcoPdfPreviewHandler.dll, registers it the way the app and the installers do, and hosts it the
  way File Explorer does (out of process through prevhost.exe), reporting every COM call.

.DESCRIPTION
  Scenarios (each starts from a clean registration and no running preview host):
    A. per-user registration (what Linkco PDF Editor writes at start-up), hosted out of process
    B. the same, hosted in process (separates .NET/assembly loading problems from surrogate problems)
    C. per-user registration with Windows' shared preview host AppID (the registration before 1.0.0)
    D. machine-wide registration only (what the installers write), hosted out of process
  Then prints Diagnose(), preview.log and recent crash events. Writes everything to -Report.

  Needs Windows PowerShell 5.1 and the .NET Framework 4.x csc.exe that ships with Windows. Changes the
  current user's (and, for D, the machine's) preview handler registration; meant for test machines
  and CI. Pass -Cli <pdfcraft-cli.exe> to render real pages; without it the pane shows that the engine
  is missing (which still exercises every COM call).

.EXAMPLE
  powershell -NoProfile -ExecutionPolicy Bypass -File packaging\windows\test-preview-host.ps1 -Cli target\release\pdfcraft-cli.exe
#>
param(
  [string] $Cli = "",
  [int] $Seconds = 15,
  [string] $Report = "",
  [switch] $NoDemote
)
$ErrorActionPreference = 'Stop'
$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $Report) { $Report = Join-Path $Root 'preview-host-report.txt' }
$Clsid = '{D4E7B6A2-4C91-4E3A-9B12-7A8F5C3E1D20}'
$SharedHost = '{6d2b5079-2f0b-48dd-ab7f-97cec514d30b}'
$Started = Get-Date
$lines = New-Object System.Collections.Generic.List[string]
function Say([string] $text) { foreach ($l in ($text -split "`r?`n")) { $lines.Add($l); Write-Output $l } }

trap {
  Say "ERROR: $($_.Exception.Message)"
  Say "  at $($_.InvocationInfo.PositionMessage)"
  [IO.File]::WriteAllLines($Report, $lines)
  exit 1
}

$Work = Join-Path ([IO.Path]::GetTempPath()) ('linkco-host-' + [Guid]::NewGuid().ToString('N').Substring(0, 8))
$IsAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
# Where the installers put it (Program Files) when we may write there; else a temp folder.
$App = if ($IsAdmin) { Join-Path $env:ProgramFiles 'Linkco Preview Test\Linkco PDF Editor' } else { Join-Path $Work 'Linkco PDF Editor' }
if (Test-Path $App) { Remove-Item $App -Recurse -Force }
New-Item -ItemType Directory -Force -Path $Work, $App | Out-Null

$Csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path $Csc)) { $Csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
$Dll = Join-Path $App 'LinkcoPdfPreviewHandler.dll'
& $Csc /nologo /target:library /optimize+ /platform:anycpu /codepage:65001 /warn:4 "/out:$Dll" `
  /r:System.dll /r:System.Drawing.dll /r:System.Windows.Forms.dll (Join-Path $PSScriptRoot 'PreviewHandler.cs') | ForEach-Object { Say "  csc: $_" }
if ($LASTEXITCODE -ne 0) { throw 'PreviewHandler.cs did not compile' }
$HostExe = Join-Path $Work 'PreviewHostTest.exe'
& $Csc /nologo /target:exe /platform:x64 /codepage:65001 "/out:$HostExe" /r:System.Windows.Forms.dll /r:System.Drawing.dll `
  (Join-Path $PSScriptRoot 'preview-host-test\PreviewHostTest.cs') | ForEach-Object { Say "  csc: $_" }
if ($LASTEXITCODE -ne 0) { throw 'PreviewHostTest.cs did not compile' }
if ($Cli) { Copy-Item -LiteralPath (Resolve-Path $Cli).Path -Destination (Join-Path $App 'pdfcraft-cli.exe') }

# A small one-page PDF with dark text on white, so "painted" can tell a rendered page from a message.
$body = 'BT /F1 36 Tf 30 700 Td (Linkco preview test) Tj ET 0 0 0 rg 30 100 540 500 re f'
$objs = @('<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
  '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
  "<< /Length $($body.Length) >>`nstream`n$body`nendstream", '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>')
$sb = New-Object System.Text.StringBuilder("%PDF-1.7`n"); $offsets = @()
for ($i = 0; $i -lt $objs.Count; $i++) { $offsets += $sb.Length; [void]$sb.Append("$($i + 1) 0 obj`n$($objs[$i])`nendobj`n") }
$xref = $sb.Length
[void]$sb.Append("xref`n0 $($objs.Count + 1)`n0000000000 65535 f `n")
foreach ($o in $offsets) { [void]$sb.Append(('{0:D10} 00000 n `n' -f $o)) }
[void]$sb.Append("trailer`n<< /Size $($objs.Count + 1) /Root 1 0 R >>`nstartxref`n$xref`n%%EOF`n")
$Pdf = Join-Path $Work 'sample.pdf'
[IO.File]::WriteAllText($Pdf, $sb.ToString(), [Text.Encoding]::ASCII)

$type = [Reflection.Assembly]::Load([IO.File]::ReadAllBytes($Dll)).GetType('LinkcoPdfPreview.LinkcoPdfPreviewHandler')

# .NET assembly binding log (Fusion), so a failed load says which file and where it looked.
$FusionDir = 'C:\LinkcoFusionLog'
if ($IsAdmin) {
  if (Test-Path $FusionDir) { Remove-Item $FusionDir -Recurse -Force }
  New-Item -ItemType Directory -Force -Path $FusionDir | Out-Null
  & icacls.exe $FusionDir /grant '*S-1-1-0:(OI)(CI)F' /setintegritylevel '(OI)(CI)low' | Out-Null
  $fk = 'HKLM:\SOFTWARE\Microsoft\Fusion'
  if (-not (Test-Path $fk)) { New-Item $fk -Force | Out-Null }
  Set-ItemProperty $fk -Name ForceLog -Value 1 -Type DWord
  Set-ItemProperty $fk -Name LogFailures -Value 1 -Type DWord
  Set-ItemProperty $fk -Name LogPath -Value ($FusionDir + '\') -Type String
}
function Show-FusionLog {
  if (-not (Test-Path $FusionDir)) { return }
  foreach ($f in @(Get-ChildItem $FusionDir -Recurse -Filter *.htm -ErrorAction SilentlyContinue | Sort-Object LastWriteTime)) {
    $t = (Get-Content -LiteralPath $f.FullName -Raw) -replace '<[^>]+>', '' -replace '&gt;', '>' -replace '&lt;', '<' -replace '&amp;', '&'
    $failed = $t -match 'Bind result: hr = 0x8'
    $ours = $f.Name -match 'LinkcoPdfPreview'
    if (-not $failed -and -not ($ours -and $f.Directory.Name -ne 'RegAsm.exe')) { continue }
    Say "  binding log $($f.Directory.Name)\$($f.Name)$(if ($failed) { '  [FAILED]' }):"
    foreach ($l in ($t -split "`r?`n" | Where-Object { $_.Trim() -and $_ -notmatch 'Native image|Using (host|machine|application) configuration|Assembly Binder Log Entry|A detailed error log' } | Select-Object -First 30)) { Say "    $($l.Trim())" }
  }
  Get-ChildItem $FusionDir -Recurse -File -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
}
function Internal([string] $method, [object[]] $arguments) {
  $m = $type.GetMethod($method, [Reflection.BindingFlags] 'NonPublic, Public, Static')
  try { return $m.Invoke($null, $arguments) } catch { return "($method failed: $($_.Exception.InnerException.Message))" }
}
$LogPath = Join-Path $env:USERPROFILE 'AppData\LocalLow\LinkcoPdfPreview\preview.log'

function Stop-Hosts {
  Get-Process -Name prevhost, dllhost -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Milliseconds 500
}
function Clear-Registration {
  try { $type::UnregisterPreviewHandler() } catch { Say "  (unregister: $($_.Exception.Message))" }
  foreach ($k in "HKCU:\Software\Classes\CLSID\$Clsid", 'HKCU:\Software\Classes\AppID\{1C0ADB7B-F017-4384-BF73-95AA5BC5D084}') {
    if (Test-Path $k) { Remove-Item $k -Recurse -Force }
  }
}
function Run-Host([string] $name, [string] $mode, [switch] $AsAdmin) {
  Stop-Hosts
  $logBefore = if (Test-Path $LogPath) { (Get-Item $LogPath).Length } else { 0 }
  Say ""
  Say "=== $name ==="
  Say ("  effective server: " + (Internal 'DescribeEffectiveServer' @($Clsid)))
  Say ("  preview host:     " + (Internal 'DescribeHostAppId' @()))
  $letter = $name.Substring(0, 1)
  $shot = Join-Path $Work "shot-$letter.png"
  $outFile = Join-Path $Work "out-$letter.txt"
  $code = $null
  if ($Elevated -and -not $NoDemote -and -not $AsAdmin) {
    # Explorer runs un-elevated (medium integrity), and COM ignores per-user registrations for
    # elevated callers: run the host as a normal user, like Explorer.
    $bat = Join-Path $Work "run-$letter.cmd"
    Set-Content -LiteralPath $bat -Encoding ASCII -Value @(
      '@echo off',
      ('"' + $HostExe + '" ' + $Clsid + ' "' + $Pdf + '" ' + $mode + ' ' + $Seconds + ' "' + $shot + '" > "' + $outFile + '" 2>&1'),
      ('>"' + $outFile + '.done" echo %ERRORLEVEL%'))
    $ErrorActionPreference = 'Continue'
    & runas.exe /trustlevel:0x20000 $bat 2>&1 | ForEach-Object { Say "  runas: $_" }
    $ErrorActionPreference = 'Stop'
    $deadline = (Get-Date).AddSeconds($Seconds + 120)
    while (-not (Test-Path "$outFile.done") -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 500 }
    if (Test-Path "$outFile.done") {
      $code = (Get-Content "$outFile.done" -Raw).Trim()
      Say "  (ran as a normal user: runas /trustlevel:0x20000)"
    } else {
      Say "  (runas /trustlevel did not run the host; running it elevated instead)"
    }
  }
  if ($null -eq $code) {
    $ErrorActionPreference = 'Continue'
    & $HostExe $Clsid $Pdf $mode $Seconds $shot 2>&1 | ForEach-Object { "$_" } | Set-Content -LiteralPath $outFile
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
  }
  if (Test-Path $outFile) { Say ((Get-Content -LiteralPath $outFile -Raw) -replace '\s+$', '') }
  Say "  exit code $code"
  Show-FusionLog
  if (Test-Path $LogPath) {
    $fs = [IO.File]::Open($LogPath, 'Open', 'Read', 'ReadWrite')
    try {
      if ($fs.Length -ge $logBefore) { [void]$fs.Seek($logBefore, 'Begin') }
      $new = (New-Object IO.StreamReader($fs)).ReadToEnd()
    } finally { $fs.Dispose() }
    Say "  preview.log during this scenario:"
    foreach ($l in ($new -split "`r?`n" | Where-Object { $_ } | Select-Object -Last 25)) { Say "    $l" }
  } else {
    Say "  preview.log: (none written - the handler code never ran)"
  }
}

$Elevated = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
Say "Test process elevated: $Elevated; $((whoami /groups | Select-String 'Mandatory Label') -replace '\s{2,}', ' ')"
Say "Windows $([Environment]::OSVersion.Version) $((Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion').DisplayVersion), .NET $([Environment]::Version), engine: $(if ($Cli) { 'pdfcraft-cli.exe' } else { 'none' })"

# A native COM client (as prevhost.exe is): VBScript CreateObject through IDispatch, 64-bit cscript.
Say "  prevhost.exe present: $(Test-Path (Join-Path $env:WINDIR 'System32\prevhost.exe'))"
$vbs = Join-Path $Work 'create.vbs'
Set-Content -LiteralPath $vbs -Encoding ASCII -Value @(
  'On Error Resume Next',
  'Set o = CreateObject("LinkcoPDFEditor.PreviewHandler")',
  'If Err.Number <> 0 Then WScript.Echo "CreateObject FAILED 0x" & Hex(Err.Number) & " " & Err.Description Else WScript.Echo "CreateObject OK: " & TypeName(o)')
function Try-Native([string] $label) {
  $ErrorActionPreference = 'Continue'
  $o = & (Join-Path $env:WINDIR 'System32\cscript.exe') //nologo $vbs 2>&1 | ForEach-Object { "$_" }
  $ErrorActionPreference = 'Stop'
  Say ("  {0,-44} {1}" -f $label, ($o -join ' '))
}
# Which CodeBase forms the CLR's COM activation accepts, from a native client (as prevhost.exe is).
Say ""
Say "=== CodeBase forms (native 64-bit cscript CreateObject, in process) ==="
function Try-CodeBase([string] $label, [string] $codeBase) {
  $k = "HKCU:\Software\Classes\CLSID\$Clsid\InprocServer32"
  Set-ItemProperty $k -Name CodeBase -Value $codeBase
  Get-ChildItem $k | ForEach-Object { Set-ItemProperty $_.PSPath -Name CodeBase -Value $codeBase }
  $ErrorActionPreference = 'Continue'
  $o = & (Join-Path $env:WINDIR 'System32\cscript.exe') //nologo $vbs 2>&1 | ForEach-Object { "$_" }
  $ErrorActionPreference = 'Stop'
  Say ("  {0,-34} {1}" -f $label, $codeBase)
  Say ("  {0,-34} -> {1}" -f '', ($o -join ' '))
}
Clear-Registration
$type::RegisterPreviewHandler($Dll, $false)
$registeredCodeBase = (Get-ItemProperty "HKCU:\Software\Classes\CLSID\$Clsid\InprocServer32").CodeBase
Try-CodeBase 'as registered by this build' $registeredCodeBase
Try-CodeBase 'escaped (Uri.AbsoluteUri, old)' (New-Object Uri($Dll)).AbsoluteUri
Try-CodeBase 'file:/// + backslashes (NSIS/WiX)' ('file:///' + $Dll)
Try-CodeBase 'plain path' $Dll

Say ""
Say "=== Native client (64-bit cscript CreateObject) ==="
Clear-Registration
$type::RegisterPreviewHandler($Dll, $false)
Try-Native 'our per-user registration:'
Clear-Registration
$type::RegisterPreviewHandler($Dll, $true)
Try-Native 'our per-user + machine-wide registration:'
Clear-Registration
$regasm = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\RegAsm.exe'
$ErrorActionPreference = 'Continue'
& $regasm /nologo /codebase $Dll 2>&1 | ForEach-Object { Say "  regasm: $_" }
$ErrorActionPreference = 'Stop'
Try-Native 'RegAsm /codebase registration (HKLM):'
$ErrorActionPreference = 'Continue'
$o = (& $HostExe $Clsid $Pdf 'inproc' 1 2>&1 | ForEach-Object { "$_" } | Select-String 'CoCreateInstance:' | Select-Object -First 1)
$ErrorActionPreference = 'Stop'
Say "  managed host, RegAsm registration, in process: $("$o".Trim())"
Say "  RegAsm wrote:"
& reg.exe query "HKLM\Software\Classes\CLSID\$Clsid" /s | ForEach-Object { if ("$_".Trim()) { Say "    $_" } }
$ErrorActionPreference = 'Continue'
& $regasm /nologo /unregister $Dll 2>&1 | Out-Null
$ErrorActionPreference = 'Stop'
Say "  binding log files:"
Get-ChildItem $FusionDir -Recurse -File -ErrorAction SilentlyContinue | Select-Object -First 12 | ForEach-Object { Say "    $($_.FullName.Substring($FusionDir.Length))" }
Show-FusionLog

Clear-Registration
$type::RegisterPreviewHandler($Dll, $false)
Run-Host 'A per-user registration, out of process (as Explorer)' 'local'
Run-Host 'B per-user registration, in process' 'inproc'
Run-Host 'E per-user registration, in process, elevated (for comparison)' 'inproc' -AsAdmin
Run-Host 'F per-user registration, out of process, elevated (for comparison)' 'local' -AsAdmin

Set-ItemProperty -Path "HKCU:\Software\Classes\CLSID\$Clsid" -Name AppID -Value $SharedHost
Run-Host 'C per-user registration, shared prevhost AppID (before 1.0.0)' 'local'

Clear-Registration
$type::RegisterPreviewHandler($Dll, $true)
foreach ($k in "HKCU:\Software\Classes\CLSID\$Clsid", 'HKCU:\Software\Classes\AppID\{1C0ADB7B-F017-4384-BF73-95AA5BC5D084}') {
  if (Test-Path $k) { Remove-Item $k -Recurse -Force }
}
Run-Host 'D machine-wide registration only (installers), out of process' 'local'

Say ""
Say "=== Diagnose() ==="
Say ($type::Diagnose())
Say "=== Application log errors since the test started ==="
$events = @(Get-WinEvent -FilterHashtable @{ LogName = 'Application'; StartTime = $Started } -ErrorAction SilentlyContinue |
  Where-Object { $_.Level -le 3 -and $_.Message -match 'prevhost|dllhost|LinkcoPdfPreview|PreviewHostTest|pdfcraft' } | Select-Object -First 6)
if ($events.Count -eq 0) { Say "  (none)" }
foreach ($e in $events) { Say ("  [$($e.ProviderName) $($e.Id)] " + (($e.Message -split "`r?`n" | Select-Object -First 14) -join ' | ')) }

Stop-Hosts
Clear-Registration
if ($IsAdmin) {
  Remove-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Fusion' -Name ForceLog, LogFailures, LogPath -ErrorAction SilentlyContinue
  Remove-Item (Split-Path -Parent $App) -Recurse -Force -ErrorAction SilentlyContinue
}
[IO.File]::WriteAllLines($Report, $lines)
Write-Output "Report: $Report"
