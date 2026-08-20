import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { idleHint } from './idleModel.js';
import { includeInvite } from './inviteModel.js';

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

test('idleHint 与 inviteModel 翻译 key 必须语义互斥不可互换', () => {
  const idleSource = readFileSync(new URL('./idleModel.js', import.meta.url), 'utf8');
  const inviteSource = readFileSync(new URL('./inviteModel.js', import.meta.url), 'utf8');
  // 源码护栏一：idleHint 必须锁定截图空闲引导 key（screenshot.idleHint）。
  assert.ok(idleSource.includes("return t('screenshot.idleHint');"), 'idleHint 必须锁定截图空闲引导 key');
  // 源码护栏二：includeInvite 必须锁定完成动作邀请 key（screenshot.invite）。
  // 两个 key 不可互换——互换会让初始引导显示完成提示、完成提示显示初始引导（语义完全颠倒），
  // 且两个模块各自的单模块护栏都发现不了这种互换（各自只锁函数内单字面量）。
  assert.ok(inviteSource.includes("return t('screenshot.invite');"), 'invite 必须锁定完成动作邀请 key');
  // 行为属性：各自翻译函数语义正确。
  const idleT = (key) => (key === 'screenshot.idleHint' ? '拖动框选' : key);
  const inviteT = (key) => (key === 'screenshot.invite' ? '按 Enter 复制' : key);
  assert.equal(idleHint(idleT), '拖动框选');
  assert.equal(includeInvite(inviteT), '按 Enter 复制');
  // 语义交叉验证：各自传对方 key 的翻译函数时返回原 key（证明两者确实是不同语义）。
  assert.equal(idleHint(inviteT), 'screenshot.idleHint', 'idleHint 必须调用自己的 idleHint key');
  assert.equal(includeInvite(idleT), 'screenshot.invite', 'invite 必须调用自己的 invite key');
});

test('idleHint 拒绝非法翻译函数', () => {
  assert.throws(() => idleHint(null), /翻译函数/);
  assert.throws(() => idleHint('x'), /翻译函数/);
});
