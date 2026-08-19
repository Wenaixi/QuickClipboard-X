// 选区位置信息（ShareX 公开行为参考：选区标签显示选区在屏幕上的坐标位置）。
// 物理像素位置 = (显示器偏移 + 逻辑坐标) * DPR，向下取整到像素边界，
// 与 selectionToPhysical 的 start 处理保持一致（选区覆盖 [left, right) 的像素）。

function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

export function selectionPhysicalPosition(selection, options = {}) {
  if (!selection || typeof selection !== 'object') {
    throw new TypeError('选区缺失');
  }
  assertFiniteNumber(selection.left, 'left');
  assertFiniteNumber(selection.top, 'top');
  const dpr = options.dpr ?? 1;
  const monitorLeft = options.monitorLeft ?? 0;
  const monitorTop = options.monitorTop ?? 0;
  if (!Number.isFinite(dpr) || dpr <= 0) {
    throw new RangeError('dpr 必须是正数');
  }
  assertFiniteNumber(monitorLeft, 'monitorLeft');
  assertFiniteNumber(monitorTop, 'monitorTop');
  return {
    x: Math.floor((monitorLeft + selection.left) * dpr),
    y: Math.floor((monitorTop + selection.top) * dpr),
  };
}

export function formatSelectionPosition(selection, options = {}) {
  const { x, y } = selectionPhysicalPosition(selection, options);
  return `X: ${x}  Y: ${y}`;
}
