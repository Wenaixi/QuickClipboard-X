import { test } from 'node:test';
import assert from 'node:assert/strict';
import { includeInvite } from './inviteModel.js';

test('includeInvite 返回翻译后的邀请文案', () => {
  const t = (key) => key === 'screenshot.invite' ? '截图后按 Enter 复制，F1 查看全部快捷键' : key;
  assert.equal(includeInvite(t), '截图后按 Enter 复制，F1 查看全部快捷键');
});

test('includeInvite 拒绝非法翻译函数', () => {
  assert.throws(() => includeInvite(null), /翻译函数/);
  assert.throws(() => includeInvite('x'), /翻译函数/);
});
