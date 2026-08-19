import { test } from 'node:test';
import assert from 'node:assert/strict';
import { actionForHotkey, hotkeyForAction } from './actionModel.js';

test('actionForHotkey 数字键映射到对应动作', () => {
  assert.equal(actionForHotkey('1'), 'copy');
  assert.equal(actionForHotkey('2'), 'save');
  assert.equal(actionForHotkey('3'), 'pin');
  assert.equal(actionForHotkey('4'), 'ai');
});

test('actionForHotkey 未知键返回 null', () => {
  assert.equal(actionForHotkey('5'), null);
  assert.equal(actionForHotkey('a'), null);
  assert.equal(actionForHotkey(''), null);
});

test('hotkeyForAction 反向映射到数字键', () => {
  assert.equal(hotkeyForAction('copy'), '1');
  assert.equal(hotkeyForAction('save'), '2');
  assert.equal(hotkeyForAction('pin'), '3');
  assert.equal(hotkeyForAction('ai'), '4');
});

test('hotkeyForAction 未知动作返回空字符串', () => {
  assert.equal(hotkeyForAction('unknown'), '');
});
