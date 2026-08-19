import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readCenterPixel, formatRgb, hexFromRgb } from './colorModel.js';

function rgbaPixel(data, width, x, y, r, g, b, a = 255) {
  const index = (y * width + x) * 4;
  data[index] = r;
  data[index + 1] = g;
  data[index + 2] = b;
  data[index + 3] = a;
}

test('readCenterPixel 从 RGBA 平铺数组读取中心像素', () => {
  const width = 3;
  const height = 3;
  const data = new Uint8ClampedArray(width * height * 4);
  rgbaPixel(data, width, 0, 0, 255, 0, 0);
  rgbaPixel(data, width, 1, 1, 10, 20, 30);
  rgbaPixel(data, width, 2, 2, 0, 0, 255);
  assert.deepEqual(readCenterPixel(data, width, height), { r: 10, g: 20, b: 30 });
});

test('readCenterPixel 偶数尺寸取右上中心像素', () => {
  const width = 4;
  const height = 4;
  const data = new Uint8ClampedArray(width * height * 4);
  rgbaPixel(data, width, 2, 1, 7, 8, 9);
  assert.deepEqual(readCenterPixel(data, width, height), { r: 7, g: 8, b: 9 });
});

test('readCenterPixel 拒绝长度不足或非法尺寸', () => {
  assert.throws(() => readCenterPixel(new Uint8ClampedArray(8), 2, 2), /背景快照数据长度不足/);
  assert.throws(() => readCenterPixel(new Uint8ClampedArray(16), 0, 2), /宽度与高度必须为正数/);
});

test('formatRgb 输出 RGB 格式并夹紧越界通道', () => {
  assert.equal(formatRgb({ r: 255, g: 0, b: 128 }), 'RGB(255, 0, 128)');
  assert.equal(formatRgb({ r: 300, g: -5, b: 42 }), 'RGB(255, 0, 42)');
});

test('formatRgb 拒绝非法输入', () => {
  assert.throws(() => formatRgb(null), /颜色对象/);
  assert.throws(() => formatRgb({ r: 1, g: 2 }), /缺少蓝色通道/);
});

test('hexFromRgb 输出大写十六进制', () => {
  assert.equal(hexFromRgb({ r: 255, g: 0, b: 128 }), '#FF0080');
  assert.equal(hexFromRgb({ r: 0, g: 0, b: 0 }), '#000000');
});
