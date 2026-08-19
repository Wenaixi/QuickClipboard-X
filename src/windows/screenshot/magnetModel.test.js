import { test } from 'node:test';
import assert from 'node:assert/strict';
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

test('magnetSelection 拒绝无效输入', () => {
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { tolerance: -1 }), /吸附容差不能为负数/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { edge: 'x' }), /edge 必须是空串或 n\/s\/e\/w 组合/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: Number.NaN }, bounds), /bottom 必须是有限数字/);
});
