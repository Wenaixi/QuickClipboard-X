function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

// 工具栏上下边距（像素），上方放置时再上移自身高度预留空间。
const TOOLBAR_GAP = 8;

// 决策工具栏放在选区下方还是上方（ShareX 公开行为：工具栏贴近选区且随选区自适应翻转）。
// 选区贴近屏幕上缘时放下方；否则默认放下方，仅当下方空间不足且上方空间更大时翻转。
export function toolbarPlacement(selection, bounds) {
  if (!selection || typeof selection !== 'object') {
    throw new TypeError('选区缺失');
  }
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  for (const key of ['top', 'bottom', 'height']) {
    assertFiniteNumber(selection[key], `选区${key}`);
  }
  const spaceBelow = bounds.height - selection.bottom;
  const spaceAbove = selection.top;
  if (spaceAbove <= TOOLBAR_GAP || spaceBelow >= spaceAbove) {
    return 'below';
  }
  return 'above';
}

// 计算工具栏的绝对定位样式：下方时贴选区下缘，上方时贴选区上缘并向上偏移，
// 左右与选区左缘对齐并夹紧到显示器内（避免工具栏溢出屏幕右侧）。
export function toolbarStyle(selection, bounds, placement, toolbarWidth = 300) {
  if (placement !== 'above' && placement !== 'below') {
    throw new TypeError('placement 必须是 above 或 below');
  }
  if (!bounds || !Number.isFinite(bounds.width) || bounds.width <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  assertFiniteNumber(toolbarWidth, '工具栏宽度');
  if (toolbarWidth <= 0) {
    throw new RangeError('工具栏宽度必须为正数');
  }
  for (const key of ['left', 'top', 'bottom']) {
    assertFiniteNumber(selection?.[key], `选区${key}`);
  }
  const maxLeft = Math.max(TOOLBAR_GAP, bounds.width - toolbarWidth - TOOLBAR_GAP);
  const left = clamp(selection.left + TOOLBAR_GAP, TOOLBAR_GAP, maxLeft);
  const top = placement === 'below' ? selection.bottom + TOOLBAR_GAP : selection.top - TOOLBAR_GAP;
  return { left: `${left}px`, top: `${top}px` };
}
