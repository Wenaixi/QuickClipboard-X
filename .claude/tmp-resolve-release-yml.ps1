$f = '.github/workflows/release.yml'
$lines = Get-Content $f
Write-Host ('total lines: ' + $lines.Count)
# 找到 3 个冲突段并按保留 HEAD + 接 upstream cache 拼接
# 段1: line 25 '<<<<<<<' 到 line 35 '>>>>>>>'  → 替换为保留本地 'Checkout' 块
# 段2: line 40 '<<<<<<<' 到 line 49 '>>>>>>>'  → 替换为保留本地空（什么都不保留）
# 段3: line 54 '<<<<<<<' 到 line 60 '>>>>>>>'  → 替换为 upstream cache 'npm' + cache-dependency-path
$kept = @()
$i = 0
while ($i -lt $lines.Count) {
  $line = $lines[$i]
  if ($line -match '^<<<<<<< HEAD$') {
    # 找 mid + end
    $j = $i + 1; while ($j -lt $lines.Count -and $lines[$j] -notmatch '^=======$') { $j++ }
    $k = $j + 1; while ($k -lt $lines.Count -and $lines[$k] -notmatch '^>>>>>>> ') { $k++ }
    # 段3特殊处理：取 upstream 部分
    if ($i -eq 53) {
      $kept += "          cache: 'npm'"
      $kept += '          cache-dependency-path: |'
      $kept += '            package-lock.json'
    }
    $i = $k + 1
    continue
  }
  $kept += $line
  $i++
}
[System.IO.File]::WriteAllText((Resolve-Path $f).Path, ($kept -join "`r`n") + "`r`n")
Write-Host ('ok: ' + $kept.Count + ' lines')
