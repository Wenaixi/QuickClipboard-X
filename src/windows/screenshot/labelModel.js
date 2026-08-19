function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

// 计算选区尺寸标签的放置方向（ShareX 公开行为：尺寸标签随选区贴近屏幕边缘时翻转防溢出）。
// 默认放在选区上缘外侧、右对齐到选区右缘；顶部空间不足时翻到下方，
// 右对齐会越出左边界时改为左对齐，保证标签始终完整可见。
export function selectionLabelPlacement(selection, bounds, options = {}) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  for (const key of ['left', 'top', 'right', 'bottom']) {
    assertFiniteNumber(selection?.[key], `selection.${key}`);
  }
  const labelWidth = options.width ?? 90;
  const labelHeight = options.height ?? 22;
  const gap = options.gap ?? 6;
  assertFiniteNumber(labelWidth, '标签宽度');
  assertFiniteNumber(labelHeight, '标签高度');
  assertFiniteNumber(gap, '标签间隙');
  if (labelWidth <= 0 || labelHeight <= 0) {
    throw new RangeError('标签宽度与高度必须为正数');
  }
  if (gap < 0) {
    throw new RangeError('标签间隙不能为负数');
  }

  const above = selection.top - labelHeight - gap >= 0;
  const alignLeft = selection.right - labelWidth < 0;
  return { above, alignLeft };
}
