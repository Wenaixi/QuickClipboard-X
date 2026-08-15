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
    dpr,
    initialAction: typeof payload.initialAction === 'string' ? payload.initialAction : '',
    screenshotAiEnabled: payload.screenshotAiEnabled !== false,
    screenshotAiConfigured: payload.screenshotAiConfigured === true,
  };
}
