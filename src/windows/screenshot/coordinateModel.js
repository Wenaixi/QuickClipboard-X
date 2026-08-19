function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

// 将光标点格式化为截图坐标指示文案（ShareX 公开行为：拖拽选区时实时显示坐标）。
// 坐标取整到逻辑像素，格式固定为 "X: <x>  Y: <y>"，便于前端直接渲染与测试。
export function formatCursorCoordinate(point) {
  assertFiniteNumber(point?.x, '点 x');
  assertFiniteNumber(point?.y, '点 y');
  return `X: ${Math.round(point.x)}  Y: ${Math.round(point.y)}`;
}

// 计算坐标指示面板的位置：默认放在光标右下方 12px 处，越界时翻转到对侧并夹紧到显示器边距。
export function coordinatePanelPosition(point, bounds, options = {}) {
  assertFiniteNumber(point?.x, '点 x');
  assertFiniteNumber(point?.y, '点 y');
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  const width = options.width ?? 96;
  const height = options.height ?? 26;
  const gap = options.gap ?? 12;
  const margin = options.margin ?? 8;
  assertFiniteNumber(width, '面板宽度');
  assertFiniteNumber(height, '面板高度');
  assertFiniteNumber(gap, '面板间隙');
  assertFiniteNumber(margin, '面板边距');
  if (width <= 0 || height <= 0) {
    throw new RangeError('面板尺寸必须为正数');
  }
  const preferRight = point.x + gap + width <= bounds.width - margin;
  const preferBelow = point.y + gap + height <= bounds.height - margin;
  const left = preferRight
    ? point.x + gap
    : Math.max(margin, Math.min(point.x - gap - width, bounds.width - width - margin));
  const top = preferBelow
    ? point.y + gap
    : Math.max(margin, Math.min(point.y - gap - height, bounds.height - height - margin));
  return { left, top };
}
