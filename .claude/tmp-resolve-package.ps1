$f = 'package.json'
$lines = Get-Content $f
$kept = @()
$i = 0
while ($i -lt $lines.Count) {
  $line = $lines[$i]
  if ($line -match '"version": "0\.5\.0"') {
    $kept += '  "version": "0.4.2",'
    $i = $i + 1
    continue
  }
  if ($line -match '^<<<<<<< HEAD$') {
    $j = $i + 1; while ($j -lt $lines.Count -and $lines[$j] -notmatch '^=======$') { $j = $j + 1 }
    $k = $j + 1; while ($k -lt $lines.Count -and $lines[$k] -notmatch '^>>>>>>> ') { $k = $k + 1 }
    $i = $k + 1
    continue
  }
  $kept += $line
  $i = $i + 1
}
[System.IO.File]::WriteAllText((Resolve-Path $f).Path, ($kept -join "`r`n") + "`r`n")
Write-Host ('ok: ' + $kept.Count + ' lines')
