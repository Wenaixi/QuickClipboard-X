import { test } from 'node:test';
import assert from 'node:assert/strict';
import { cycleValue } from './emojiKbNavigation.js';

test('cycleValue 右移循环', () => {
  assert.equal(cycleValue(['a', 'b', 'c'], 'a', 1), 'b');
  assert.equal(cycleValue(['a', 'b', 'c'], 'c', 1), 'a');
});
test('cycleValue 左移循环', () => {
  assert.equal(cycleValue(['a', 'b', 'c'], 'b', -1), 'a');
  assert.equal(cycleValue(['a', 'b', 'c'], 'a', -1), 'c');
});
test('cycleValue 未知当前值从头开始', () => {
  assert.equal(cycleValue(['a', 'b', 'c'], 'zzz', 1), 'a');
  assert.equal(cycleValue(['a', 'b', 'c'], 'zzz', -1), 'c');
});
test('cycleValue 空数组返回 undefined(不读 arr[-1]/arr[0] 越界)', () => {
  // 回归:旧实现 indexOf=-1 分支 arr[delta>0?0:-1] 读 undefined;空数组时
  // arr[-1] 也是 undefined,但语义未明确定义,这里锁死"空数组 → undefined"契约
  assert.equal(cycleValue([], 'a', 1), undefined);
  assert.equal(cycleValue([], 'a', -1), undefined);
  assert.equal(cycleValue([], 'zzz', 1), undefined);
});
