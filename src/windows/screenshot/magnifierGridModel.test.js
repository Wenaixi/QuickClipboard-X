import { test } from 'node:test';
import assert from 'node:assert/strict';
import { magnifierGridLines, magnifierCrosshair } from './magnifierGridModel.js';

test('magnifierGridLines 按缩放倍率间隔输出网格线且不越界', () => {
  const grid = magnifierGridLines({ panel: { width: 168, height: 168 }, scale: 6 });
  assert.deepEqual(grid.vertical.slice(0, 3), [6, 12, 18]);
  assert.deepEqual(grid.horizontal.slice(0, 3), [6, 12, 18]);
  assert.ok(grid.vertical.every((x) => x > 0 && x < 168));
  assert.ok(grid.horizontal.every((y) => y > 0 && y < 168));
  assert.equal(grid.vertical.length, 27);
  assert.equal(grid.horizontal.length, 27);
});

test('magnifierGridLines 面板小于倍率时不输出网格线', () => {
  const grid = magnifierGridLines({ panel: { width: 10, height: 10 }, scale: 20 });
  assert.deepEqual(grid.vertical, []);
  assert.deepEqual(grid.horizontal, []);
});

test('magnifierGridLines 支持矩形面板与自定义倍率', () => {
  const grid = magnifierGridLines({ panel: { width: 100, height: 50 }, scale: 10 });
  assert.equal(grid.vertical.length, 9);
  assert.equal(grid.horizontal.length, 4);
});

test('magnifierCrosshair 取面板几何中心', () => {
  assert.deepEqual(magnifierCrosshair({ panel: { width: 168, height: 168 }, scale: 6 }), { x: 84, y: 84 });
  assert.deepEqual(magnifierCrosshair({ panel: { width: 100, height: 50 }, scale: 10 }), { x: 50, y: 25 });
});

test('magnifierGridLines 与 magnifierCrosshair 拒绝无效输入', () => {
  assert.throws(() => magnifierGridLines(null), /几何/);
  assert.throws(() => magnifierGridLines({ panel: { width: 0, height: 1 }, scale: 1 }), /面板/);
  assert.throws(() => magnifierGridLines({ panel: { width: 1, height: 1 }, scale: 0 }), /倍率/);
  assert.throws(() => magnifierCrosshair({ panel: { width: 1, height: 1 }, scale: 0 }), /倍率/);
});
