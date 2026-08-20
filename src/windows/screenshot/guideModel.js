function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

// 计算拖拽时光标处的十字参考线（ShareX 公开行为：拖动选区时显示贯穿屏幕的水平与垂直辅助线）。
// 垂直线沿光标 x 贯穿整个画布高度，水平线沿光标 y 贯穿整个画布宽度；
// 坐标夹紧到边界内，保证线始终落在可绘制区域。
export function guideLines(point, bounds) {
  assertFiniteNumber(point?.x, '点 x');
  assertFiniteNumber(point?.y, '点 y');
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  // 引导线是 1px 宽绘制元素：x 夹紧到 [0, width-1]、y 夹紧到 [0, height-1]，
  // 否则光标恰在右/下边缘（x=width）时整条线画到画布外不可见。
  const x = clamp(point.x, 0, bounds.width - 1);
  const y = clamp(point.y, 0, bounds.height - 1);
  return {
    vertical: { left: x, top: 0, width: 1, height: bounds.height },
    horizontal: { left: 0, top: y, width: bounds.width, height: 1 },
  };
}
