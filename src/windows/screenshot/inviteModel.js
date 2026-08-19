// 完成动作邀请提示（ShareX 公开行为：截图界面选区建立后提示可完成动作的快捷键）。
// includeInvite 返回翻译后的邀请文案，拒绝非法翻译函数。

export function includeInvite(t) {
  if (typeof t !== 'function') {
    throw new TypeError('翻译函数缺失');
  }
  return t('screenshot.invite');
}
