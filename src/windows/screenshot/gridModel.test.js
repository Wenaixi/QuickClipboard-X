import { test } from 'node:test';
import assert from 'node:assert/strict';
import { thirdsGrid } from './gridModel.js';

test('thirdsGrid 按三分位返回贯穿画布的辅助线', () => {
  const lines = thirdsGrid({ width: 300, height: 150 });
  assert.deepEqual(lines.vertical, [
    { left: 100, top: 0, width: 1, height: 150 },
    { left: 200, top: 0, width: 1, height: 150 },
  ]);
  assert.deepEqual(lines.horizontal, [
    { left: 0, top: 50, width: 300, height: 1 },
    { left: 0, top: 100, width: 300, height: 1 },
  ]);
});

test('thirdsGrid 尺寸不可整除时按四舍五入取整', () => {
  const lines = thirdsGrid({ width: 800, height: 600 });
  assert.deepEqual(lines.vertical.map((line) => line.left), [267, 533]);
  assert.deepEqual(lines.horizontal.map((line) => line.top), [200, 400]);
});

test('thirdsGrid 任意合法边界下两条三分线不重合且贯穿画布', () => {
  const cases = [
    { width: 300, height: 150 },
    { width: 800, height: 600 },
    { width: 1920, height: 1080 },
    { width: 1366, height: 768 },
    { width: 1, height: 1 },
  ];
  for (const bounds of cases) {
    const grid = thirdsGrid(bounds);
    // 两条垂直线按 1/3 与 2/3 四舍五入，宽度足够时不得重合。
    const [v1, v2] = grid.vertical;
    assert.ok(v1.left < v2.left, `宽 ${bounds.width} 垂直线必须分开`);
    assert.ok(v1.height === bounds.height && v2.height === bounds.height, '垂直线必须贯穿画布');
    const [h1, h2] = grid.horizontal;
    assert.ok(h1.top < h2.top, `高 ${bounds.height} 水平线必须分开`);
    assert.ok(h1.width === bounds.width && h2.width === bounds.width, '水平线必须贯穿画布');
  }
});

test('thirdsGrid 拒绝非法边界', () => {
  assert.throws(() => thirdsGrid(null), /边界尺寸/);
  assert.throws(() => thirdsGrid({ width: 0, height: 100 }), /边界尺寸必须为正数/);
  assert.throws(() => thirdsGrid({ width: 100, height: Number.NaN }), /高度 必须是有限数字/);
});
