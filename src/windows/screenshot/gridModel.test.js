import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('thirdsGrid 源码按 1/3 与 2/3 四舍五入且双线贯穿对称', () => {
  const source = readFileSync(new URL('./gridModel.js', import.meta.url), 'utf8');
  // 源码护栏：两条线必须分别由宽度/高度的 1/3 与 2/3 四舍五入得到（禁止硬编码比例）。
  assert.ok(source.includes('Math.round(bounds.width / 3)'), '第一条垂直线必须按 1/3 取整');
  assert.ok(source.includes('Math.round((bounds.width * 2) / 3)'), '第二条垂直线必须按 2/3 取整');
  assert.ok(source.includes('Math.round(bounds.height / 3)'), '第一条水平线必须按 1/3 取整');
  assert.ok(source.includes('Math.round((bounds.height * 2) / 3)'), '第二条水平线必须按 2/3 取整');
  // 行为属性：精确可整除时落点精确、双线贯穿全轴、两侧间距对称。
  const grid = thirdsGrid({ width: 1920, height: 1080 });
  assert.deepEqual(grid.vertical.map((l) => l.left), [640, 1280], '1920 精确三分');
  assert.deepEqual(grid.horizontal.map((l) => l.top), [360, 720], '1080 精确三分');
  for (const bounds of [{ width: 1920, height: 1080 }, { width: 800, height: 600 }, { width: 1366, height: 768 }]) {
    const g = thirdsGrid(bounds);
    const [v1, v2] = g.vertical;
    const [h1, h2] = g.horizontal;
    assert.equal(v1.height, bounds.height, '垂直线必须贯穿全高');
    assert.equal(h1.width, bounds.width, '水平线必须贯穿全宽');
    assert.ok(v2.left - v1.left > 0, '两条垂直线不得重合');
    assert.ok(h2.top - h1.top > 0, '两条水平线不得重合');
    // 对称性：第二线到右缘的距离与第一线到左缘的距离差不超过 1px（四舍五入误差）。
    assert.ok(Math.abs((bounds.width - v2.left) - v1.left) <= 1, '垂直线必须左右对称');
    assert.ok(Math.abs((bounds.height - h2.top) - h1.top) <= 1, '水平线必须上下对称');
  }
});

test('thirdsGrid 拒绝非法边界', () => {
  assert.throws(() => thirdsGrid(null), /边界尺寸/);
  assert.throws(() => thirdsGrid({ width: 0, height: 100 }), /边界尺寸必须为正数/);
  assert.throws(() => thirdsGrid({ width: 100, height: Number.NaN }), /高度 必须是有限数字/);
});
