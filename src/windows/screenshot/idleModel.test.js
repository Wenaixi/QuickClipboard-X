import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { idleHint } from './idleModel.js';

test('idleHint 返回翻译后的初始引导文案', () => {
  const t = (key) => key === 'screenshot.idleHint' ? '拖动以框选截图区域，按 F1 查看快捷键' : key;
  assert.equal(idleHint(t), '拖动以框选截图区域，按 F1 查看快捷键');
});

test('idleHint 源码翻译 key 必须锁定为 screenshot.idleHint', () => {
  const source = readFileSync(new URL('./idleModel.js', import.meta.url), 'utf8');
  // 源码护栏：翻译 key 必须为单一字面量 screenshot.idleHint（禁止拼接或改名导致提示消失）。
  assert.ok(source.includes("return t('screenshot.idleHint');"), '翻译 key 必须锁定 screenshot.idleHint');
  assert.ok(source.includes("throw new TypeError('翻译函数缺失');"), '非法翻译函数必须拒绝');
  // 行为属性：合法翻译函数返回文案，非法输入抛错。
  const t = (key) => key === 'screenshot.idleHint' ? '拖动以框选截图区域' : key;
  assert.equal(idleHint(t), '拖动以框选截图区域');
  assert.throws(() => idleHint(null), /翻译函数/);
});

test('idleHint 拒绝非法翻译函数', () => {
  assert.throws(() => idleHint(null), /翻译函数/);
  assert.throws(() => idleHint('x'), /翻译函数/);
});
