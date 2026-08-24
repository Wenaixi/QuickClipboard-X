$files = @('i18n/README.en.md','i18n/README.ja.md','i18n/README.ko.md','i18n/README.zh-TW.md')
foreach ($f in $files) {
  $lines = Get-Content $f
  $startIdx = -1; $midIdx = -1; $endIdx = -1
  for ($i = 0; $i -lt $lines.Count; $i++) {
    if ($startIdx -lt 0 -and $lines[$i] -match '^<<<<<<< HEAD$') { $startIdx = $i }
    elseif ($midIdx -lt 0 -and $lines[$i] -match '^=======$') { $midIdx = $i }
    elseif ($endIdx -lt 0 -and $lines[$i] -match '^>>>>>>> ') { $endIdx = $i; break }
  }
  Write-Host ("$f start=$startIdx mid=$midIdx end=$endIdx")
  if ($startIdx -lt 0 -or $midIdx -lt 0 -or $endIdx -lt 0) { Write-Host '  SKIP'; continue }
  $kept = @()
  $kept += $lines[0..($startIdx - 1)]
  $kept += $lines[($midIdx + 1)..($endIdx - 1)]
  if ($endIdx + 1 -le $lines.Count - 1) { $kept += $lines[($endIdx + 1)..($lines.Count - 1)] }
  [System.IO.File]::WriteAllText((Resolve-Path $f).Path, ($kept -join "`r`n") + "`r`n")
  Write-Host ("  ok: " + $kept.Count + " lines")
}
