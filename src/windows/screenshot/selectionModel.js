function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function normalizeAxis(start, end, limit) {
  const low = Math.min(clamp(start, 0, limit), clamp(end, 0, limit));
  const high = Math.max(clamp(start, 0, limit), clamp(end, 0, limit));

  if (high > low) {
    return [low, high];
  }
  if (low >= limit) {
    return [Math.max(0, limit - 1), limit];
  }
  return [low, Math.min(limit, low + 1)];
}

export function normalizeSelection(start, end, bounds) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  assertFiniteNumber(start?.x, '起点 x');
  assertFiniteNumber(start?.y, '起点 y');
  assertFiniteNumber(end?.x, '终点 x');
  assertFiniteNumber(end?.y, '终点 y');

  const [left, right] = normalizeAxis(start.x, end.x, bounds.width);
  const [top, bottom] = normalizeAxis(start.y, end.y, bounds.height);
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

export function isClickGesture(start, end, threshold = 4) {
  assertFiniteNumber(start?.x, '起点 x');
  assertFiniteNumber(start?.y, '起点 y');
  assertFiniteNumber(end?.x, '终点 x');
  assertFiniteNumber(end?.y, '终点 y');
  assertFiniteNumber(threshold, '单击阈值');
  if (threshold < 0) {
    throw new RangeError('单击阈值不能为负数');
  }
  return Math.hypot(end.x - start.x, end.y - start.y) <= threshold;
}

export function selectionFromPhysical(selection, devicePixelRatio, bounds) {
  if (!bounds || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  assertFiniteNumber(devicePixelRatio, 'devicePixelRatio');
  if (devicePixelRatio <= 0) {
    throw new RangeError('devicePixelRatio 必须为正数');
  }
  for (const key of ['left', 'top', 'right', 'bottom']) {
    assertFiniteNumber(selection?.[key], `selection.${key}`);
  }
  return normalizeSelection(
    { x: selection.left / devicePixelRatio, y: selection.top / devicePixelRatio },
    { x: selection.right / devicePixelRatio, y: selection.bottom / devicePixelRatio },
    bounds
  );
}

export function isCurrentGesture(expectedId, currentId) {
  return expectedId === currentId;
}

export function selectionForPointerGesture(start, end, bounds, windowSelections = []) {
  if (isClickGesture(start, end)) {
    const selectedWindow = windowSelections.find((selection) => (
      selection
      && start.x >= selection.left
      && start.x < selection.right
      && start.y >= selection.top
      && start.y < selection.bottom
    ));
    if (selectedWindow) {
      return normalizeSelection(
        { x: selectedWindow.left, y: selectedWindow.top },
        { x: selectedWindow.right, y: selectedWindow.bottom },
        bounds
      );
    }
  }
  return normalizeSelection(start, end, bounds);
}

export function hitSelectionInterior(point, selection, inset = 4) {
  assertFiniteNumber(point?.x, '点 x');
  assertFiniteNumber(point?.y, '点 y');
  for (const key of ['left', 'top', 'right', 'bottom']) {
    assertFiniteNumber(selection?.[key], `selection.${key}`);
  }
  assertFiniteNumber(inset, '内部边距');
  if (inset < 0) {
    throw new RangeError('内部边距不能为负数');
  }
  const left = selection.left + inset;
  const top = selection.top + inset;
  const right = selection.right - inset;
  const bottom = selection.bottom - inset;
  return point.x >= left && point.x < right && point.y >= top && point.y < bottom;
}

export function hitSelectionEdge(point, selection, tolerance = 4) {
  assertFiniteNumber(point?.x, '点 x');
  assertFiniteNumber(point?.y, '点 y');
  for (const key of ['left', 'top', 'right', 'bottom']) {
    assertFiniteNumber(selection?.[key], `selection.${key}`);
  }
  assertFiniteNumber(tolerance, '边缘容差');
  if (tolerance < 0) {
    throw new RangeError('边缘容差不能为负数');
  }

  if (
    point.x < selection.left - tolerance
    || point.x >= selection.right + tolerance
    || point.y < selection.top - tolerance
    || point.y >= selection.bottom + tolerance
  ) {
    return null;
  }
  // 严格内部点（距各边都超过容差）留给平移，不属边缘。
  if (
    point.x > selection.left + tolerance
    && point.x < selection.right - tolerance
    && point.y > selection.top + tolerance
    && point.y < selection.bottom - tolerance
  ) {
    return null;
  }

  const edges = [];
  if (point.x <= selection.left + tolerance) edges.push('w');
  if (point.x >= selection.right - tolerance) edges.push('e');
  if (point.y <= selection.top + tolerance) edges.push('n');
  if (point.y >= selection.bottom - tolerance) edges.push('s');
  if (edges.length === 0) {
    return null;
  }
  // 角点按 n/s 在前、e/w 在后的约定命名（nw/ne/sw/se）。
  edges.sort((a, b) => 'nsew'.indexOf(a) - 'nsew'.indexOf(b));
  return edges.join('');
}

export function resizeSelection(selection, edge, point, bounds, options = {}) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  if (typeof edge !== 'string' || edge.length === 0 || !/^[nsew]+$/.test(edge)) {
    throw new TypeError('edge 必须是 n/s/e/w 组合');
  }
  assertFiniteNumber(point?.x, '点 x');
  assertFiniteNumber(point?.y, '点 y');

  const current = normalizeSelection(
    { x: selection.left, y: selection.top },
    { x: selection.right, y: selection.bottom },
    bounds
  );

  // 保持比例（ShareX 公开行为：按住 Shift 时调整大小锁定宽高比）。
  // 以拖动边的对边为锚点，主轴取拖动点到锚点的距离，另一维按当前宽高比推导，
  // 角点取长轴驱动；最后整体夹紧到显示器边界，比例不因夹紧而改变。
  if (options.keepAspectRatio === true) {
    const ratio = current.width / current.height;
    // 拖 w 锚定右边、拖 e 锚定左边；拖 n 锚定下边、拖 s 锚定上边。
    const anchorX = edge.includes('w') ? current.right : current.left;
    const anchorY = edge.includes('n') ? current.bottom : current.top;
    const freeWidth = edge.includes('w') || edge.includes('e') ? Math.abs(point.x - anchorX) : 0;
    const freeHeight = edge.includes('n') || edge.includes('s') ? Math.abs(point.y - anchorY) : 0;
    const mainAxisIsWidth = freeWidth >= freeHeight;
    const width = mainAxisIsWidth
      ? Math.max(1, Math.round(freeWidth))
      : Math.max(1, Math.round(freeHeight * ratio));
    const height = mainAxisIsWidth
      ? Math.max(1, Math.round(width / ratio))
      : Math.max(1, Math.round(freeHeight));

    const maxLeft = Math.max(0, bounds.width - width);
    const maxTop = Math.max(0, bounds.height - height);
    const left = clamp(edge.includes('w') ? current.right - width : current.left, 0, maxLeft);
    const top = clamp(edge.includes('n') ? current.bottom - height : current.top, 0, maxTop);
    return { left, top, right: left + width, bottom: top + height, width, height };
  }

  let left = current.left;
  let right = current.right;
  let top = current.top;
  let bottom = current.bottom;

  if (edge.includes('w')) left = clamp(point.x, 0, bounds.width);
  if (edge.includes('e')) right = clamp(point.x, 0, bounds.width);
  if (edge.includes('n')) top = clamp(point.y, 0, bounds.height);
  if (edge.includes('s')) bottom = clamp(point.y, 0, bounds.height);

  // 仅被拖动的边按对边夹紧到最小 1px，不翻转选区；未被拖动的边保持原值。
  if (edge.includes('w')) left = clamp(left, 0, Math.max(0, right - 1));
  if (edge.includes('e')) right = clamp(right, left + 1, bounds.width);
  if (edge.includes('n')) top = clamp(top, 0, Math.max(0, bottom - 1));
  if (edge.includes('s')) bottom = clamp(bottom, top + 1, bounds.height);

  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

export function nudgeSelection(selection, dx, dy, bounds) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  assertFiniteNumber(dx, '横向位移');
  assertFiniteNumber(dy, '纵向位移');

  const current = normalizeSelection(
    { x: selection.left, y: selection.top },
    { x: selection.right, y: selection.bottom },
    bounds
  );
  const maxLeft = Math.max(0, bounds.width - current.width);
  const maxTop = Math.max(0, bounds.height - current.height);
  const left = clamp(current.left + dx, 0, maxLeft);
  const top = clamp(current.top + dy, 0, maxTop);
  return {
    left,
    top,
    right: left + current.width,
    bottom: top + current.height,
    width: current.width,
    height: current.height,
  };
}

export function selectionToPhysical(selection, devicePixelRatio, physicalBounds) {
  if (!physicalBounds || physicalBounds.width <= 0 || physicalBounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  assertFiniteNumber(devicePixelRatio, 'devicePixelRatio');
  if (devicePixelRatio <= 0) {
    throw new RangeError('devicePixelRatio 必须为正数');
  }
  for (const key of ['left', 'top', 'right', 'bottom']) {
    assertFiniteNumber(selection?.[key], `selection.${key}`);
  }

  const rawLeft = Math.floor(selection.left * devicePixelRatio);
  const rawTop = Math.floor(selection.top * devicePixelRatio);
  const rawRight = Math.ceil(selection.right * devicePixelRatio);
  const rawBottom = Math.ceil(selection.bottom * devicePixelRatio);
  const physicalSelection = (start, end, limit) => {
    const requestedSize = Math.max(1, end - start);
    if (requestedSize >= limit) {
      return [0, limit];
    }

    let safeStart = start;
    let safeEnd = end;
    if (safeStart < 0) {
      safeEnd += -safeStart;
      safeStart = 0;
    }
    if (safeEnd > limit) {
      safeStart -= safeEnd - limit;
      safeEnd = limit;
    }
    safeStart = clamp(safeStart, 0, limit - requestedSize);
    return [safeStart, safeStart + requestedSize];
  };

  const [left, right] = physicalSelection(rawLeft, rawRight, physicalBounds.width);
  const [top, bottom] = physicalSelection(rawTop, rawBottom, physicalBounds.height);
  return { left, top, right, bottom, width: right - left, height: bottom - top };
}

export function createRafWriter(write, requestFrame = globalThis.requestAnimationFrame) {
  if (typeof write !== 'function') {
    throw new TypeError('write 必须是函数');
  }
  if (typeof requestFrame !== 'function') {
    throw new TypeError('requestFrame 必须是函数');
  }

  let pendingValue;
  let scheduled = false;
  let generation = 0;
  const schedule = (value) => {
    pendingValue = value;
    if (scheduled) return;
    scheduled = true;
    const scheduledGeneration = generation;
    requestFrame(() => {
      scheduled = false;
      if (scheduledGeneration !== generation) {
        pendingValue = undefined;
        return;
      }
      const nextValue = pendingValue;
      pendingValue = undefined;
      if (nextValue !== undefined) write(nextValue);
    });
  };

  return {
    schedule,
    cancel() {
      generation += 1;
      pendingValue = undefined;
    },
  };
}
