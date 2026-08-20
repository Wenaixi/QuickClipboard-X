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


test('hotkeyForAction 源码必须用 || \'\' 回退保障返回类型为字符串', () => {
  const source = readFileSync(new URL('./actionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function hotkeyForAction');
  const body = source.slice(start, start + 400);
  // 源码护栏：find 不到时返回 undefined，|| \'\' 确保返回类型始终为字符串而非 undefined。
  assert.ok(body.includes("|| ''"), '必须用 || \'\' 回退 undefined');
  assert.ok(!body.includes('|| null'), '不能用 || null 回退（类型不匹配）');
  // 行为验证：未知动作返回空字符串而非 null 或 undefined。
  assert.equal(hotkeyForAction('unknown'), '', '未知动作必须返回空字符串');
  assert.equal(hotkeyForAction(''), '', '空动作名必须返回空字符串');
  // 未知的默认值不能是 undefined 或 null。
  assert.notEqual(hotkeyForAction('unknown'), undefined, '不能返回 undefined');
  assert.notEqual(hotkeyForAction('unknown'), null, '不能返回 null');
});

test('动作映射全集封闭且每个动作恰好一个热键与防御式容错', () => {
  // 源码护栏：映射块必须恰好 4 个条目且与预期完全一致（加重复动作热键如 5: copy
  // 会让 hotkeyForAction 反向查找产生歧义——find 只返回第一个键）。
  const source = readFileSync(new URL('./actionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('const HOTKEY_ACTIONS = {');
  const blockEnd = source.indexOf('};', start);
  const block = source.slice(start, blockEnd);
  const entries = [...block.matchAll(/^  '([0-9])': '([a-z]+)',?$/gm)].map((m) => [m[1], m[2]]);
  assert.deepEqual(entries, [['1', 'copy'], ['2', 'save'], ['3', 'pin'], ['4', 'ai']], '映射条目必须与预期完全一致');
  // 行为黑盒：扫描全部数字键 0-9，每个已知动作必须恰好命中一个热键（重复动作必现歧义）。
  const countByAction = {};
  for (let i = 0; i <= 9; i += 1) {
    const action = actionForHotkey(String(i));
    if (action) countByAction[action] = (countByAction[action] || 0) + 1;
  }
  for (const action of ['copy', 'save', 'pin', 'ai']) {
    assert.equal(countByAction[action], 1, `动作 ${action} 必须恰好一个数字热键`);
  }
  // 防御式容错：非字符串 key（真实事件对象 key 恒为字符串，但容错不抛错）安全返回 null。
  assert.equal(actionForHotkey(undefined), null);
  assert.equal(actionForHotkey(null), null);
  assert.equal(actionForHotkey(5), null);
  assert.equal(actionForHotkey('10'), null);
});
