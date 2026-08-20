import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { includeInvite } from './inviteModel.js';

test('includeInvite 返回翻译后的邀请文案', () => {
  const t = (key) => key === 'screenshot.invite' ? '截图后按 Enter 复制，F1 查看全部快捷键' : key;
  assert.equal(includeInvite(t), '截图后按 Enter 复制，F1 查看全部快捷键');
});

test('includeInvite 源码翻译 key 必须锁定为 screenshot.invite', () => {
  const source = readFileSync(new URL('./inviteModel.js', import.meta.url), 'utf8');
  // 源码护栏：翻译 key 必须为单一字面量 screenshot.invite（禁止拼接或改名导致提示消失）。
  assert.ok(source.includes("return t('screenshot.invite');"), '翻译 key 必须锁定 screenshot.invite');
  assert.ok(source.includes("throw new TypeError('翻译函数缺失');"), '非法翻译函数必须拒绝');
  // 行为属性：合法翻译函数返回文案，非法输入抛错。
  const t = (key) => key === 'screenshot.invite' ? '截图后按 Enter 复制' : key;
  assert.equal(includeInvite(t), '截图后按 Enter 复制');
  assert.throws(() => includeInvite(null), /翻译函数/);
});

test('includeInvite 拒绝非法翻译函数', () => {
  assert.throws(() => includeInvite(null), /翻译函数/);
  assert.throws(() => includeInvite('x'), /翻译函数/);
});
