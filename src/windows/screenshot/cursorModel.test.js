import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('cursorForEdge 八方向映射完整且悬停优先边缘后内部', () => {
  const source = readFileSync(new URL('./cursorModel.js', import.meta.url), 'utf8');
  // 源码护栏：8 个边缘方向的光标映射必须全部存在，缺任何一方向选区角点悬停就退化为十字。
  for (const [edge, cursor] of [['n', 'ns-resize'], ['s', 'ns-resize'], ['e', 'ew-resize'], ['w', 'ew-resize'], ['ne', 'nesw-resize'], ['sw', 'nesw-resize'], ['nw', 'nwse-resize'], ['se', 'nwse-resize']]) {
    assert.ok(source.includes(`${edge}: '${cursor}'`), `EDGE_CURSORS 必须包含 ${edge} 映射`);
    assert.equal(cursorForEdge(edge), cursor, `cursorForEdge('${edge}') 必须返回 ${cursor}`);
  }
  // 行为属性：悬停时边缘/角点优先于内部（边缘点绝不返回 move），选区外返回 null。
  const selection = { left: 500, top: 300, right: 700, bottom: 500 };
  assert.equal(cursorForSelectionHover({ x: 699, y: 400 }, selection, bounds), 'ew-resize', '东边缘必须优先');
  assert.equal(cursorForSelectionHover({ x: 699, y: 300 }, selection, bounds), 'nesw-resize', '东北角必须优先');
  assert.equal(cursorForSelectionHover({ x: 600, y: 499 }, selection, bounds), 'ns-resize', '南边缘必须优先');
  assert.equal(cursorForSelectionHover({ x: 600, y: 400 }, selection, bounds), 'move', '内部返回移动光标');
  assert.equal(cursorForSelectionHover({ x: 100, y: 100 }, selection, bounds), null, '选区外返回 null');
});

test('cursorForSelectionHover 源码边缘判定必须先于内部判定', () => {
  const source = readFileSync(new URL('./cursorModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function cursorForSelectionHover');
  const body = source.slice(start, start + 600);
  // 顺序类不变量：边缘/角点判定必须先于内部判定（否则边缘点会被内部点逻辑截获返回 move）。
  // 只 contains 区分不了先后，必须比较 find 下标。
  const edgeIdx = body.indexOf('const edge = hitSelectionEdge(point, selection);');
  const edgeReturnIdx = body.indexOf('if (edge) return cursorForEdge(edge);');
  const interiorIdx = body.indexOf('if (hitSelectionInterior(point, selection, 0)) return \'move\';');
  assert.ok(edgeIdx >= 0 && edgeReturnIdx >= 0 && interiorIdx >= 0, '边缘/内部判定必须都存在');
  assert.ok(edgeIdx < interiorIdx, '边缘判定必须先于内部判定');
  assert.ok(edgeReturnIdx < interiorIdx, '边缘返回值必须先于内部返回值');
  // 行为属性：悬停边缘点时边缘判定先执行，绝不返回 move（即使点同时满足内部条件）。
  const selection = { left: 500, top: 300, right: 700, bottom: 500, width: 200, height: 200 };
  // x=699 同时满足内部条件（500 <= 699 < 700）与右边缘条件（>= 700 - 4），必须返回 ew-resize。
  assert.equal(cursorForSelectionHover({ x: 699, y: 400 }, selection, bounds), 'ew-resize', '同时满足内部与边缘时边缘必须优先');
});

test('cursorForSelectionHover 拒绝无效坐标或边界', () => {
  const selection = { left: 400, top: 300, right: 700, bottom: 500, width: 300, height: 200 };
  assert.throws(() => cursorForSelectionHover({ x: Number.NaN, y: 1 }, selection, bounds), /点 x 必须是有限数字/);
  assert.throws(() => cursorForSelectionHover({ x: 1, y: 1 }, selection, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});
