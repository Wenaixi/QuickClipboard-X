// 选区调整手柄（ShareX 公开行为参考：选区四角与四边中点显示可视手柄，便于发现可调整位置）。
// selectionHandles 计算八个手柄的绝对定位坐标，供前端渲染小圆点。

export function selectionHandles(selection) {
  if (!selection || typeof selection !== 'object') {
    throw new TypeError('选区缺失');
  }
  for (const key of ['left', 'top', 'right', 'bottom']) {
    if (!Number.isFinite(selection[key])) {
      throw new TypeError(`${key} 必须是有限数字`);
    }
  }
  const { left, top, right, bottom } = selection;
  const centerX = (left + right) / 2;
  const centerY = (top + bottom) / 2;
  return [
    { edge: 'nw', left, top },
    { edge: 'n', left: centerX, top },
    { edge: 'ne', left: right, top },
    { edge: 'e', left: right, top: centerY },
    { edge: 'se', left: right, top: bottom },
    { edge: 's', left: centerX, top: bottom },
    { edge: 'sw', left, top: bottom },
    { edge: 'w', left, top: centerY },
  ];
}
