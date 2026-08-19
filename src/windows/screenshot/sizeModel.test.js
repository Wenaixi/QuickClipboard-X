import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatPixelSize, formatMegapixels, formatAspectRatio, physicalSize } from './sizeModel.js';

test('physicalSize 与 selectionToPhysical 物理取整策略一致', () => {
  // 尺寸标签显示的像素数必须与实际截图宽度一致：右边界向上取整减左边界向下取整。
  const size = physicalSize({ left: 100.4, top: 50.6, right: 200.2, bottom: 120.8, width: 99.8, height: 70.2 }, 1.25);
  assert.equal(size.width, 126);
  assert.equal(size.height, 88);
});

test('physicalSize 无边信息时回退为宽高乘 DPR 四舍五入', () => {
  const size = physicalSize({ width: 100, height: 60 }, 1.5);
  assert.equal(size.width, 150);
  assert.equal(size.height, 90);
});

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

test('formatAspectRatio 按最简整数比输出宽高比', () => {
  assert.equal(formatAspectRatio({ width: 1920, height: 1080 }), '16:9');
  assert.equal(formatAspectRatio({ width: 300, height: 200 }), '3:2');
  assert.equal(formatAspectRatio({ width: 1, height: 1 }), '1:1');
});

test('formatAspectRatio 互质尺寸保持原比例', () => {
  assert.equal(formatAspectRatio({ width: 7, height: 11 }), '7:11');
  assert.equal(formatAspectRatio({ width: 1000, height: 1 }), '1000:1');
});

test('formatAspectRatio 先取整再化简', () => {
  assert.equal(formatAspectRatio({ width: 1920.2, height: 1080.4 }), '16:9');
  assert.equal(formatAspectRatio({ width: 300.6, height: 200.4 }), '301:200');
});

test('physicalSize 默认按 1:1 输出物理像素尺寸', () => {
  assert.deepEqual(physicalSize({ width: 1200, height: 700 }, 1), { width: 1200, height: 700 });
});

test('physicalSize 按 DPR 换算并四舍五入到整数像素', () => {
  assert.deepEqual(physicalSize({ width: 1200, height: 700 }, 1.5), { width: 1800, height: 1050 });
  assert.deepEqual(physicalSize({ width: 100.4, height: 200.6 }, 1.5), { width: 151, height: 301 });
});

test('physicalSize 拒绝非法 DPR 或尺寸', () => {
  assert.throws(() => physicalSize({ width: 1200, height: 700 }, 0), /dpr 必须是正数/);
  assert.throws(() => physicalSize({ width: 1200, height: 700 }, Number.NaN), /dpr 必须是正数/);
  assert.throws(() => physicalSize(null, 1), /尺寸对象/);
});

test('formatAspectRatio 拒绝零或非法输入', () => {
  assert.throws(() => formatAspectRatio({ width: 0, height: 100 }), /宽度与高度必须为正数/);
  assert.throws(() => formatAspectRatio(null), /尺寸对象/);
  assert.throws(() => formatAspectRatio({ width: -1, height: 100 }), /宽度与高度必须为非负数/);
  assert.throws(() => formatAspectRatio({ width: 1920, height: Number.NaN }), /高度 必须是有限数字/);
});
