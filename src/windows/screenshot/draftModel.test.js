import { test } from 'node:test';
import assert from 'node:assert/strict';
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

test('resetInteractionState 拒绝无效输入', () => {
  assert.throws(() => resetInteractionState(null), /状态容器/);
});
