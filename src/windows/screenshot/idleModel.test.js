import { test } from 'node:test';
import assert from 'node:assert/strict';
import { idleHint } from './idleModel.js';

test('idleHint 返回翻译后的初始引导文案', () => {
  const t = (key) => key === 'screenshot.idleHint' ? '拖动以框选截图区域，按 F1 查看快捷键' : key;
  assert.equal(idleHint(t), '拖动以框选截图区域，按 F1 查看快捷键');
});

test('idleHint 拒绝非法翻译函数', () => {
  assert.throws(() => idleHint(null), /翻译函数/);
  assert.throws(() => idleHint('x'), /翻译函数/);
});
