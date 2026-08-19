import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('数字动作映射单一来源且正反向往返对称', () => {
  const source = readFileSync(new URL('./actionModel.js', import.meta.url), 'utf8');
  // 源码护栏：映射必须来自单一常量（HOTKEY_ACTIONS），禁止硬编码 switch/if 链导致正反向漂移。
  assert.ok(source.includes('const HOTKEY_ACTIONS = {'), '必须存在单一映射常量');
  assert.ok(source.includes("'1': 'copy'"), '1 必须映射 copy');
  assert.ok(source.includes("'2': 'save'"), '2 必须映射 save');
  assert.ok(source.includes("'3': 'pin'"), '3 必须映射 pin');
  assert.ok(source.includes("'4': 'ai'"), '4 必须映射 ai');
  assert.ok(!source.includes('case \'1\''), '禁止 switch 硬编码映射');
  assert.ok(!source.includes('if (key === \'1\')'), '禁止 if 硬编码映射');
  // 行为对称性：已知动作往返不变，未知键/动作返回空。
  for (const key of ['1', '2', '3', '4']) {
    assert.equal(actionForHotkey(hotkeyForAction(actionForHotkey(key))), actionForHotkey(key), `键 ${key} 往返必须不变`);
  }
  for (const action of ['copy', 'save', 'pin', 'ai']) {
    assert.equal(hotkeyForAction(actionForHotkey(hotkeyForAction(action))), hotkeyForAction(action), `动作 ${action} 往返必须不变`);
  }
  assert.equal(hotkeyForAction(actionForHotkey('9')), '', '未知键不得产生动作');
});

test('hotkeyForAction 未知动作返回空字符串', () => {
  assert.equal(hotkeyForAction('unknown'), '');
});
