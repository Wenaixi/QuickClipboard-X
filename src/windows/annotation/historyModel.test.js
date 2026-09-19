import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  pushSnapshot,
  undoSnapshot,
  redoSnapshot,
  canUndoSnapshots,
  canRedoSnapshots,
  advanceSnapshotGeneration,
  isCurrentSnapshotGeneration,
} from './historyModel.js';

test('pushSnapshot 追加深拷贝快照且超上限丢弃最旧', () => {
  const push = pushSnapshot([], { layers: [{ id: 'a' }] }, 3);
  assert.equal(canUndoSnapshots(push.history), true);
  const second = pushSnapshot(push.history, { layers: [{ id: 'b' }] }, 3, push.redoStack);
  const third = pushSnapshot(second.history, { layers: [{ id: 'c' }] }, 3, second.redoStack);
  const fourth = pushSnapshot(third.history, { layers: [{ id: 'd' }] }, 3, third.redoStack);
  assert.equal(fourth.history.length, 3);
  assert.deepEqual(fourth.history[0], { layers: [{ id: 'b' }] }, '最旧快照必须被丢弃');
});

test('pushSnapshot 必须清空重做栈（新分支不可重做）', () => {
  const pushed = pushSnapshot([], { layers: [] });
  const undone = undoSnapshot(pushed.history, pushed.redoStack);
  assert.equal(canRedoSnapshots(undone.redoStack), true);
  const branched = pushSnapshot(undone.history, { layers: [{ id: 'new' }] }, 50, undone.redoStack);
  assert.deepEqual(branched.redoStack, [], '编辑后必须清空重做栈');
});

test('undoSnapshot 弹出快照入重做栈且深拷贝防污染', () => {
  const pushed = pushSnapshot([], { layers: [{ points: [{ x: 1, y: 2 }] }] });
  const undone = undoSnapshot(pushed.history, pushed.redoStack);
  assert.deepEqual(undone.snapshot, { layers: [{ points: [{ x: 1, y: 2 }] }] });
  undone.snapshot.layers[0].points[0].x = 999;
  assert.equal(pushed.history[0].layers[0].points[0].x, 1, '撤销后修改返回快照不得污染栈内状态');
  assert.equal(undone.redoStack.length, 1);
  // 无历史返回 null。
  assert.equal(undoSnapshot([], []), null);
});

test('redoSnapshot 从重做栈恢复并返回快照', () => {
  const pushed = pushSnapshot([], { layers: [{ id: 'a' }] });
  const undone = undoSnapshot(pushed.history, pushed.redoStack);
  assert.equal(canRedoSnapshots(undone.redoStack), true);
  const redone = redoSnapshot(undone.history, undone.redoStack);
  assert.deepEqual(redone.snapshot, { layers: [{ id: 'a' }] });
  assert.equal(redone.history.length, 1);
  assert.equal(canRedoSnapshots(redone.redoStack), false);
  assert.equal(redoSnapshot([], []), null);
});

test('代际令牌推进使旧异步回调失效', () => {
  const ref = { current: 0 };
  const gen = advanceSnapshotGeneration(ref);
  const stale = isCurrentSnapshotGeneration(0, ref);
  assert.equal(stale, false, '旧代际必须失效');
  assert.equal(isCurrentSnapshotGeneration(gen, ref), true);
});

test('historyModel 源码护栏：双栈结构/深拷贝/代际令牌存在', () => {
  const source = readFileSync(new URL('./historyModel.js', import.meta.url), 'utf8');
  assert.ok(source.includes('undoSnapshot'), '必须提供撤销');
  assert.ok(source.includes('redoSnapshot'), '必须提供重做');
  assert.ok(source.includes('deepCloneSnapshot'), '快照必须深拷贝');
  assert.ok(source.includes('advanceSnapshotGeneration'), '必须提供代际令牌');
  assert.ok(source.includes('redoStack: []'), 'push 必须清空重做栈');
});