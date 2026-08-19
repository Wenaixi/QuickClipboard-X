function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function bestSnapDelta(candidates, tolerance) {
  let bestDelta = 0;
  let bestDistance = tolerance + 1;
  for (const candidate of candidates) {
    const distance = Math.abs(candidate.delta);
    if (distance <= tolerance && distance < bestDistance) {
      bestDistance = distance;
      bestDelta = candidate.delta;
    }
  }
  return bestDelta;
}

// 选区磁吸到屏幕引导线（ShareX 公开行为：移动/调整大小靠近屏幕边缘与中心线时自动吸附）。
// 默认容差 6px；edge 为空串表示整体平移（左缘/右缘/垂直中心线取最小位移者，保持尺寸），
// edge 为 n/s/e/w 组合表示仅被拖动的边吸附（拖 e 吸附右缘或垂直中心线，其余同理）。
export function magnetSelection(selection, bounds, options = {}) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  const tolerance = options.tolerance ?? 6;
  assertFiniteNumber(tolerance, '吸附容差');
  if (tolerance < 0) {
    throw new RangeError('吸附容差不能为负数');
  }
  const edge = options.edge ?? '';
  if (typeof edge !== 'string' || (edge !== '' && !/^[nsew]+$/.test(edge))) {
    throw new TypeError('edge 必须是空串或 n/s/e/w 组合');
  }
  for (const key of ['left', 'top', 'right', 'bottom']) {
    assertFiniteNumber(selection?.[key], `${key}`);
  }

  let left = selection.left;
  let top = selection.top;
  let right = selection.right;
  let bottom = selection.bottom;

  if (edge === '') {
    const dx = bestSnapDelta([
      { delta: 0 - left },
      { delta: bounds.width - right },
      { delta: bounds.width / 2 - (left + right) / 2 },
    ], tolerance);
    const dy = bestSnapDelta([
      { delta: 0 - top },
      { delta: bounds.height - bottom },
      { delta: bounds.height / 2 - (top + bottom) / 2 },
    ], tolerance);
    left += dx;
    right += dx;
    top += dy;
    bottom += dy;
    const snapWidth = right - left;
    const snapHeight = bottom - top;
    left = clamp(left, 0, Math.max(0, bounds.width - snapWidth));
    top = clamp(top, 0, Math.max(0, bounds.height - snapHeight));
    right = left + snapWidth;
    bottom = top + snapHeight;
  } else {
    if (edge.includes('e')) {
      right += bestSnapDelta([
        { delta: bounds.width - right },
        { delta: bounds.width / 2 - right },
      ], tolerance);
      right = clamp(right, left + 1, bounds.width);
    }
    if (edge.includes('w')) {
      left += bestSnapDelta([
        { delta: 0 - left },
        { delta: bounds.width / 2 - left },
      ], tolerance);
      left = clamp(left, 0, Math.max(0, right - 1));
    }
    if (edge.includes('s')) {
      bottom += bestSnapDelta([
        { delta: bounds.height - bottom },
        { delta: bounds.height / 2 - bottom },
      ], tolerance);
      bottom = clamp(bottom, top + 1, bounds.height);
    }
    if (edge.includes('n')) {
      top += bestSnapDelta([
        { delta: 0 - top },
        { delta: bounds.height / 2 - top },
      ], tolerance);
      top = clamp(top, 0, Math.max(0, bottom - 1));
    }
  }

  return { left, top, right, bottom, width: right - left, height: bottom - top };
}
