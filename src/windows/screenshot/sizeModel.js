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
