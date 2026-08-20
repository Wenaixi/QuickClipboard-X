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

test('coordinatePanelPosition 翻转分支双重夹紧区间且恰好边界不翻转', () => {
  const source = readFileSync(new URL('./coordinateModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function coordinatePanelPosition');
  const body = source.slice(start, start + 1300);
  // 源码护栏：翻转分支必须用双重夹紧（Math.max(margin, Math.min(point.x - gap - width,
  // bounds.width - width - margin))）——翻到左侧时左缘至少 margin、右缘至多 width - margin。
  assert.ok(body.includes('Math.max(margin, Math.min(point.x - gap - width, bounds.width - width - margin))'), '翻转 left 必须双重夹紧到边距区间');
  assert.ok(body.includes('Math.max(margin, Math.min(point.y - gap - height, bounds.height - height - margin))'), '翻转 top 必须双重夹紧到边距区间');
  // 行为：翻转到左侧后 left 必须落在 [margin, width-面板宽-margin] 区间（含边界），
  // 即面板整体完全在边距内，不允许任一侧越界。
  const flipped = coordinatePanelPosition({ x: 1910, y: 1070 }, bounds);
  assert.ok(flipped.left >= 8 && flipped.left <= bounds.width - 96 - 8, '翻转后 left 必须在边距区间内');
  assert.ok(flipped.top >= 8 && flipped.top <= bounds.height - 26 - 8, '翻转后 top 必须在边距区间内');
  // 精确边界：x + gap + width 恰好等于 bounds.width - margin 时仍放在右侧（不翻转）。
  const atRightEdge = coordinatePanelPosition({ x: bounds.width - 96 - 12 - 8, y: 300 }, bounds);
  assert.equal(atRightEdge.left, bounds.width - 96 - 12 - 8 + 12, '恰好贴合右边界仍放右侧不翻转');
  // 再大一像素必须翻转且翻到左侧：left 应等于 clamp 到边距区间的 point.x - gap - width。
  const pastX = bounds.width - 96 - 12 - 8 + 1;
  const pastRightEdge = coordinatePanelPosition({ x: pastX, y: 300 }, bounds);
  assert.equal(pastRightEdge.left, Math.max(8, Math.min(pastX - 12 - 96, bounds.width - 96 - 8)), '超过右边界必须翻到左侧并夹紧');
  assert.ok(pastRightEdge.left < pastX, '翻到左侧后面板必须在光标左侧');
});

test('coordinatePanelPosition 拒绝无效输入或负尺寸', () => {
  assert.throws(() => coordinatePanelPosition({ x: 1, y: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => coordinatePanelPosition({ x: 1, y: 1 }, bounds, { width: 0 }), /面板尺寸必须为正数/);
});
