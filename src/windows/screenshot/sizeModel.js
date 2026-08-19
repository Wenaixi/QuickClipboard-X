function assertSize(size, name) {
  if (!size || typeof size !== 'object') {
    throw new TypeError(`${name} 尺寸对象缺失`);
  }
  if (!Number.isFinite(size.width)) {
    throw new TypeError('宽度 必须是有限数字');
  }
  if (!Number.isFinite(size.height)) {
    throw new TypeError('高度 必须是有限数字');
  }
  if (size.width < 0 || size.height < 0) {
    throw new RangeError('宽度与高度必须为非负数');
  }
}

// 格式化选区尺寸为 `宽 × 高` 像素文本（ShareX 公开行为：选区标签显示像素尺寸）。
export function formatPixelSize(size) {
  assertSize(size, '像素尺寸');
  return `${Math.round(size.width)} × ${Math.round(size.height)}`;
}

// 格式化选区尺寸为百万像素文本，保留一位小数。
export function formatMegapixels(size) {
  assertSize(size, '百万像素');
  const megapixels = (size.width * size.height) / 1_000_000;
  return `${megapixels.toFixed(1)} MP`;
}

function greatestCommonDivisor(a, b) {
  let x = a;
  let y = b;
  while (y !== 0) {
    const remainder = x % y;
    x = y;
    y = remainder;
  }
  return x;
}

// 格式化选区宽高比为最简整数比（ShareX 公开行为：选区信息显示如 16:9），先取整再化简。
export function formatAspectRatio(size) {
  assertSize(size, '宽高比');
  const width = Math.round(size.width);
  const height = Math.round(size.height);
  if (width <= 0 || height <= 0) {
    throw new RangeError('宽度与高度必须为正数');
  }
  const divisor = greatestCommonDivisor(width, height);
  return `${width / divisor}:${height / divisor}`;
}
