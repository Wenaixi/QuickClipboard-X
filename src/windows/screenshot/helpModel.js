// 快捷键帮助面板（ShareX 公开行为：截图界面可按 F1 查看全部快捷键说明）。
// helpEntries 返回按分组排序的快捷键说明数组，每项 { id, keys, label }；
// isHelpShortcut 识别 F1 与 ?（无修饰键）作为帮助开关。

const HELP_ITEMS = [
  { id: 'complete', keys: ['Enter', 'Ctrl+C'], labelKey: 'screenshot.help.complete' },
  { id: 'save', keys: ['Ctrl+S'], labelKey: 'screenshot.help.save' },
  { id: 'pin', keys: ['Ctrl+P'], labelKey: 'screenshot.help.pin' },
  { id: 'fullscreen', keys: ['Ctrl+A'], labelKey: 'screenshot.help.fullscreen' },
  { id: 'cancel', keys: ['Esc'], labelKey: 'screenshot.help.cancel' },
  { id: 'nudge', keys: ['方向键'], labelKey: 'screenshot.help.nudge' },
  { id: 'square', keys: ['Shift'], labelKey: 'screenshot.help.square' },
  { id: 'center', keys: ['Ctrl'], labelKey: 'screenshot.help.center' },
  { id: 'undo', keys: ['Ctrl+Z'], labelKey: 'screenshot.help.undo' },
  { id: 'quickAction', keys: ['1-9'], labelKey: 'screenshot.help.quickAction' },
];

export function helpEntries(t) {
  if (typeof t !== 'function') {
    throw new TypeError('翻译函数缺失');
  }
  return HELP_ITEMS.map((item) => ({
    id: item.id,
    keys: [...item.keys],
    label: t(item.labelKey),
  }));
}

export function isHelpShortcut(event) {
  if (!event || typeof event !== 'object') return false;
  if (event.ctrlKey || event.metaKey || event.altKey) return false;
  return event.key === 'F1' || event.key === '?';
}
