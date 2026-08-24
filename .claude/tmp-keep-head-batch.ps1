$files = @(
  'src-tauri/src/commands/mod.rs',
  'src-tauri/src/commands/settings.rs',
  'src-tauri/src/lib.rs',
  'src-tauri/src/services/clipboard/monitor.rs',
  'src-tauri/src/services/database/clipboard.rs',
  'src-tauri/src/services/database/favorites.rs',
  'src-tauri/src/services/low_memory/manager.rs',
  'src-tauri/src/services/settings/model.rs',
  'src-tauri/src/services/system/app_filter.rs',
  'src-tauri/src/services/system/hotkey.rs',
  'src-tauri/src/services/system/hotkey/global.rs',
  'src-tauri/src/services/system/hotkey/navigation.rs',
  'src-tauri/src/services/system/raw_input.rs',
  'src-tauri/src/windows/main_window/edge_monitor.rs',
  'src-tauri/src/windows/main_window/visibility.rs',
  'src-tauri/src/windows/pin_image_window/pin_image_window.rs',
  'src-tauri/src/windows/tray/menu.rs',
  'src/shared/api/clipboard.js',
  'src/shared/locales/en-US.json',
  'src/shared/locales/zh-CN.json',
  'src/windows/main/App.jsx',
  'src/windows/main/components/TabNavigation.jsx'
)
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
