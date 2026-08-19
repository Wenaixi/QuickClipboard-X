// 完成快捷键统一判定（ShareX 公开契约：截图界面 Enter 完成并复制，Ctrl+C 同样完成并复制，
// Ctrl+S 保存、Ctrl+P 贴图）。单一来源避免 keydown 里散落的 if/else 分支漂移。

export function completeShortcutForEvent(event) {
  if (!event || typeof event !== 'object') {
    throw new TypeError('事件对象缺失');
  }
  const key = typeof event.key === 'string' ? event.key.toLowerCase() : '';
  if (event.ctrlKey) {
    if (key === 'c') return 'copy';
    if (key === 's') return 'save';
    if (key === 'p') return 'pin';
    return null;
  }
  if (event.metaKey || event.altKey) return null;
  return key === 'enter' ? 'copy' : null;
}
