import { test } from 'node:test';
import assert from 'node:assert/strict';
import { magnifierGeometry, sampleMagnifierGrid } from './magnifierModel.js';

const bounds = { width: 1920, height: 1080 };

test('magnifierGeometry 默认把面板放在光标右下且不越界', () => {
  const geometry = magnifierGeometry({ x: 400, y: 300 }, bounds);
  assert.deepEqual(geometry.panel, { left: 416, top: 316, width: 168, height: 168 });
  assert.equal(geometry.scale, 6);
  assert.ok(geometry.source.cols > 0 && geometry.source.rows > 0);
});

test('magnifierGeometry 光标靠近右下角时面板翻转到左上方', () => {
  const geometry = magnifierGeometry({ x: 1800, y: 1000 }, bounds);
  assert.ok(geometry.panel.left < 1800, '右侧放不下时应翻到左侧');
  assert.ok(geometry.panel.top < 1000, '下方放不下时应翻到上方');
  assert.ok(geometry.panel.left >= 8 && geometry.panel.top >= 8);
});

test('magnifierGeometry 采样源夹紧到显示器边界且不超出', () => {
  const geometry = magnifierGeometry({ x: 5, y: 5 }, bounds);
  assert.ok(geometry.source.left >= 0 && geometry.source.top >= 0);
  assert.ok(geometry.source.left + geometry.source.cols <= bounds.width);
  assert.ok(geometry.source.top + geometry.source.rows <= bounds.height);
});

test('magnifierGeometry 拒绝无效点、边界或负面板尺寸', () => {
  assert.throws(() => magnifierGeometry({ x: Number.NaN, y: 1 }, bounds), /点 x 必须是有限数字/);
  assert.throws(() => magnifierGeometry({ x: 1, y: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => magnifierGeometry({ x: 1, y: 1 }, bounds, { scale: -1 }), /面板尺寸与缩放倍率必须为正数/);
});

test('sampleMagnifierGrid 按几何从快照 RGBA 采样网格', () => {
  const snapshotWidth = 10;
  const snapshotHeight = 10;
  const source = new Uint8Array(snapshotWidth * snapshotHeight * 4);
  for (let i = 0; i < source.length; i += 1) {
    source[i] = (i / 4) % 255;
  }
  const geometry = magnifierGeometry({ x: 5, y: 5 }, { width: snapshotWidth, height: snapshotHeight }, {
    scale: 2,
    panelWidth: 20,
    panelHeight: 20,
    gap: 2,
    margin: 1,
  });
  const grid = sampleMagnifierGrid(source, geometry, snapshotWidth, snapshotHeight);
  assert.ok(grid.length > 0);
  assert.ok(grid[0].length > 0);
  assert.deepEqual(grid[0][0].length, 4);
});

test('sampleMagnifierGrid 数据不足时拒绝并保持空输入兼容', () => {
  assert.throws(() => sampleMagnifierGrid(new Uint8Array(3), { source: { left: 0, top: 0, cols: 1, rows: 1 } }, 10, 10), /背景快照数据长度不足/);
  assert.deepEqual(sampleMagnifierGrid(null, null, 10, 10), []);
});
