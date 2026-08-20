import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('历史模型默认上限 10 且 push 不可变 undo 不修改原数组', () => {
  const source = readFileSync(new URL('./historyModel.js', import.meta.url), 'utf8');
  const pushStart = source.indexOf('export function pushSelectionHistory');
  const pushBody = source.slice(pushStart, pushStart + 400);
  // 源码护栏：默认上限必须为 10（连按方向键最多可撤销 10 步）；追加必须不可变（[...history] 展开）。
  assert.ok(pushBody.includes('limit = 10'), '默认上限必须为 10');
  assert.ok(pushBody.includes('const next = [...history, selection];'), '追加必须不可变展开');
  assert.ok(pushBody.includes('next.slice(next.length - limit)'), '超限必须丢弃最旧条目');
  // 行为属性：默认上限 10 截断、undo 不修改原数组、undo 后可继续 push。
  let history = [];
  for (let i = 0; i < 12; i += 1) history = pushSelectionHistory(history, { i });
  assert.equal(history.length, 10, '默认上限必须截断到 10');
  assert.deepEqual(history[0], { i: 2 }, '最旧两条必须被丢弃');
  const original = [{ left: 0 }, { left: 10 }];
  const undone = undoSelectionHistory(original);
  assert.deepEqual(original, [{ left: 0 }, { left: 10 }], 'undo 不得修改原数组');
  assert.deepEqual(undone.selection, { left: 10 });
  assert.deepEqual(undone.history, [{ left: 0 }]);
  const afterUndo = pushSelectionHistory(undone.history, { left: 20 });
  assert.deepEqual(afterUndo, [{ left: 0 }, { left: 20 }], 'undo 后可继续追加');
  assert.equal(canUndoSelection(undone.history), true, 'undo 后仍有可撤销记录');
});

test('undo 顺序先查空再查类型且非法数组输入抛错与 limit=1 边界', () => {
  const source = readFileSync(new URL('./historyModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function undoSelectionHistory');
  const body = source.slice(start, start + 400);
  // 源码护栏：null/undefined 检查必须位于数组类型检查之前（否则 undo(null) 会抛错而非返回 null，
  // 破坏 App 中"无历史返回 null"的语义）。顺序类不变量用下标比较。
  const nullIdx = body.indexOf('if (history === null || history === undefined)');
  const arrayIdx = body.indexOf('assertHistoryArray(history)');
  assert.ok(nullIdx >= 0 && arrayIdx >= 0 && nullIdx < arrayIdx, '空检查必须早于数组类型检查');
  // 行为：null/undefined 返回 null；字符串/数字等非法输入必须抛错。
  assert.equal(undoSelectionHistory(null), null);
  assert.equal(undoSelectionHistory(undefined), null);
  assert.throws(() => undoSelectionHistory('x'), /历史必须是数组/);
  assert.throws(() => undoSelectionHistory(42), /历史必须是数组/);
  // push limit=1 边界：只保留最新 1 条。
  const one = pushSelectionHistory([{ left: 0 }], { left: 1 }, 1);
  assert.deepEqual(one, [{ left: 1 }]);
  // canUndoSelection 与 undo 返回值一致性：非空数组必然可撤销且弹出非 null。
  const nonEmpty = [{ left: 0 }, { left: 1 }];
  assert.equal(canUndoSelection(nonEmpty), true);
  assert.ok(undoSelectionHistory(nonEmpty) !== null, '非空历史撤销必须返回结果');
  assert.equal(canUndoSelection([]), false);
  assert.equal(undoSelectionHistory([]), null);
});

test('pushSelectionHistory 拒绝非法输入', () => {
  assert.throws(() => pushSelectionHistory(null, { left: 0 }), /历史必须是数组/);
  assert.throws(() => pushSelectionHistory([], { left: 0 }, 0), /上限必须是正整数/);
  assert.throws(() => pushSelectionHistory([], { left: 0 }, 1.5), /上限必须是正整数/);
});
