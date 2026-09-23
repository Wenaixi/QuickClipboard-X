import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

// 屏幕录制热键设置入口护栏：后端(recording_shortcut)与前端默认值
// (recordingShortcut)早就有,唯独设置页无任何 UI 渲染消费——用户无法修改
// 或清空录制热键。本护栏断言设置页已接 UI,且后端/默认三处齐全,防止未来
// 任一处被删导致「后端有键、用户无处配」的口径再度分裂。

const sectionSource = readFileSync(new URL('./ShortcutsSection.jsx', import.meta.url), 'utf8');
const zhLocale = readFileSync(new URL('../../../shared/locales/zh-CN.json', import.meta.url), 'utf8');
const enLocale = readFileSync(new URL('../../../shared/locales/en-US.json', import.meta.url), 'utf8');
const settingsService = readFileSync(
  new URL('../../../shared/services/settingsService.js', import.meta.url),
  'utf8'
);

const label = () => "t('settings.shortcuts.recordingShortcut')";
const desc = () => "t('settings.shortcuts.recordingShortcutDesc')";

test('设置页已渲染屏幕录制热键输入入口', () => {
  assert.ok(
    sectionSource.includes(`label={${label()}}`) && sectionSource.includes(`description={${desc()}}`),
    '设置页必须渲染录制热键条目(标签+描述)'
  );
});

test('录制热键输入必须走 onSettingChange 保存与 recording 后端状态', () => {
  assert.ok(
    sectionSource.includes(`onSettingChange('recordingShortcut', value)`),
    '录制热键必须保存到 recordingShortcut 字段'
  );
  assert.ok(
    sectionSource.includes("hasErrorStatus('recordingShortcut', 'recording')"),
    '录制热键必须关联后端 recording 注册状态,注册失败可见错误'
  );
  assert.ok(
    sectionSource.includes(`handleShortcutChange('recordingShortcut', 'Ctrl+Shift+R')`),
    '重置必须回到后端默认 Ctrl+Shift+R'
  );
});

test('录制热键三处必须齐全(前端默认值+双语语言包键)', () => {
  assert.ok(
    settingsService.includes("recordingShortcut: 'Ctrl+Shift+R'"),
    'settingsService 默认值必须包含 recordingShortcut(与 Rust model 对齐)'
  );
  assert.match(zhLocale, /"recordingShortcut"\s*:/, 'zh 语言包必须含录制热键标签');
  assert.match(zhLocale, /"recordingShortcutDesc"\s*:/, 'zh 语言包必须含录制热键说明');
  assert.match(enLocale, /"recordingShortcut"\s*:/, 'en 语言包必须含录制热键标签');
  assert.match(enLocale, /"recordingShortcutDesc"\s*:/, 'en 语言包必须含录制热键说明');
});