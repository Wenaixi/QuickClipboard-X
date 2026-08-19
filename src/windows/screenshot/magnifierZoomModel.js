function assertFiniteNumber(value, name) {
  if (!Number.isFinite(value)) {
    throw new TypeError(`${name} 必须是有限数字`);
  }
}

const DEFAULT_MIN = 2;
const DEFAULT_MAX = 24;
const DEFAULT_STEP = 1;

// 根据鼠标滚轮增量计算新的放大镜缩放倍率（ShareX 公开行为：滚轮调整放大镜缩放）。
// 向上滚（deltaY < 0）放大一步、向下滚缩小一步，结果夹紧到 [min, max] 且步长固定。
export function magnifierScaleForWheel(currentScale, deltaY, options = {}) {
  assertFiniteNumber(currentScale, '当前缩放倍率');
  assertFiniteNumber(deltaY, '滚轮增量');
  const min = options.min ?? DEFAULT_MIN;
  const max = options.max ?? DEFAULT_MAX;
  const step = options.step ?? DEFAULT_STEP;
  assertFiniteNumber(min, '最小缩放倍率');
  assertFiniteNumber(max, '最大缩放倍率');
  assertFiniteNumber(step, '缩放步长');
  if (min <= 0 || max < min || step <= 0) {
    throw new RangeError('缩放范围与步长必须为正数且最大值不小于最小值');
  }
  if (deltaY === 0) return currentScale;
  const direction = deltaY < 0 ? 1 : -1;
  return Math.min(max, Math.max(min, currentScale + direction * step));
}
