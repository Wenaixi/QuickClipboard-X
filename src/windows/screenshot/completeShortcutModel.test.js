import { test } from 'node:test';
import assert from 'node:assert/strict';
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

test('completeShortcutForEvent 拒绝无效输入', () => {
  assert.throws(() => completeShortcutForEvent(null), /事件对象/);
  assert.throws(() => completeShortcutForEvent('x'), /事件对象/);
  assert.equal(completeShortcutForEvent({}), null);
});
