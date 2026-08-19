function assertLength(length) {
  if (!Number.isFinite(length)) {
    throw new TypeError('标尺长度 必须是有限数字');
  }
  if (length <= 0) {
    throw new RangeError('标尺长度必须为正数');
  }
}

// 按屏幕尺寸自适应主刻度间隔（ShareX 公开行为：标尺主刻度随显示尺寸调整）。
// 800 及以下取 50，1600 及以下取 100，以上取 200。
export function rulerMajorStep(length) {
  assertLength(length);
  if (length <= 800) return 50;
  if (length <= 1600) return 100;
  return 200;
}

// 生成从 0 到长度（含端点）的标尺刻度：每主刻度间隔的 1/5 放一个次刻度，
// 主刻度位置带像素文本标签，次刻度 label 为 null。
// 用整数计数循环替代浮点累加：浮点累加在长度含小数时会产生漂移（如 1365.33），
// 且 position % majorStep === 0 对漂移值不可靠；计数循环让位置始终为 minorStep 的整数倍。
export function rulerTicks(length) {
  assertLength(length);
  const majorStep = rulerMajorStep(length);
  const minorStep = majorStep / 5;
  const ticks = [];
  const total = Math.floor(length / minorStep);
  for (let index = 0; index <= total; index += 1) {
    const position = index * minorStep;
    ticks.push({ position, label: index % 5 === 0 ? String(position) : null });
  }
  return ticks;
}
