import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { magnetSelection } from './magnetModel.js';

const bounds = { width: 1920, height: 1080 };

test('magnetSelection 平移靠近屏幕左缘时吸附到 0 且保持尺寸', () => {
  const result = magnetSelection({ left: 3, top: 100, right: 403, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 0, top: 100, right: 400, bottom: 400, width: 400, height: 300 });
});

test('magnetSelection 平移靠近屏幕右缘时吸附到宽度', () => {
  const result = magnetSelection({ left: 1517, top: 100, right: 1917, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 1520, top: 100, right: 1920, bottom: 400, width: 400, height: 300 });
});

test('magnetSelection 平移靠近垂直中心线时吸附到中心', () => {
  const result = magnetSelection({ left: 953, top: 100, right: 973, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 950, top: 100, right: 970, bottom: 400, width: 20, height: 300 });
});

test('magnetSelection 超出容差时保持原选区', () => {
  const result = magnetSelection({ left: 10, top: 100, right: 410, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 10, top: 100, right: 410, bottom: 400, width: 400, height: 300 });
});

test('magnetSelection 两轴同时靠近边缘时同时吸附', () => {
  const result = magnetSelection({ left: 2, top: 3, right: 402, bottom: 203 }, bounds);
  assert.deepEqual(result, { left: 0, top: 0, right: 400, bottom: 200, width: 400, height: 200 });
});

test('magnetSelection 调整大小拖 e 边靠近右缘时仅右边缘吸附', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 1915, bottom: 400 }, bounds, { edge: 'e' });
  assert.deepEqual(result, { left: 100, top: 100, right: 1920, bottom: 400, width: 1820, height: 300 });
});

test('magnetSelection 调整大小拖 e 边靠近垂直中心线时右边缘吸附到中心', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 958, bottom: 400 }, bounds, { edge: 'e' });
  assert.deepEqual(result, { left: 100, top: 100, right: 960, bottom: 400, width: 860, height: 300 });
});

test('magnetSelection 调整大小拖 w 边靠近左缘时仅左边缘吸附', () => {
  const result = magnetSelection({ left: 3, top: 100, right: 403, bottom: 400 }, bounds, { edge: 'w' });
  assert.deepEqual(result, { left: 0, top: 100, right: 403, bottom: 400, width: 403, height: 300 });
});

test('magnetSelection 调整大小拖角点 se 时双轴同时吸附', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 1915, bottom: 1075 }, bounds, { edge: 'se' });
  assert.deepEqual(result, { left: 100, top: 100, right: 1920, bottom: 1080, width: 1820, height: 980 });
});

test('magnetSelection 调整大小拖 s 边靠近水平中心线时下边缘吸附到中心', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 400, bottom: 542 }, bounds, { edge: 's' });
  assert.deepEqual(result, { left: 100, top: 100, right: 400, bottom: 540, width: 300, height: 440 });
});

test('magnetSelection 越界选区不产生负宽或零宽', () => {
  const bounds = { width: 800, height: 600 };
  const w = magnetSelection({ left: -100, top: 100, right: -50, bottom: 200 }, bounds, { edge: 'w' });
  assert.ok(w.width >= 1, 'edge=w 越界选区必须保持最小 1px 宽');
  assert.equal(w.left, 0);
  const n = magnetSelection({ left: 100, top: -100, right: 200, bottom: -50 }, bounds, { edge: 'n' });
  assert.ok(n.height >= 1, 'edge=n 越界选区必须保持最小 1px 高');
  assert.equal(n.top, 0);
  const pan = magnetSelection({ left: -5, top: -5, right: 805, bottom: 605 }, bounds);
  assert.ok(pan.right <= 800, '平移后右边界不得越界');
  assert.ok(pan.bottom <= 600, '平移后下边界不得越界');
});

test('magnetSelection 任何输入都返回边界内最小 1px 合法选区', () => {
  const source = readFileSync(new URL('./magnetModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function magnetSelection');
  const body = source.slice(start);
  // 平移与调整两条路径都必须以夹紧收尾，返回的选区不得越界或产生负尺寸。
  assert.ok(body.includes('left = clamp(left, 0, Math.max(0, bounds.width - snapWidth))'), '平移路径必须夹紧 left');
  assert.ok(body.includes('right = Math.min(left + snapWidth, bounds.width)'), '平移路径必须夹紧 right');
  assert.ok(body.includes('right = Math.max(right, left + 1)'), 'w 边调整必须保证最小 1px 宽度');
  assert.ok(body.includes('bottom = Math.max(bottom, top + 1)'), 'n 边调整必须保证最小 1px 高度');
  // 越界输入也必须产生合法选区。
  const pan = magnetSelection({ left: -5, top: -5, right: 805, bottom: 605 }, bounds);
  assert.ok(pan.width >= 1 && pan.height >= 1, '平移越界输入必须保持最小 1px');
  const w = magnetSelection({ left: -100, top: 100, right: -50, bottom: 200 }, bounds, { edge: 'w' });
  assert.ok(w.width >= 1 && w.height >= 1 && w.left >= 0, 'w 边越界输入必须夹紧且最小 1px');
});

test('magnetSelection 拒绝无效输入', () => {
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { tolerance: -1 }), /吸附容差不能为负数/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { edge: 'x' }), /edge 必须是空串或 n\/s\/e\/w 组合/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: Number.NaN }, bounds), /bottom 必须是有限数字/);
});
