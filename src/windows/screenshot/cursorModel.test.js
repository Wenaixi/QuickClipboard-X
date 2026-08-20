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

test('cursorForSelectionHover 内部判定必须显式传 0 内缩覆盖完整选区矩形', () => {
  const source = readFileSync(new URL('./cursorModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function cursorForSelectionHover');
  const body = source.slice(start, start + 600);
  // 源码护栏：内部判定必须显式传 0 内缩（完整选区矩形），禁止省略参数用默认 inset 4——
  // 默认内缩会让窄选区（宽度 <= 8px）的内部矩形空转，中心点丢失移动光标。
  assert.ok(body.includes("if (hitSelectionInterior(point, selection, 0)) return 'move';"), '内部判定必须显式传 0 内缩');
  assert.ok(!body.includes("hitSelectionInterior(point, selection)) return 'move'"), '禁止省略内缩参数用默认值');
  // 行为佐证：远离边缘的中心点返回移动光标；窄选区中心点被边缘判定捕获返回调整光标
  // （边缘优先，不落到内部判定）；选区外紧邻边缘仍返回 null（容差外不误判）。
  const wide = { left: 100, top: 100, right: 300, bottom: 300 };
  assert.equal(cursorForSelectionHover({ x: 200, y: 200 }, wide, bounds), 'move', '宽选区中心必须移动光标');
  const narrow = { left: 100, top: 100, right: 106, bottom: 300 };
  const narrowHover = cursorForSelectionHover({ x: 103, y: 200 }, narrow, bounds);
  assert.ok(narrowHover !== 'move', '窄选区中心点必须在边缘带内（不落内部判定）');
  assert.equal(cursorForSelectionHover({ x: 91, y: 200 }, wide, bounds), null, '选区外容差外必须返回 null');
});


test('cursorForSelectionHover 源码 null selection 守卫必须在 hitSelectionEdge 之前', () => {
  const source = readFileSync(new URL('./cursorModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function cursorForSelectionHover');
  const body = source.slice(start, start + 600);
  // 源码护栏：立即返回 null 的守卫（if (!selection) return null;）必须在
  // 调用 hitSelectionEdge 之前，否则 selection 为 null 时会因为
  // hitSelectionEdge 内访问 selection.left 抛出 TypeError，不能安全返回 null。
  const guardIdx = body.indexOf('if (!selection) return null;');
  const edgeIdx = body.indexOf('const edge = hitSelectionEdge(point, selection);');
  assert.ok(guardIdx >= 0 && edgeIdx >= 0, 'null 守卫和 hitSelectionEdge 都必须存在');
  assert.ok(guardIdx < edgeIdx, 'null selection 守卫必须在 hitSelectionEdge 之前');
  // 行为验证：selection 为 null 时返回 null而不抛错。
  assert.equal(cursorForSelectionHover({ x: 550, y: 400 }, null, bounds), null, 'null selection 必须返回 null');
  assert.equal(cursorForSelectionHover({ x: 550, y: 400 }, undefined, bounds), null, 'undefined selection 必须返回 null');
});
test('cursorForSelectionHover 拒绝无效坐标或边界', () => {
  const selection = { left: 400, top: 300, right: 700, bottom: 500, width: 300, height: 200 };
  assert.throws(() => cursorForSelectionHover({ x: Number.NaN, y: 1 }, selection, bounds), /点 x 必须是有限数字/);
  assert.throws(() => cursorForSelectionHover({ x: 1, y: 1 }, selection, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});
