import { test } from 'node:test';
import assert from 'node:assert/strict';
import { pushSelectionHistory, undoSelectionHistory, canUndoSelection } from './historyModel.js';

test('pushSelectionHistory 追加新状态且不修改原数组', () => {
  const history = [{ left: 0 }, { left: 10 }];
  const next = pushSelectionHistory(history, { left: 20 });
  assert.deepEqual(history, [{ left: 0 }, { left: 10 }]);
  assert.deepEqual(next, [{ left: 0 }, { left: 10 }, { left: 20 }]);
});

test('pushSelectionHistory 超过上限时只保留最近 N 条', () => {
  let history = [];
  for (let i = 0; i < 12; i++) history = pushSelectionHistory(history, { i }, 10);
  assert.equal(history.length, 10);
  assert.deepEqual(history[0], { i: 2 });
  assert.deepEqual(history[9], { i: 11 });
});

test('undoSelectionHistory 弹出最近状态并返回剩余历史', () => {
  const history = [{ left: 0 }, { left: 10 }, { left: 20 }];
  const result = undoSelectionHistory(history);
  assert.deepEqual(result.selection, { left: 20 });
  assert.deepEqual(result.history, [{ left: 0 }, { left: 10 }]);
});

test('undoSelectionHistory 空历史或非法输入返回 null', () => {
  assert.equal(undoSelectionHistory([]), null);
  assert.equal(undoSelectionHistory(null), null);
});

test('canUndoSelection 空历史为 false 否则为 true', () => {
  assert.equal(canUndoSelection([]), false);
  assert.equal(canUndoSelection([{ left: 0 }]), true);
  assert.equal(canUndoSelection(null), false);
});

test('pushSelectionHistory 拒绝非法输入', () => {
  assert.throws(() => pushSelectionHistory(null, { left: 0 }), /历史必须是数组/);
  assert.throws(() => pushSelectionHistory([], { left: 0 }, 0), /上限必须是正整数/);
  assert.throws(() => pushSelectionHistory([], { left: 0 }, 1.5), /上限必须是正整数/);
});
