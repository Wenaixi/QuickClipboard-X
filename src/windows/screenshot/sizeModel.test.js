import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatPixelSize, formatMegapixels } from './sizeModel.js';

test('formatPixelSize 按国际规范输出像素尺寸', () => {
  assert.equal(formatPixelSize({ width: 1920, height: 1080 }), '1920 × 1080');
  assert.equal(formatPixelSize({ width: 1, height: 1 }), '1 × 1');
});

test('formatPixelSize 取整到整数像素', () => {
  assert.equal(formatPixelSize({ width: 1920.6, height: 1080.4 }), '1921 × 1080');
});

test('formatPixelSize 拒绝非法输入', () => {
  assert.throws(() => formatPixelSize(null), /尺寸对象/);
  assert.throws(() => formatPixelSize({ width: -1, height: 1080 }), /宽度与高度必须为非负数/);
  assert.throws(() => formatPixelSize({ width: 1920, height: Number.NaN }), /高度 必须是有限数字/);
});

test('formatMegapixels 输出百万像素并保留一位小数', () => {
  assert.equal(formatMegapixels({ width: 1920, height: 1080 }), '2.1 MP');
  assert.equal(formatMegapixels({ width: 1, height: 1 }), '0.0 MP');
});

test('formatMegapixels 大尺寸保留一位小数', () => {
  assert.equal(formatMegapixels({ width: 3840, height: 2160 }), '8.3 MP');
});

test('formatMegapixels 拒绝非法输入', () => {
  assert.throws(() => formatMegapixels({ width: 1920, height: -1 }), /宽度与高度必须为非负数/);
});
