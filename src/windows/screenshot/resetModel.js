// 选区重置可用性（ShareX 公开行为参考：太小或误触的选区可用 Esc/右键取消重来）。
// canResetSelection 判断当前选区是否小到需要重置——小于最小尺寸时允许重置，
// 达到最小尺寸后保留选区，避免用户不小心按取消丢工作。

export function canResetSelection(selection, options = {}) {
  if (!selection || typeof selection !== 'object') {
    throw new TypeError('选区缺失');
  }
  const width = selection.width;
  const height = selection.height;
  if (!Number.isFinite(width) || !Number.isFinite(height)) {
    throw new TypeError('宽度与高度必须是有限数字');
  }
  const minSize = options.minSize ?? 5;
  if (!Number.isFinite(minSize) || minSize <= 0) {
    throw new RangeError('最小尺寸必须为正数');
  }
  return width < minSize || height < minSize;
}
