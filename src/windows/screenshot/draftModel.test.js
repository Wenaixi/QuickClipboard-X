import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { selectionFromDraft, resetInteractionState } from './draftModel.js';

test('selectionFromDraft 用方形路径生成选区', () => {
  // 正方形边长取两轴位移较大者（y 位移 150 > x 位移 100），所以右下角为 (250, 250)。
  const selection = selectionFromDraft({ start: { x: 100, y: 100 }, end: { x: 200, y: 250 } }, { shiftKey: true }, { width: 1920, height: 1080 });
  assert.equal(selection.left, 100);
  assert.equal(selection.top, 100);
  assert.equal(selection.right, 250);
  assert.equal(selection.bottom, 250);
  assert.equal(selection.width, 150);
  assert.equal(selection.height, 150);
});

test('selectionFromDraft 普通路径生成自由选区', () => {
  const draft = { start: { x: 100, y: 100 }, end: { x: 200, y: 250 } };
  const selection = selectionFromDraft(draft, { shiftKey: false }, { width: 1920, height: 1080 });
  assert.equal(selection.left, 100);
  assert.equal(selection.top, 100);
  assert.equal(selection.right, 200);
  assert.equal(selection.bottom, 250);
  assert.equal(selection.width, 100);
  assert.equal(selection.height, 150);
});

test('selectionFromDraft 拒绝无效输入', () => {
  assert.throws(() => selectionFromDraft(null, {}, { width: 1, height: 1 }), /草稿/);
  assert.throws(() => selectionFromDraft({ start: { x: 0, y: 0 }, end: { x: 1, y: 1 } }, {}, { width: 0, height: 1 }), /边界/);
});

test('resetInteractionState 重置选区与全部交互状态', () => {
  const state = {
    draftRef: { current: { start: { x: 1, y: 1 }, end: { x: 2, y: 2 } } },
    selectionRef: { current: { left: 1, top: 1, right: 2, bottom: 2 } },
    moveRef: { current: {} },
    resizeRef: { current: {} },
    setSelection: () => {},
    setSelecting: () => {},
    setMoving: () => {},
    setResizing: () => {},
  };
  resetInteractionState(state);
  assert.equal(state.draftRef.current, null);
  assert.equal(state.selectionRef.current, null);
  assert.equal(state.moveRef.current, null);
  assert.equal(state.resizeRef.current, null);
});

test('草稿转换复用选区模型且重置完整清空全部交互状态', () => {
  const source = readFileSync(new URL('./draftModel.js', import.meta.url), 'utf8');
  // 源码护栏：Shift 路径必须走 squareSelection、普通路径必须走 normalizeSelection（复用 selectionModel，禁止重复实现）。
  assert.ok(source.includes('return event.shiftKey'), '必须按 Shift 分支切换路径');
  assert.ok(source.includes('? squareSelection(start, end, bounds)'), 'Shift 必须走方形路径');
  assert.ok(source.includes(': normalizeSelection(start, end, bounds)'), '普通必须走规范化路径');
  // 重置完整性：四个引用全部置 null，四个 setter 全部复位。
  const resetStart = source.indexOf('export function resetInteractionState');
  const resetBody = source.slice(resetStart, resetStart + 400);
  assert.ok(resetBody.includes('state.draftRef.current = null;'), '草稿引用必须清空');
  assert.ok(resetBody.includes('state.selectionRef.current = null;'), '选区引用必须清空');
  assert.ok(resetBody.includes('state.moveRef.current = null;'), '移动引用必须清空');
  assert.ok(resetBody.includes('state.resizeRef.current = null;'), '调整引用必须清空');
  assert.ok(resetBody.includes('state.setSelection?.(null)'), '选区状态必须复位');
  assert.ok(resetBody.includes('state.setSelecting?.(false)'), '框选状态必须复位');
  assert.ok(resetBody.includes('state.setMoving?.(false)'), '移动状态必须复位');
  assert.ok(resetBody.includes('state.setResizing?.(false)'), '调整状态必须复位');
  // 行为属性：Shift 走正方形、普通走自由、重置后四引用全空且 setter 收到复位值。
  const calls = [];
  const state = {
    draftRef: { current: {} },
    selectionRef: { current: {} },
    moveRef: { current: {} },
    resizeRef: { current: {} },
    setSelection: (v) => calls.push(['selection', v]),
    setSelecting: (v) => calls.push(['selecting', v]),
    setMoving: (v) => calls.push(['moving', v]),
    setResizing: (v) => calls.push(['resizing', v]),
  };
  resetInteractionState(state);
  assert.equal(state.draftRef.current, null);
  assert.equal(state.selectionRef.current, null);
  assert.equal(state.moveRef.current, null);
  assert.equal(state.resizeRef.current, null);
  assert.deepEqual(calls, [['selection', null], ['selecting', false], ['moving', false], ['resizing', false]]);
});

test('selectionFromDraft 非法草稿坐标先于边界校验抛错且引用未定义也拒绝', () => {
  // 坐标校验必须先行：start/end 缺失或含非有限坐标时抛 TypeError（不是 RangeError），而不是静默走默认路径。
  assert.throws(() => selectionFromDraft({}, {}, { width: 1920, height: 1080 }), TypeError);
  assert.throws(() => selectionFromDraft({ start: { x: 0, y: 0 }, end: { x: Number.NaN, y: 1 } }, {}, { width: 1920, height: 1080 }), TypeError);
  // 引用未定义（start 缺失）同样拒绝。
  assert.throws(() => selectionFromDraft({ start: null, end: { x: 1, y: 1 } }, {}, { width: 1920, height: 1080 }), TypeError);
  // 边界非法抛 RangeError（与坐标校验的错误类型必须不同，防止两类校验混淆）。
  assert.throws(() => selectionFromDraft({ start: { x: 0, y: 0 }, end: { x: 1, y: 1 } }, {}, { width: 0, height: 1 }), RangeError);
  // 合法输入不抛错：Shift 方形与普通路径都能正常生成。
  const draft = { start: { x: 100, y: 100 }, end: { x: 200, y: 250 } };
  assert.equal(selectionFromDraft(draft, { shiftKey: true }, { width: 1920, height: 1080 }).width, 150);
  assert.equal(selectionFromDraft(draft, { shiftKey: false }, { width: 1920, height: 1080 }).width, 100);
});

test('selectionFromDraft 缺省 shiftKey 字段按 falsy 走普通路径且重置幂等', () => {
  const source = readFileSync(new URL('./draftModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function selectionFromDraft');
  const body = source.slice(start, start + 460);
  // 源码护栏一：Shift 判定必须直接用 event.shiftKey 三元（falsy 语义）——
  // 事件对象缺 shiftKey 字段（undefined）必须走普通路径，禁止 `=== true` 严格比较。
  assert.ok(body.includes('return event.shiftKey'), '必须直接使用 event.shiftKey 三元');
  assert.ok(!body.includes('event.shiftKey === true'), '禁止严格等于 true 比较（undefined 应走普通路径）');
  // 源码护栏二：resetInteractionState 必须同时清空四个引用并复位四个 setter。
  const resetStart = source.indexOf('export function resetInteractionState');
  const resetBody = source.slice(resetStart, resetStart + 400);
  assert.ok(resetBody.includes('state.draftRef.current = null;'), '草稿引用必须清空');
  assert.ok(resetBody.includes('state.resizeRef.current = null;'), '调整引用必须清空');
  assert.ok(resetBody.includes('state.setResizing?.(false)'), '调整 setter 必须复位');
  // 行为一：事件缺 shiftKey 字段（合成事件/自动化调用常见）走普通路径返回自由选区。
  const draft = { start: { x: 100, y: 100 }, end: { x: 200, y: 250 } };
  assert.equal(selectionFromDraft(draft, {}, { width: 1920, height: 1080 }).width, 100, '缺省 shiftKey 必须走普通路径');
  assert.equal(selectionFromDraft(draft, { shiftKey: undefined }, { width: 1920, height: 1080 }).width, 100, 'shiftKey undefined 必须走普通路径');
  assert.equal(selectionFromDraft(draft, { shiftKey: true }, { width: 1920, height: 1080 }).width, 150, 'shiftKey true 必须走方形路径');
  // 行为二：resetInteractionState 幂等——连续调用两次不抛错且引用保持 null、setter 收到两次复位。
  const calls = [];
  const state = {
    draftRef: { current: {} },
    selectionRef: { current: {} },
    moveRef: { current: {} },
    resizeRef: { current: {} },
    setSelection: (v) => calls.push(['selection', v]),
    setSelecting: (v) => calls.push(['selecting', v]),
    setMoving: (v) => calls.push(['moving', v]),
    setResizing: (v) => calls.push(['resizing', v]),
  };
  resetInteractionState(state);
  resetInteractionState(state);
  assert.equal(state.draftRef.current, null);
  assert.equal(state.selectionRef.current, null);
  assert.equal(state.moveRef.current, null);
  assert.equal(state.resizeRef.current, null);
  assert.deepEqual(calls, [
    ['selection', null], ['selecting', false], ['moving', false], ['resizing', false],
    ['selection', null], ['selecting', false], ['moving', false], ['resizing', false],
  ], '幂等重置必须重复复位');
});

test('resetInteractionState 拒绝无效输入', () => {
  assert.throws(() => resetInteractionState(null), /状态容器/);
});
