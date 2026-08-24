$f = '.gitignore'
$lines = Get-Content $f
$startIdx = -1; $midIdx = -1; $endIdx = -1
for ($i = 0; $i -lt $lines.Count; $i++) {
  if ($startIdx -lt 0 -and $lines[$i] -match '^<<<<<<< HEAD$') { $startIdx = $i }
  elseif ($midIdx -lt 0 -and $lines[$i] -match '^=======$') { $midIdx = $i }
  elseif ($endIdx -lt 0 -and $lines[$i] -match '^>>>>>>> ') { $endIdx = $i; break }
}
$kept = @()
$kept += $lines[0..($startIdx - 1)]
# 接 upstream 的 docs/ 规则（本地 0..startIdx-1 没忽略 docs/，加一行防御）
$kept += 'docs'
# HEAD 段
$kept += $lines[($midIdx + 1)..($endIdx - 1)]
if ($endIdx + 1 -le $lines.Count - 1) { $kept += $lines[($endIdx + 1)..($lines.Count - 1)] }
[System.IO.File]::WriteAllText((Resolve-Path $f).Path, ($kept -join "`r`n") + "`r`n")
Write-Host ("ok: " + $kept.Count + " lines")
