import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('magnifierGeometry 任意光标位置面板不越界且采样源完整', () => {
  const corners = [
    { x: 0, y: 0 },
    { x: bounds.width - 1, y: 0 },
    { x: 0, y: bounds.height - 1 },
    { x: bounds.width - 1, y: bounds.height - 1 },
    { x: bounds.width / 2, y: bounds.height / 2 },
    { x: -50, y: -50 },
    { x: bounds.width + 50, y: bounds.height + 50 },
  ];
  for (const point of corners) {
    const geometry = magnifierGeometry(point, bounds);
    // 面板必须完全落在显示器边界内（含 8px 边距）。
    assert.ok(geometry.panel.left >= 8, `left=${point.x} 面板 left 越界`);
    assert.ok(geometry.panel.top >= 8, `top=${point.y} 面板 top 越界`);
    assert.ok(geometry.panel.left + geometry.panel.width <= bounds.width - 8, `left=${point.x} 面板右缘越界`);
    assert.ok(geometry.panel.top + geometry.panel.height <= bounds.height - 8, `top=${point.y} 面板下缘越界`);
    // 采样源必须完整落在显示器边界内。
    assert.ok(geometry.source.left >= 0 && geometry.source.left + geometry.source.cols <= bounds.width, '采样源横向越界');
    assert.ok(geometry.source.top >= 0 && geometry.source.top + geometry.source.rows <= bounds.height, '采样源纵向越界');
  }
});

test('magnifierGeometry 源码默认常量与采样源按面板尺寸推导', () => {
  const source = readFileSync(new URL('./magnifierModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function magnifierGeometry');
  const body = source.slice(start, start + 1200);
  // 源码护栏一：默认倍率必须为 6（与 App 放大镜默认缩放一致）。
  assert.ok(body.includes('const scale = options.scale ?? 6;'), '默认倍率必须为 6');
  // 源码护栏二：面板默认尺寸必须为 168x168（正方形放大镜面板）。
  assert.ok(body.includes('const panelWidth = options.panelWidth ?? 168;'), '默认面板宽度必须为 168');
  assert.ok(body.includes('const panelHeight = options.panelHeight ?? 168;'), '默认面板高度必须为 168');
  // 源码护栏三：采样源格数必须由面板尺寸除以倍率推导（面板每格对应一个源像素）。
  assert.ok(body.includes('const cols = Math.max(1, Math.floor(panelWidth / scale));'), '采样列数必须由面板宽除以倍率推导');
  assert.ok(body.includes('const rows = Math.max(1, Math.floor(panelHeight / scale));'), '采样行数必须由面板高除以倍率推导');
  // 行为属性：默认面板 168x168 倍率 6 时采样源为 28x28 格。
  const geometry = magnifierGeometry({ x: 400, y: 300 }, bounds);
  assert.equal(geometry.scale, 6);
  assert.deepEqual(geometry.panel, { left: 416, top: 316, width: 168, height: 168 });
  assert.deepEqual(geometry.source.cols, 28, '168/6 必须得到 28 列');
  assert.deepEqual(geometry.source.rows, 28, '168/6 必须得到 28 行');
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

test('sampleMagnifierGrid 越界几何兜底夹紧到快照末像素且源码显式 min 防护', () => {
  const source = readFileSync(new URL('./magnifierModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function sampleMagnifierGrid');
  const body = source.slice(start, start + 700);
  // 源码护栏：采样坐标必须显式夹紧到快照边界（min(snapshotWidth-1) / min(snapshotHeight-1)），
  // 即使 geometry.source 未夹紧（外部构造的越界几何）也不得越界读 Uint8Array。
  assert.ok(body.includes('const x = Math.min(snapshotWidth - 1, Math.floor(area.left) + col);'), 'x 必须夹紧到快照宽-1');
  assert.ok(body.includes('const y = Math.min(snapshotHeight - 1, Math.floor(area.top) + row);'), 'y 必须夹紧到快照高-1');
  // 行为验证：构造越界几何（left/top 远超快照），采样必须返回末像素而非越界崩溃。
  const snapshotWidth = 4;
  const snapshotHeight = 4;
  const data = new Uint8Array(snapshotWidth * snapshotHeight * 4);
  for (let i = 0; i < data.length; i += 1) {
    data[i] = 7;
  }
  const lastIndex = (3 * snapshotWidth + 3) * 4;
  data[lastIndex] = 99;
  const outOfBounds = { source: { left: 100, top: 100, cols: 2, rows: 2 } };
  const grid = sampleMagnifierGrid(data, outOfBounds, snapshotWidth, snapshotHeight);
  assert.equal(grid.length, 2, '必须按 rows 返回行数');
  assert.deepEqual(grid[1][1], [99, 7, 7, 7], '越界采样必须夹紧到末像素 (3,3) 的 RGBA');
});

test('sampleMagnifierGrid 数据不足时拒绝并保持空输入兼容', () => {
  assert.throws(() => sampleMagnifierGrid(new Uint8Array(3), { source: { left: 0, top: 0, cols: 1, rows: 1 } }, 10, 10), /背景快照数据长度不足/);
  assert.deepEqual(sampleMagnifierGrid(null, null, 10, 10), []);
});
