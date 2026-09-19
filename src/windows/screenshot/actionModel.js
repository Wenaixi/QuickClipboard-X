// ShareX 公开契约：选区建立后按数字键 1/2/3/4 快速执行对应动作。
// 数字键与动作一一对应，映射保持单一来源，便于测试与扩展。
const HOTKEY_ACTIONS = {
  '1': 'copy',
  '2': 'save',
  '3': 'pin',
  '4': 'edit',
  '5': 'upload',
  '6': 'copy+pin',
  '7': 'ai',
};

export function actionForHotkey(key) {
  // 防御式容错：真实事件对象 key 恒为字符串，但非字符串入参（数字等）
  // 若直接查表会被 JS 强制转字符串误命中，必须显式守卫。
  if (typeof key !== 'string') {
    return null;
  }
  return HOTKEY_ACTIONS[key] || null;
}

export function hotkeyForAction(action) {
  return Object.keys(HOTKEY_ACTIONS).find((key) => HOTKEY_ACTIONS[key] === action) || '';
}
