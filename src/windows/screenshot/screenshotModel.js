function positiveNumber(value, fallback) {
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

export function normalizeBootstrap(payload = {}, viewport = {}) {
  const monitor = payload.monitor || {};
  const dpr = positiveNumber(payload.devicePixelRatio, positiveNumber(viewport.devicePixelRatio, 1));
  const logicalWidth = positiveNumber(monitor.logicalWidth, positiveNumber(payload.viewport?.width, positiveNumber(viewport.width, 1)));
  const logicalHeight = positiveNumber(monitor.logicalHeight, positiveNumber(payload.viewport?.height, positiveNumber(viewport.height, 1)));
  const physicalWidth = positiveNumber(monitor.physicalWidth, positiveNumber(monitor.width, Math.round(logicalWidth * dpr)));
  const physicalHeight = positiveNumber(monitor.physicalHeight, positiveNumber(monitor.height, Math.round(logicalHeight * dpr)));

  return {
    sessionId: typeof payload.sessionId === 'string' ? payload.sessionId : '',
    bounds: { width: logicalWidth, height: logicalHeight },
    physicalBounds: { width: physicalWidth, height: physicalHeight },
    monitorLeft: Number.isFinite(monitor.left) ? monitor.left / dpr : 0,
    monitorTop: Number.isFinite(monitor.top) ? monitor.top / dpr : 0,
    dpr,
    initialAction: typeof payload.initialAction === 'string' ? payload.initialAction : '',
    screenshotAiEnabled: payload.screenshotAiEnabled !== false,
    screenshotAiConfigured: payload.screenshotAiConfigured === true,
    screenshotMagnifierEnabled: payload.screenshotMagnifierEnabled !== false,
    magnifierBackground: typeof payload.magnifierBackground === 'string' ? payload.magnifierBackground : null,
    // 截图提示开关：设置项随 bootstrap 下发，false 时隐藏空闲引导与模式提示。
    screenshotHintsEnabled: payload.screenshotHintsEnabled !== false,
    // 截图窗口生命周期模式：quick 隐藏复用 / dispose 销毁 / auto 超时释放。
    // 默认 quick 与后端 AppSettings 默认一致，旧会话（无此字段）按 quick 处理。
    lifecycleMode: payload.lifecycleMode === 'dispose' || payload.lifecycleMode === 'auto' ? payload.lifecycleMode : 'quick',
  };
}
