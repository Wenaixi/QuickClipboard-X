import { test } from 'node:test';
import assert from 'node:assert/strict';
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

test('coordinatePanelPosition 拒绝无效输入或负尺寸', () => {
  assert.throws(() => coordinatePanelPosition({ x: 1, y: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => coordinatePanelPosition({ x: 1, y: 1 }, bounds, { width: 0 }), /面板尺寸必须为正数/);
});
