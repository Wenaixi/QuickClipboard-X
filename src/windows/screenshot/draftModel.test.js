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

test('resetInteractionState 拒绝无效输入', () => {
  assert.throws(() => resetInteractionState(null), /状态容器/);
});
