import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cursorForEdge, cursorForSelectionHover } from './cursorModel.js';

const bounds = { width: 1920, height: 1080 };

test('cursorForEdge 边缘方向映射为对应 CSS 光标', () => {
  assert.equal(cursorForEdge('e'), 'ew-resize');
  assert.equal(cursorForEdge('w'), 'ew-resize');
  assert.equal(cursorForEdge('n'), 'ns-resize');
  assert.equal(cursorForEdge('s'), 'ns-resize');
});

test('cursorForEdge 角点映射为对角调整光标', () => {
  assert.equal(cursorForEdge('ne'), 'nesw-resize');
  assert.equal(cursorForEdge('sw'), 'nesw-resize');
  assert.equal(cursorForEdge('nw'), 'nwse-resize');
  assert.equal(cursorForEdge('se'), 'nwse-resize');
});

test('cursorForEdge 未知或空边缘回退到十字光标', () => {
  assert.equal(cursorForEdge(null), 'crosshair');
  assert.equal(cursorForEdge(''), 'crosshair');
  assert.equal(cursorForEdge('diagonal'), 'crosshair');
});

test('cursorForSelectionHover 选区内部返回移动光标', () => {
  const selection = { left: 400, top: 300, right: 700, bottom: 500, width: 300, height: 200 };
  assert.equal(cursorForSelectionHover({ x: 550, y: 400 }, selection, bounds), 'move');
});

test('cursorForSelectionHover 东边缘返回左右调整光标', () => {
  const selection = { left: 400, top: 300, right: 700, bottom: 500, width: 300, height: 200 };
  assert.equal(cursorForSelectionHover({ x: 698, y: 400 }, selection, bounds), 'ew-resize');
});

test('cursorForSelectionHover 选区外或无边选区返回 null', () => {
  const selection = { left: 400, top: 300, right: 700, bottom: 500, width: 300, height: 200 };
  assert.equal(cursorForSelectionHover({ x: 100, y: 100 }, selection, bounds), null);
  assert.equal(cursorForSelectionHover({ x: 550, y: 400 }, null, bounds), null);
});

test('cursorForSelectionHover 拒绝无效坐标或边界', () => {
  const selection = { left: 400, top: 300, right: 700, bottom: 500, width: 300, height: 200 };
  assert.throws(() => cursorForSelectionHover({ x: Number.NaN, y: 1 }, selection, bounds), /点 x 必须是有限数字/);
  assert.throws(() => cursorForSelectionHover({ x: 1, y: 1 }, selection, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});
