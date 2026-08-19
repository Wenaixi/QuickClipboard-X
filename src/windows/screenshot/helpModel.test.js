import { test } from 'node:test';
import assert from 'node:assert/strict';
import { helpEntries, isHelpShortcut } from './helpModel.js';

const t = (key) => ({
  'screenshot.help.complete': '完成截图并复制',
  'screenshot.help.fullscreen': '全屏选区',
  'screenshot.help.save': '保存为图片',
  'screenshot.help.pin': '贴图',
  'screenshot.help.cancel': '取消截图',
  'screenshot.help.nudge': '方向键微调选区',
  'screenshot.help.square': 'Shift 保持正方形',
  'screenshot.help.center': 'Ctrl 从中心缩放',
  'screenshot.help.undo': 'Ctrl+Z 撤销',
  'screenshot.help.quickAction': '数字键快捷动作',
}[key]);

test('helpEntries 返回全部已知快捷键条目', () => {
  const entries = helpEntries(t);
  assert.equal(entries.length, 10);
  const ids = entries.map((entry) => entry.id);
  for (const id of ['complete', 'save', 'pin', 'fullscreen', 'cancel', 'nudge', 'square', 'center', 'undo', 'quickAction']) {
    assert.ok(ids.includes(id), '缺少 ' + id + ' 条目');
  }
});

test('helpEntries 每条含非空键与翻译文案', () => {
  for (const entry of helpEntries(t)) {
    assert.ok(Array.isArray(entry.keys) && entry.keys.length > 0, entry.id + ' 键数组为空');
    assert.ok(entry.keys.every((key) => typeof key === 'string' && key.length > 0), entry.id + ' 存在空键');
    assert.equal(typeof entry.label, 'string');
    assert.ok(entry.label.length > 0, entry.id + ' 文案为空');
  }
});

test('helpEntries 拒绝非法翻译函数', () => {
  assert.throws(() => helpEntries(null), /翻译函数/);
  assert.throws(() => helpEntries('x'), /翻译函数/);
});

test('isHelpShortcut 识别 F1 与问号键', () => {
  assert.equal(isHelpShortcut({ key: 'F1' }), true);
  assert.equal(isHelpShortcut({ key: '?', ctrlKey: false, metaKey: false, altKey: false }), true);
});

test('isHelpShortcut 拒绝普通键与带修饰键的 F1', () => {
  assert.equal(isHelpShortcut({ key: 'a' }), false);
  assert.equal(isHelpShortcut({ key: 'F1', ctrlKey: true, metaKey: false, altKey: false }), false);
  assert.equal(isHelpShortcut({ key: '?', ctrlKey: true, metaKey: false, altKey: false }), false);
  assert.equal(isHelpShortcut(null), false);
});
