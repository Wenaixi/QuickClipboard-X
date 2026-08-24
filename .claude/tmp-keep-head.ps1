param([string[]]$files)
foreach ($f in $files) {
  $lines = [System.IO.File]::ReadAllLines((Resolve-Path $f).Path)
  $selections = @()
  $i = 0
  while ($i -lt $lines.Count) {
    $line = $lines[$i]
    if ($line -match '^<<<<<<< HEAD$') {
      $j = $i + 1
      while ($j -lt $lines.Count -and $lines[$j] -notmatch '^=======$') { $j = $j + 1 }
      $k = $j + 1
      while ($k -lt $lines.Count -and $lines[$k] -notmatch '^>>>>>>> ') { $k = $k + 1 }
      $i = $k + 1
      continue
    }
    $selections += $line
    $i = $i + 1
  }
  [System.IO.File]::WriteAllText((Resolve-Path $f).Path, ($selections -join "`r`n") + "`r`n")
  Write-Host ("$f ok: " + $selections.Count + " lines")
}
