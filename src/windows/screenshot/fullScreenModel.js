// 全屏选区（ShareX 公开行为参考：可按快捷键或按钮一键选中整个屏幕）。
// fullScreenSelection 生成覆盖整个边界的选区：右/下边界用 ceil/floor 向上取整，
// 保证小数边界时选区完全覆盖屏幕而不留空白边。

export function fullScreenSelection(bounds) {
  if (!bounds || typeof bounds !== 'object') {
    throw new TypeError('边界缺失');
  }
  if (!Number.isFinite(bounds.width) || !Number.isFinite(bounds.height)) {
    throw new TypeError('边界宽度与高度必须是有限数字');
  }
  if (bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  const right = Math.ceil(bounds.width);
  const bottom = Math.ceil(bounds.height);
  return { left: 0, top: 0, right, bottom, width: right, height: bottom };
}
