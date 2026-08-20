import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('quickAction 键与 actionModel 的数字键映射保持一致', () => {
  const entry = helpEntries(t).find((item) => item.id === 'quickAction');
  assert.deepEqual(entry.keys, ['1', '2', '3', '4']);
});

test('helpEntries 每条含非空键与翻译文案', () => {
  for (const entry of helpEntries(t)) {
    assert.ok(Array.isArray(entry.keys) && entry.keys.length > 0, entry.id + ' 键数组为空');
    assert.ok(entry.keys.every((key) => typeof key === 'string' && key.length > 0), entry.id + ' 存在空键');
    assert.equal(typeof entry.label, 'string');
    assert.ok(entry.label.length > 0, entry.id + ' 文案为空');
  }
});

test('helpEntries 返回 keys 数组拷贝防止共享引用污染', () => {
  const source = readFileSync(new URL('./helpModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function helpEntries');
  const body = source.slice(start, start + 400);
  // 源码护栏：keys 必须展开拷贝（[...item.keys]），禁止直接引用常量数组——
  // 否则调用方 push 修改会污染 HELP_ITEMS 并影响所有后续调用。
  assert.ok(body.includes('keys: [...item.keys]'), 'keys 必须展开拷贝');
  // 行为验证：修改一次调用的返回 keys 不得影响下一次调用（独立副本）。
  const first = helpEntries(t);
  const quickAction = first.find((entry) => entry.id === 'quickAction');
  quickAction.keys.push('9');
  quickAction.keys[0] = 'mutated';
  const second = helpEntries(t).find((entry) => entry.id === 'quickAction');
  assert.deepEqual(second.keys, ['1', '2', '3', '4'], '修改一次调用的 keys 不得污染下一次调用');
  // 两次调用返回的对象也必须是独立实例（深拷贝到 keys 层）。
  assert.notEqual(helpEntries(t).find((entry) => entry.id === 'cancel'), helpEntries(t).find((entry) => entry.id === 'cancel'), '每次调用必须返回独立条目对象');
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

test('isHelpShortcut 源码整体拒绝修饰键且白名单仅 F1 与问号', () => {
  const source = readFileSync(new URL('./helpModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function isHelpShortcut');
  const body = source.slice(start, start + 400);
  // 源码护栏一：必须整体拒绝 ctrl/meta/alt 修饰键（组合键不得误开帮助面板）。
  assert.ok(body.includes('if (event.ctrlKey || event.metaKey || event.altKey) return false;'), '必须整体拒绝修饰键');
  // 源码护栏二：白名单必须只包含 F1 与问号键。
  assert.ok(body.includes("return event.key === 'F1' || event.key === '?';"), '白名单必须仅 F1 与问号');
  // 源码护栏三：非法输入必须返回 false 而非抛错。
  assert.ok(body.includes("if (!event || typeof event !== 'object') return false;"), '非法输入必须安全返回 false');
  // 行为属性：组合键全部拒绝、白名单命中、普通键拒绝。
  assert.equal(isHelpShortcut({ key: 'F1', ctrlKey: true, metaKey: false, altKey: false }), false);
  assert.equal(isHelpShortcut({ key: 'F1', ctrlKey: false, metaKey: true, altKey: false }), false);
  assert.equal(isHelpShortcut({ key: '?', ctrlKey: false, metaKey: false, altKey: true }), false);
  assert.equal(isHelpShortcut({ key: 'F1', ctrlKey: false, metaKey: false, altKey: false }), true);
  assert.equal(isHelpShortcut({ key: '/', ctrlKey: false, metaKey: false, altKey: false }), false);
});


test('helpEntries 源码 label 必须使用翻译函数结果而非原始 key', () => {
  const source = readFileSync(new URL('./helpModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function helpEntries');
  const body = source.slice(start, start + 400);
  // 源码护栏：label 必须取翻译函数的返回结果（若直接写 item.labelKey 会在 UI 显示原始 key）。
  assert.ok(body.includes('label: t(item.labelKey)'), 'label 必须调用翻译函数');
  // 行为验证：翻译函数返回标记值时 label 必须等于标记（证明用了翻译结果而非原始 key）。
  const markerT = (key) => `marker:${key}`;
  const entries = helpEntries(markerT);
  assert.equal(entries.length, 10, '必须返回全部条目');
  for (const entry of entries) {
    assert.ok(entry.label.startsWith('marker:'), `${entry.id} 的 label 必须是翻译结果而非原始 key`);
    assert.ok(entry.label.includes('screenshot.help.'), `${entry.id} 的 label 必须包含翻译 key`);
  }
  // 具体验证第一条：complete 条目的 label 必须是对应 key 的翻译结果。
  const complete = entries.find((entry) => entry.id === 'complete');
  assert.equal(complete.label, 'marker:screenshot.help.complete', 'complete 条目 label 必须是翻译的完成提示');
});
test('帮助条目 labelKey 必须在双语言包中存在', () => {
  const source = readFileSync(new URL('./helpModel.js', import.meta.url), 'utf8');
  const labelKeys = [...source.matchAll(/labelKey: '([^']+)'/g)].map((match) => match[1]);
  assert.ok(labelKeys.length >= 10, '帮助模型必须定义全部快捷键条目');
  for (const locale of ['zh-CN', 'en-US']) {
    const messages = JSON.parse(readFileSync(new URL(`../../shared/locales/${locale}.json`, import.meta.url)));
    for (const key of labelKeys) {
      const parts = key.split('.');
      let value = messages;
      for (const part of parts) value = value?.[part];
      assert.equal(typeof value, 'string', `${locale} 缺少帮助条目 ${key}`);
      assert.ok(value.length > 0, `${locale} 帮助条目 ${key} 文案为空`);
    }
  }
});
