import { test } from 'node:test';
import assert from 'node:assert/strict';
import { canResetSelection } from './resetModel.js';

test('canResetSelection 选区小于最小尺寸时允许重置', () => {
  assert.equal(canResetSelection({ width: 1, height: 1 }), true);
  assert.equal(canResetSelection({ width: 4, height: 10 }), true);
  assert.equal(canResetSelection({ width: 10, height: 2 }), true);
});

test('canResetSelection 达到最小尺寸后不允许重置', () => {
  assert.equal(canResetSelection({ width: 6, height: 6 }), false);
  assert.equal(canResetSelection({ width: 100, height: 50 }), false);
});

test('canResetSelection 支持自定义最小尺寸', () => {
  assert.equal(canResetSelection({ width: 12, height: 12 }, { minSize: 12 }), false);
  assert.equal(canResetSelection({ width: 11, height: 11 }, { minSize: 12 }), true);
});

test('canResetSelection 拒绝无效输入', () => {
  assert.throws(() => canResetSelection(null), /选区/);
  assert.throws(() => canResetSelection({ width: Number.NaN, height: 1 }), /宽度/);
  assert.throws(() => canResetSelection({ width: 1, height: 1 }, { minSize: 0 }), /最小尺寸/);
});
