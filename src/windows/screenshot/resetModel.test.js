import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('canResetSelection 默认最小尺寸为 5 且任一维度小于即允许重置', () => {
  const source = readFileSync(new URL('./resetModel.js', import.meta.url), 'utf8');
  // 源码护栏：默认最小尺寸必须明确为 5（太小误触选区才允许 Esc/右键重置重来），
  // 且宽或高任一小于阈值即允许重置（不是两个都小）。
  assert.ok(source.includes('const minSize = options.minSize ?? 5;'), '默认最小尺寸必须为 5');
  assert.ok(source.includes('return width < minSize || height < minSize;'), '宽或高任一小于阈值即允许重置');
  // 行为边界：默认阈值 5 下，恰好 5x5 不允许重置，4x5 允许，5x4 允许。
  assert.equal(canResetSelection({ width: 5, height: 5 }), false, '恰好 5x5 不允许重置');
  assert.equal(canResetSelection({ width: 4, height: 5 }), true, '宽小于阈值允许重置');
  assert.equal(canResetSelection({ width: 5, height: 4 }), true, '高小于阈值允许重置');
});

test('canResetSelection 浮点宽度高度阈值边界与默认值一致', () => {
  // 选区坐标可为小数，阈值判定必须覆盖浮点边界：4.999 允许重置、5.001 不允许。
  assert.equal(canResetSelection({ width: 4.999, height: 20 }), true, '4.999 宽必须允许重置');
  assert.equal(canResetSelection({ width: 5.001, height: 20 }), false, '5.001 宽不得允许重置');
  assert.equal(canResetSelection({ width: 20, height: 4.999 }), true, '4.999 高必须允许重置');
  assert.equal(canResetSelection({ width: 20, height: 5.001 }), false, '5.001 高不得允许重置');
  // 恰好等于阈值与阈值以上一丁点都不允许（严格小于才允许）。
  assert.equal(canResetSelection({ width: 5, height: 20 }), false, '恰好 5 宽不允许重置');
  assert.equal(canResetSelection({ width: 5.0001, height: 20 }), false, '5.0001 宽不允许重置');
});

test('canResetSelection 拒绝无效输入', () => {
  assert.throws(() => canResetSelection(null), /选区/);
  assert.throws(() => canResetSelection({ width: Number.NaN, height: 1 }), /宽度/);
  assert.throws(() => canResetSelection({ width: 1, height: 1 }, { minSize: 0 }), /最小尺寸/);
});
