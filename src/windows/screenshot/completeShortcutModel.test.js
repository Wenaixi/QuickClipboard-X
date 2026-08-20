import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { completeShortcutForEvent } from './completeShortcutModel.js';

test('completeShortcutForEvent 无修饰键 Enter 完成并复制', () => {
  assert.equal(completeShortcutForEvent({ key: 'Enter', ctrlKey: false, metaKey: false, altKey: false }), 'copy');
});

test('completeShortcutForEvent Ctrl+C 完成并复制', () => {
  assert.equal(completeShortcutForEvent({ key: 'c', ctrlKey: true, metaKey: false, altKey: false }), 'copy');
  assert.equal(completeShortcutForEvent({ key: 'C', ctrlKey: true, metaKey: false, altKey: false }), 'copy');
});

test('completeShortcutForEvent Ctrl+S 保存且 Ctrl+P 贴图', () => {
  assert.equal(completeShortcutForEvent({ key: 's', ctrlKey: true, metaKey: false, altKey: false }), 'save');
  assert.equal(completeShortcutForEvent({ key: 'p', ctrlKey: true, metaKey: false, altKey: false }), 'pin');
});

test('completeShortcutForEvent 普通键与未知组合返回空', () => {
  assert.equal(completeShortcutForEvent({ key: 'a', ctrlKey: false, metaKey: false, altKey: false }), null);
  assert.equal(completeShortcutForEvent({ key: 'Enter', ctrlKey: true, metaKey: false, altKey: false }), null);
  assert.equal(completeShortcutForEvent({ key: 'c', ctrlKey: false, metaKey: false, altKey: false }), null);
});

test('completeShortcutForEvent AltGr 组合键拒绝避免误触发复制', () => {
  // AltGr 在 Chromium/WebView2 中同时置位 ctrlKey 与 altKey，必须整体拒绝。
  assert.equal(completeShortcutForEvent({ key: 'c', ctrlKey: true, altKey: true, metaKey: false }), null);
  assert.equal(completeShortcutForEvent({ key: 's', ctrlKey: true, altKey: true, metaKey: false }), null);
  assert.equal(completeShortcutForEvent({ key: 'p', ctrlKey: true, altKey: true, metaKey: false }), null);
  assert.equal(completeShortcutForEvent({ key: 'Enter', ctrlKey: true, altKey: true, metaKey: false }), null);
});

test('completeShortcutForEvent 单一来源且 AltGr 拒绝先于 Ctrl 分支', () => {
  const source = readFileSync(new URL('./completeShortcutModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function completeShortcutForEvent');
  const body = source.slice(start, start + 600);
  // 源码护栏一：必须整体拒绝 alt/meta 修饰键（AltGr 会同时置位 ctrlKey 与 altKey）。
  assert.ok(body.includes('if (event.altKey || event.metaKey) return null;'), '必须整体拒绝 alt/meta 修饰键');
  // 源码护栏二：Ctrl 分支的映射必须在 alt 拒绝之后（顺序不变量，否则 AltGr+C 误触发复制）。
  const altIdx = body.indexOf('if (event.altKey || event.metaKey) return null;');
  const ctrlIdx = body.indexOf('if (event.ctrlKey) {');
  assert.ok(altIdx >= 0 && ctrlIdx >= 0 && altIdx < ctrlIdx, 'AltGr 拒绝必须先于 Ctrl 分支');
  // 源码护栏三：映射必须存在（Enter 复制、Ctrl+C 复制、Ctrl+S 保存、Ctrl+P 贴图）。
  assert.ok(body.includes("if (key === 'c') return 'copy';"), 'Ctrl+C 必须映射复制');
  assert.ok(body.includes("if (key === 's') return 'save';"), 'Ctrl+S 必须映射保存');
  assert.ok(body.includes("if (key === 'p') return 'pin';"), 'Ctrl+P 必须映射贴图');
  assert.ok(body.includes("return key === 'enter' ? 'copy' : null;"), 'Enter 必须映射复制');
  // 行为属性：大小写不敏感、AltGr 四组合全部拒绝。
  assert.equal(completeShortcutForEvent({ key: 'C', ctrlKey: true, metaKey: false, altKey: false }), 'copy');
  assert.equal(completeShortcutForEvent({ key: 'S', ctrlKey: true, metaKey: false, altKey: false }), 'save');
  assert.equal(completeShortcutForEvent({ key: 'P', ctrlKey: true, metaKey: false, altKey: false }), 'pin');
  assert.equal(completeShortcutForEvent({ key: 'ENTER', ctrlKey: false, metaKey: false, altKey: false }), 'copy');
});

test('completeShortcutForEvent 拒绝无效输入', () => {
  assert.throws(() => completeShortcutForEvent(null), /事件对象/);
  assert.throws(() => completeShortcutForEvent('x'), /事件对象/);
  assert.equal(completeShortcutForEvent({}), null);
});
