import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('formatAspectRatio 先取整再欧几里得化简为最简整数比', () => {
  const source = readFileSync(new URL('./sizeModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function formatAspectRatio');
  const body = source.slice(start, start + 500);
  // 源码护栏：必须先取整（小数尺寸四舍五入）再用统一 GCD 化简，禁止内联循环。
  assert.ok(body.includes('Math.round(size.width)'), '宽度必须先取整');
  assert.ok(body.includes('Math.round(size.height)'), '高度必须先取整');
  assert.ok(body.includes('greatestCommonDivisor(width, height)'), '必须用统一 GCD 化简');
  assert.ok(body.includes('return `${width / divisor}:${height / divisor}`;'), '必须输出最简整数比');
  // 行为属性：常见比例最简、互质保持、小数先取整、极端大数不退化。
  assert.equal(formatAspectRatio({ width: 1920, height: 1080 }), '16:9');
  assert.equal(formatAspectRatio({ width: 1366, height: 768 }), '683:384', '1366:768 可被 2 化简');
  assert.equal(formatAspectRatio({ width: 300.6, height: 200.4 }), '301:200');
  assert.equal(formatAspectRatio({ width: 3840, height: 2160 }), '16:9', '4K 必须化简');
});

test('尺寸格式化源码语义完整（像素取整与百万像素单位）', () => {
  const source = readFileSync(new URL('./sizeModel.js', import.meta.url), 'utf8');
  // 源码护栏一：像素尺寸必须四舍五入到整数并用 × 分隔（与选区标签文案一致）。
  assert.ok(source.includes("return `${Math.round(size.width)} × ${Math.round(size.height)}`;"), '像素尺寸必须取整且用 × 分隔');
  // 源码护栏二：百万像素必须除以 1_000_000（避免手写零数错位）。
  assert.ok(source.includes('(size.width * size.height) / 1_000_000'), '百万像素必须除以百万');
  // 源码护栏三：百万像素必须保留一位小数并带 MP 单位。
  assert.ok(source.includes("return `${megapixels.toFixed(1)} MP`;"), '百万像素必须保留一位小数并带 MP 单位');
  // 行为属性：取整边界、百万像素小数位、零尺寸退化。
  assert.equal(formatPixelSize({ width: 1920.5, height: 1080.5 }), '1921 × 1081');
  assert.equal(formatPixelSize({ width: 0.4, height: 0.4 }), '0 × 0');
  assert.equal(formatMegapixels({ width: 2000, height: 1000 }), '2.0 MP');
  assert.equal(formatMegapixels({ width: 3840, height: 2160 }), '8.3 MP');
  assert.equal(formatMegapixels({ width: 0, height: 100 }), '0.0 MP');
});

test('physicalSize 带边界分支源码显式 ceil 右缘 floor 左缘且不可被回退替代', () => {
  const source = readFileSync(new URL('./sizeModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function physicalSize');
  const body = source.slice(start, start + 500);
  // 源码护栏一：带边界分支必须显式右/下 ceil 减左/上 floor——保证标签像素数与
  // selectionToPhysical 实际截图完全一致（floor 起点 ceil 终点覆盖完整像素范围）。
  assert.ok(body.includes('Math.ceil(size.right * dpr) - Math.floor(size.left * dpr)'), '宽度必须右缘 ceil 减左缘 floor');
  assert.ok(body.includes('Math.ceil(size.bottom * dpr) - Math.floor(size.top * dpr)'), '高度必须下缘 ceil 减上缘 floor');
  // 源码护栏二：hasEdges 必须要求四边全部有限（缺任一边界不得走边缘分支，避免与截图像素不一致）。
  assert.ok(body.includes('Number.isFinite(size.left) && Number.isFinite(size.top)'), 'hasEdges 必须同时要求 left/top 有限');
  assert.ok(body.includes('Number.isFinite(size.right) && Number.isFinite(size.bottom)'), 'hasEdges 必须同时要求 right/bottom 有限');
  // 行为佐证：1.25 DPR 下边缘分支（126x88）与回退分支（round(99.8*1.25)=125）结果不同，
  // 证明带边界分支不可被回退策略替代（否则标签像素数与实际截图不一致）。
  const edges = { left: 100.4, top: 50.6, right: 200.2, bottom: 120.8, width: 99.8, height: 70.2 };
  assert.deepEqual(physicalSize(edges, 1.25), { width: 126, height: 88 });
  const fallback = physicalSize({ width: 99.8, height: 70.2 }, 1.25);
  assert.notDeepEqual(fallback, { width: 126, height: 88 }, '回退分支不得与边缘分支结果相同（证明不可替代）');
});

test('formatAspectRatio 源码 GCD 欧几里得实现且先取整后化简保证最简比', () => {
  const source = readFileSync(new URL('./sizeModel.js', import.meta.url), 'utf8');
  const gcdStart = source.indexOf('function greatestCommonDivisor');
  const gcdBody = source.slice(gcdStart, gcdStart + 400);
  // 源码护栏一：GCD 必须用标准欧几里得辗转相除（while 余数循环），
  // 禁止暴力递减（性能退化到 O(min(a,b))）或提前返回错误 divisor。
  assert.ok(gcdBody.includes('while (y !== 0) {'), 'GCD 必须用 while 余数循环');
  assert.ok(gcdBody.includes('const remainder = x % y;'), 'GCD 必须用取余计算');
  assert.ok(gcdBody.includes('x = y;'), 'GCD 必须交换被除数');
  assert.ok(gcdBody.includes('y = remainder;'), 'GCD 必须用余数继续');
  assert.ok(gcdBody.includes('return x;'), 'GCD 必须返回最后非零除数');
  // 行为属性：常见比例最简、互质保持、大数快速化简、极端 1 像素比例不退化。
  assert.equal(formatAspectRatio({ width: 1920, height: 1080 }), '16:9');
  assert.equal(formatAspectRatio({ width: 3840, height: 2160 }), '16:9');
  assert.equal(formatAspectRatio({ width: 1366, height: 768 }), '683:384');
  assert.equal(formatAspectRatio({ width: 7, height: 11 }), '7:11');
  assert.equal(formatAspectRatio({ width: 1000, height: 1 }), '1000:1');
  assert.equal(formatAspectRatio({ width: 3840, height: 1 }), '3840:1', '极端大宽高比不得退化');
  assert.equal(formatAspectRatio({ width: 1920, height: 1 }), '1920:1');
});

test('formatAspectRatio 拒绝零或非法输入', () => {
  assert.throws(() => formatAspectRatio({ width: 0, height: 100 }), /宽度与高度必须为正数/);
  assert.throws(() => formatAspectRatio(null), /尺寸对象/);
  assert.throws(() => formatAspectRatio({ width: -1, height: 100 }), /宽度与高度必须为非负数/);
  assert.throws(() => formatAspectRatio({ width: 1920, height: Number.NaN }), /高度 必须是有限数字/);
});
