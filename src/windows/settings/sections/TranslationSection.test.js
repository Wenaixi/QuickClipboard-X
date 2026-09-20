import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./TranslationSection.jsx', import.meta.url), 'utf8');

test('翻译设置测试按钮必须真实调用后端配置测试命令,不得是仅转圈圈的假实现', () => {
  assert.ok(source.includes('await invoke(\'test_screenshot_ai_config\')'), '测试按钮必须复用 AI 配置测试命令');
  assert.ok(source.includes('settings.translation.testSuccess'), '测试成功必须有成功提示');
  assert.ok(source.includes('settings.translation.testFailed'), '测试失败必须有失败提示');
  assert.ok(!source.includes('setTimeout(() => setTesting(false), 2000)'), '不得残留仅转圈圈的假实现');
});
