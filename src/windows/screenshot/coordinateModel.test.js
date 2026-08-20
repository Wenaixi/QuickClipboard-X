import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { formatCursorCoordinate, coordinatePanelPosition } from './coordinateModel.js';

const bounds = { width: 1920, height: 1080 };

test('formatCursorCoordinate 取整到逻辑像素并保持固定格式', () => {
  assert.equal(formatCursorCoordinate({ x: 123.4, y: 456.7 }), 'X: 123  Y: 457');
  assert.equal(formatCursorCoordinate({ x: 0, y: 0 }), 'X: 0  Y: 0');
});

test('formatCursorCoordinate 拒绝无效坐标', () => {
  assert.throws(() => formatCursorCoordinate({ x: Number.NaN, y: 1 }), /点 x 必须是有限数字/);
  assert.throws(() => formatCursorCoordinate({ x: 1 }), /点 y 必须是有限数字/);
});

test('coordinatePanelPosition 默认放在光标右下方', () => {
  const position = coordinatePanelPosition({ x: 400, y: 300 }, bounds);
  assert.deepEqual(position, { left: 412, top: 312 });
});

test('coordinatePanelPosition 靠近右下角时翻转到左上方并夹紧', () => {
  const position = coordinatePanelPosition({ x: 1900, y: 1060 }, bounds);
  assert.ok(position.left < 1900, '右侧放不下时应翻到左侧');
  assert.ok(position.top < 1060, '下方放不下时应翻到上方');
  assert.ok(position.left >= 8 && position.top >= 8);
  assert.ok(position.left + 96 <= bounds.width - 8);
  assert.ok(position.top + 26 <= bounds.height - 8);
});

test('coordinatePanelPosition 源码默认间隙 12 边距 8 且翻转到对侧夹紧', () => {
  const source = readFileSync(new URL('./coordinateModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function coordinatePanelPosition');
  const body = source.slice(start, start + 1300);
  // 源码护栏一：默认间隙必须为 12px（光标与面板的视觉间距）。
  assert.ok(body.includes('const gap = options.gap ?? 12;'), '默认间隙必须为 12');
  // 源码护栏二：默认边距必须为 8px（面板与显示器边缘的安全距离）。
  assert.ok(body.includes('const margin = options.margin ?? 8;'), '默认边距必须为 8');
  // 源码护栏三：越界时翻转到对侧（右侧放不下翻左侧、下方放不下翻上方）。
  assert.ok(body.includes('point.x - gap - width'), '右侧放不下必须翻到左侧');
  assert.ok(body.includes('point.y - gap - height'), '下方放不下必须翻到上方');
  // 行为属性：默认右下 12px 偏移；四角越界全部翻转且面板整体在边界内。
  const corners = [{ x: 1900, y: 1060 }, { x: 10, y: 10 }, { x: 1900, y: 10 }, { x: 10, y: 1060 }];
  for (const point of corners) {
    const position = coordinatePanelPosition(point, bounds);
    assert.ok(position.left >= 8 && position.left + 96 <= bounds.width - 8, '面板水平必须在边界内');
    assert.ok(position.top >= 8 && position.top + 26 <= bounds.height - 8, '面板垂直必须在边界内');
  }
  const flippedRight = coordinatePanelPosition({ x: 1900, y: 400 }, bounds);
  assert.ok(flippedRight.left + 96 < 1900, '右侧放不下必须翻到左侧');
  const flippedBottom = coordinatePanelPosition({ x: 400, y: 1060 }, bounds);
  assert.ok(flippedBottom.top + 26 < 1060, '下方放不下必须翻到上方');
});

test('coordinatePanelPosition 拒绝无效输入或负尺寸', () => {
  assert.throws(() => coordinatePanelPosition({ x: 1, y: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => coordinatePanelPosition({ x: 1, y: 1 }, bounds, { width: 0 }), /面板尺寸必须为正数/);
});
