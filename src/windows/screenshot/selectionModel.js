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
