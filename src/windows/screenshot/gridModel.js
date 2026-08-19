function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

// 计算三分法构图辅助网格（ShareX 公开行为：截图区域显示三分法网格辅助构图对齐）。
// 返回两条垂直线与两条水平线，各自贯穿整个画布，位置按宽度/高度的 1/3 与 2/3 四舍五入取整。
export function thirdsGrid(bounds) {
  if (!bounds || typeof bounds !== 'object') {
    throw new TypeError('边界尺寸缺失');
  }
  assertFiniteNumber(bounds.width, '宽度');
  assertFiniteNumber(bounds.height, '高度');
  if (bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  const x1 = Math.round(bounds.width / 3);
  const x2 = Math.round((bounds.width * 2) / 3);
  const y1 = Math.round(bounds.height / 3);
  const y2 = Math.round((bounds.height * 2) / 3);
  return {
    vertical: [
      { left: x1, top: 0, width: 1, height: bounds.height },
      { left: x2, top: 0, width: 1, height: bounds.height },
    ],
    horizontal: [
      { left: 0, top: y1, width: bounds.width, height: 1 },
      { left: 0, top: y2, width: bounds.width, height: 1 },
    ],
  };
}
