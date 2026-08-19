// 截图交互模式提示（ShareX 公开行为参考：操作时界面给出当前交互模式提示）。
// modeForState 根据三个布尔状态推导当前模式，modeHint 将其翻译为语言包文案。

const MODE_KEYS = {
  select: 'screenshot.mode.select',
  move: 'screenshot.mode.move',
  resize: 'screenshot.mode.resize',
};

export function modeForState(state) {
  if (!state || typeof state !== 'object') {
    throw new TypeError('状态对象缺失');
  }
  if (state.resizing) return 'resize';
  if (state.moving) return 'move';
  if (state.selecting) return 'select';
  return null;
}

export function modeHint(mode, t) {
  if (typeof t !== 'function') {
    throw new TypeError('翻译函数缺失');
  }
  const key = MODE_KEYS[mode];
  return key ? t(key) : null;
}
