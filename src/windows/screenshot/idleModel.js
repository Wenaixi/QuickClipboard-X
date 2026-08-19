// 选区建立前初始引导（ShareX 公开行为：截图界面未选择时显示操作提示）。
// idleHint 返回翻译后的引导文案，拒绝非法翻译函数。

export function idleHint(t) {
  if (typeof t !== 'function') {
    throw new TypeError('翻译函数缺失');
  }
  return t('screenshot.idleHint');
}
