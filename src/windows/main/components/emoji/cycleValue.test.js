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
