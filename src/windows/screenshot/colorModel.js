function clampChannel(value) {
  return Math.min(255, Math.max(0, Math.round(value)));
}

// 从 RGBA 平铺字节数组读取中心像素（ShareX 公开行为：放大镜同时显示光标下像素的颜色值）。
// 偶数尺寸时取右上中心像素；数据长度不足或尺寸非法时抛异常。
export function readCenterPixel(data, width, height) {
  if (!width || !Number.isInteger(width) || !height || !Number.isInteger(height) || width <= 0 || height <= 0) {
    throw new RangeError('宽度与高度必须为正数');
  }
  if (!data || data.length < width * height * 4) {
    throw new RangeError('背景快照数据长度不足');
  }
  const x = Math.floor(width / 2);
  const y = Math.floor((height - 1) / 2);
  const index = (y * width + x) * 4;
  return { r: data[index], g: data[index + 1], b: data[index + 2] };
}

function assertRgb(rgb) {
  if (!rgb || typeof rgb !== 'object') {
    throw new TypeError('颜色对象缺失');
  }
  if (rgb.b === undefined) {
    throw new TypeError('缺少蓝色通道');
  }
  if (!Number.isFinite(rgb.r) || !Number.isFinite(rgb.g) || !Number.isFinite(rgb.b)) {
    throw new TypeError('颜色通道必须是有限数字');
  }
}

// 格式化为 RGB(r, g, b) 文本，通道夹紧到 0-255。
export function formatRgb(rgb) {
  assertRgb(rgb);
  return `RGB(${clampChannel(rgb.r)}, ${clampChannel(rgb.g)}, ${clampChannel(rgb.b)})`;
}

// 格式化为 #RRGGBB 大写十六进制，供色块背景使用。
export function hexFromRgb(rgb) {
  assertRgb(rgb);
  const toHex = (value) => clampChannel(value).toString(16).padStart(2, '0').toUpperCase();
  return `#${toHex(rgb.r)}${toHex(rgb.g)}${toHex(rgb.b)}`;
}
