# Publishes a text report as GitHub Actions annotations (which, unlike logs, the checks API returns):
# chunks First .. First+9 of about 3000 characters each, titled "<file> part N of M".
param(
  [Parameter(Mandatory = $true)] [string] $Path,
  [int] $First = 0,
  [string] $Kind = 'notice'
)
if (-not (Test-Path -LiteralPath $Path)) { Write-Output "::$Kind title=report::$Path was not written"; exit 0 }
$text = (Get-Content -LiteralPath $Path -Raw) -replace "`r", ''
$size = 3000
$chunks = [System.Collections.Generic.List[string]]::new()
$start = 0
while ($start -lt $text.Length) {
  $len = [Math]::Min($size, $text.Length - $start)
  if ($start + $len -lt $text.Length) {
    $cut = $text.LastIndexOf("`n", $start + $len - 1, $len)
    if ($cut -gt $start) { $len = $cut - $start + 1 }
  }
  $chunks.Add($text.Substring($start, $len))
  $start += $len
}
$name = Split-Path -Leaf $Path
for ($i = $First; $i -lt [Math]::Min($First + 10, $chunks.Count); $i++) {
  $msg = $chunks[$i] -replace '%', '%25' -replace "`n", '%0A'
  Write-Output "::$Kind title=$name part $($i + 1) of $($chunks.Count)::$msg"
}
