function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

// 计算放大镜面板位置与背景快照采样区域（均为逻辑像素）。
// 面板默认放在光标右下方，越界时翻转到对侧，最后夹在显示器边距内；
// 采样源以光标为圆心覆盖面板格数对应的源像素矩形，并夹紧到显示器边界。
export function magnifierGeometry(point, bounds, options = {}) {
  assertFiniteNumber(point?.x, '点 x');
  assertFiniteNumber(point?.y, '点 y');
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }

  const scale = options.scale ?? 6;
  const panelWidth = options.panelWidth ?? 168;
  const panelHeight = options.panelHeight ?? 168;
  const gap = options.gap ?? 16;
  const margin = options.margin ?? 8;
  assertFiniteNumber(scale, '缩放倍率');
  assertFiniteNumber(panelWidth, '面板宽度');
  assertFiniteNumber(panelHeight, '面板高度');
  assertFiniteNumber(gap, '面板间隙');
  assertFiniteNumber(margin, '面板边距');
  if (scale <= 0 || panelWidth <= 0 || panelHeight <= 0) {
    throw new RangeError('面板尺寸与缩放倍率必须为正数');
  }

  const cols = Math.max(1, Math.floor(panelWidth / scale));
  const rows = Math.max(1, Math.floor(panelHeight / scale));
  const sourceLeft = clamp(point.x - cols / 2, 0, Math.max(0, bounds.width - cols));
  const sourceTop = clamp(point.y - rows / 2, 0, Math.max(0, bounds.height - rows));

  const preferRight = point.x + gap + panelWidth <= bounds.width - margin;
  const preferBelow = point.y + gap + panelHeight <= bounds.height - margin;
  const panelLeft = preferRight
    ? point.x + gap
    : clamp(point.x - gap - panelWidth, margin, Math.max(margin, bounds.width - panelWidth - margin));
  const panelTop = preferBelow
    ? point.y + gap
    : clamp(point.y - gap - panelHeight, margin, Math.max(margin, bounds.height - panelHeight - margin));

  return {
    panel: { left: panelLeft, top: panelTop, width: panelWidth, height: panelHeight },
    source: { left: sourceLeft, top: sourceTop, cols, rows },
    scale,
  };
}

// 从背景快照 RGBA 字节中采样放大镜网格；快照宽度用于行步进。
// 返回二维数组，每个格子为 [r, g, b, a]。
export function sampleMagnifierGrid(source, geometry, snapshotWidth, snapshotHeight) {
  if (!source || source.length === 0 || !geometry) {
    return [];
  }
  const { source: area } = geometry;
  const expected = snapshotWidth * snapshotHeight * 4;
  if (source.length < expected) {
    throw new RangeError('背景快照数据长度不足');
  }
  const grid = [];
  for (let row = 0; row < area.rows; row += 1) {
    const line = [];
    for (let col = 0; col < area.cols; col += 1) {
      const x = Math.min(snapshotWidth - 1, Math.floor(area.left) + col);
      const y = Math.min(snapshotHeight - 1, Math.floor(area.top) + row);
      const index = (y * snapshotWidth + x) * 4;
      line.push([source[index], source[index + 1], source[index + 2], source[index + 3]]);
    }
    grid.push(line);
  }
  return grid;
}
