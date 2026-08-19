import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fullScreenSelection } from './fullScreenModel.js';

test('fullScreenSelection 生成覆盖整个边界的选区', () => {
  assert.deepEqual(fullScreenSelection({ width: 1920, height: 1080 }), { left: 0, top: 0, right: 1920, bottom: 1080, width: 1920, height: 1080 });
});

test('fullScreenSelection 支持小数边界并向上取整完整覆盖', () => {
  // 右/下边界向上取整，保证选区完全覆盖屏幕而不留空白边。
  const selection = fullScreenSelection({ width: 100.6, height: 50.4 });
  assert.equal(selection.right, 101);
  assert.equal(selection.bottom, 51);
  assert.equal(selection.width, 101);
  assert.equal(selection.height, 51);
});

test('fullScreenSelection 拒绝无效边界', () => {
  assert.throws(() => fullScreenSelection(null), /边界/);
  assert.throws(() => fullScreenSelection({ width: 0, height: 1 }), /正数/);
  assert.throws(() => fullScreenSelection({ width: 1, height: Number.NaN }), /有限数字/);
});
