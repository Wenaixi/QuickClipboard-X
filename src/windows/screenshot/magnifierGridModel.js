// 放大镜面板网格线与中心十字（ShareX 公开行为参考：放大镜面板内绘制网格线，便于精确对齐像素）。
// magnifierGridLines 按缩放倍率间隔输出面板内网格线位置（不包含边界），
// magnifierCrosshair 返回面板几何中心供绘制十字参考线。

function assertGeometry(geometry) {
  if (!geometry || typeof geometry !== 'object') {
    throw new TypeError('几何缺失');
  }
  const { panel, scale } = geometry;
  if (!panel || !Number.isFinite(panel.width) || !Number.isFinite(panel.height) || panel.width <= 0 || panel.height <= 0) {
    throw new RangeError('面板尺寸必须为正数');
  }
  if (!Number.isFinite(scale) || scale <= 0) {
    throw new RangeError('缩放倍率必须为正数');
  }
}

export function magnifierGridLines(geometry) {
  assertGeometry(geometry);
  const { panel, scale } = geometry;
  const vertical = [];
  for (let x = scale; x < panel.width; x += scale) {
    vertical.push(x);
  }
  const horizontal = [];
  for (let y = scale; y < panel.height; y += scale) {
    horizontal.push(y);
  }
  return { vertical, horizontal };
}

export function magnifierCrosshair(geometry) {
  assertGeometry(geometry);
  return {
    x: geometry.panel.width / 2,
    y: geometry.panel.height / 2,
  };
}
