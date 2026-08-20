import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { selectionPhysicalPosition, formatSelectionPosition } from './positionModel.js';
import { formatCursorCoordinate } from './coordinateModel.js';

test('selectionPhysicalPosition 默认按 1:1 与原点换算', () => {
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }), { x: 100, y: 50 });
  assert.deepEqual(selectionPhysicalPosition({ left: 0, top: 0 }), { x: 0, y: 0 });
});

test('selectionPhysicalPosition 按 DPR 换算并向下取整', () => {
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }, { dpr: 1.5 }), { x: 150, y: 75 });
  assert.deepEqual(selectionPhysicalPosition({ left: 100.5, top: 50 }, { dpr: 1.5 }), { x: 150, y: 75 });
});

test('selectionPhysicalPosition 支持负坐标副屏的显示器偏移', () => {
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }, { monitorLeft: -1920 }), { x: -1820, y: 50 });
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }, { monitorLeft: -1920, dpr: 1.5 }), { x: -2730, y: 75 });
});

test('formatSelectionPosition 输出固定格式位置文案', () => {
  assert.equal(formatSelectionPosition({ left: 100, top: 50 }, { dpr: 1.5 }), 'X: 150  Y: 75');
  assert.equal(formatSelectionPosition({ left: 0, top: 0 }), 'X: 0  Y: 0');
});

test('selectionPhysicalPosition 源码默认值与 floor 换算且与 selectionToPhysical 起点语义一致', () => {
  const source = readFileSync(new URL('./positionModel.js', import.meta.url), 'utf8');
  const selectionSource = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function selectionPhysicalPosition');
  const body = source.slice(start, start + 700);
  // 源码护栏一：默认 dpr=1、monitorLeft=0、monitorTop=0（单显示器 1:1 语义）。
  assert.ok(body.includes('const dpr = options.dpr ?? 1;'), '默认 dpr 必须为 1');
  assert.ok(body.includes('const monitorLeft = options.monitorLeft ?? 0;'), '默认显示器左偏移必须为 0');
  assert.ok(body.includes('const monitorTop = options.monitorTop ?? 0;'), '默认显示器上偏移必须为 0');
  // 源码护栏二：物理位置必须由（显示器偏移 + 逻辑坐标）乘 dpr 后向下取整（覆盖 [x, x+1) 像素）。
  assert.ok(body.includes('x: Math.floor((monitorLeft + selection.left) * dpr),'), 'x 必须向下取整换算');
  assert.ok(body.includes('y: Math.floor((monitorTop + selection.top) * dpr),'), 'y 必须向下取整换算');
  // 源码护栏三：与 selectionToPhysical 的起点处理一致（都是 floor，无四舍五入）。
  const selStart = selectionSource.indexOf('export function selectionToPhysical');
  const selBody = selectionSource.slice(selStart, selStart + 600);
  assert.ok(selBody.includes('Math.floor'), 'selectionToPhysical 起点必须用 floor');
  // 行为：多显示器负坐标 + 非整数逻辑坐标 + dpr 的浮点组合必须稳定向下取整。
  assert.deepEqual(
    selectionPhysicalPosition({ left: 100.7, top: 50.3 }, { monitorLeft: -1920, monitorTop: 240, dpr: 1.25 }),
    { x: Math.floor((-1920 + 100.7) * 1.25), y: Math.floor((240 + 50.3) * 1.25) },
    '负偏移与非整数坐标组合必须一致向下取整'
  );
  // 默认参数行为：不传 options 时 1:1 原点映射。
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }), { x: 100, y: 50 });
  assert.deepEqual(selectionPhysicalPosition({ left: -50, top: 0 }), { x: -50, y: 0 });
});

test('formatSelectionPosition 与 formatCursorCoordinate 共享完全一致的位置文案格式', () => {
  const source = readFileSync(new URL('./positionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function formatSelectionPosition');
  const body = source.slice(start, start + 250);
  // 源码护栏一：选区位置文案必须用双空格分隔的大写 X/Y（X: <x>  Y: <y>），
  // 与截图界面坐标提示（formatCursorCoordinate）完全一致，防止两处提示格式漂移。
  assert.ok(body.includes('return `X: ${x}  Y: ${y}`;'), '选区位置文案必须用双空格分隔的大写 X/Y');
  // 源码护栏二：coordinateModel 的坐标提示也必须保持相同格式（双空格分隔）。
  const coordSource = readFileSync(new URL('./coordinateModel.js', import.meta.url), 'utf8');
  const coordStart = coordSource.indexOf('export function formatCursorCoordinate');
  const coordBody = coordSource.slice(coordStart, coordStart + 250);
  assert.ok(coordBody.includes('return `X: ${Math.round(point.x)}  Y: ${Math.round(point.y)}`;'), '坐标提示文案必须同样双空格分隔');
  // 行为验证：两个模型对同一逻辑坐标输出相同格式模式（/^X: -?\d+  Y: -?\d+$/，双空格）。
  const selectionText = formatSelectionPosition({ left: 123.4, top: 456.7 }, { dpr: 1 });
  const cursorText = formatCursorCoordinate({ x: 123.4, y: 456.7 });
  assert.match(selectionText, /^X: -?\d+  Y: -?\d+$/, '选区位置文案必须匹配统一格式模式');
  assert.match(cursorText, /^X: -?\d+  Y: -?\d+$/, '坐标提示文案必须匹配统一格式模式');
  // 语义区分：位置走 floor 换算（123.4 取 123），坐标提示走 round（456.7 取 457），
  // 但格式模板必须一致（都含双空格分隔）。
  assert.equal(selectionText, 'X: 123  Y: 456', '选区位置按 floor 取物理像素');
  assert.equal(cursorText, 'X: 123  Y: 457', '坐标提示按 round 取逻辑像素');
});

test('selectionPhysicalPosition 拒绝非法输入', () => {
  assert.throws(() => selectionPhysicalPosition(null), /选区/);
  assert.throws(() => selectionPhysicalPosition({ left: Number.NaN, top: 1 }), /left 必须是有限数字/);
  assert.throws(() => selectionPhysicalPosition({ left: 1, top: 1 }, { dpr: 0 }), /dpr 必须是正数/);
  assert.throws(() => selectionPhysicalPosition({ left: 1, top: 1 }, { monitorTop: Number.NaN }), /monitorTop 必须是有限数字/);
});
