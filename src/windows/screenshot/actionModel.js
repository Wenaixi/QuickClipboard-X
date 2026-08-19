// ShareX 公开契约：选区建立后按数字键 1/2/3/4 快速执行对应动作。
// 数字键与动作一一对应，映射保持单一来源，便于测试与扩展。
const HOTKEY_ACTIONS = {
  '1': 'copy',
  '2': 'save',
  '3': 'pin',
  '4': 'ai',
};

export function actionForHotkey(key) {
  return HOTKEY_ACTIONS[key] || null;
}

export function hotkeyForAction(action) {
  return Object.keys(HOTKEY_ACTIONS).find((key) => HOTKEY_ACTIONS[key] === action) || '';
}
