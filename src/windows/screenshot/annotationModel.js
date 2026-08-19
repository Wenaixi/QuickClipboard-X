function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

// 把带宽度/颜色/透明度的线段参数归一化为 CSS 描边样式（ShareX 公开行为：选区边框可调粗细与颜色）。
// 宽度与透明度夹紧到合理范围，颜色保持字符串原样由调用方保证合法 CSS 颜色。
export function lineStyle(width, color, opacity) {
  assertFiniteNumber(width, '线宽');
  if (typeof color !== 'string' || color.length === 0) {
    throw new TypeError('颜色必须是合法 CSS 颜色字符串');
  }
  assertFiniteNumber(opacity, '透明度');
  if (width < 0) {
    throw new RangeError('线宽不能为负数');
  }
  if (opacity < 0 || opacity > 1) {
    throw new RangeError('透明度必须在 0 到 1 之间');
  }
  const safeWidth = Math.min(width, 64);
  return {
    borderWidth: `${safeWidth}px`,
    borderColor: color,
    opacity,
  };
}
