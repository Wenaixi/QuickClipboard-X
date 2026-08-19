import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./AIConfigSection.jsx', import.meta.url), 'utf8');

test('AI 配置测试按钮必须真实调用后端配置测试命令', () => {
  assert.ok(source.includes('await invoke(\'test_screenshot_ai_config\')'), '测试按钮必须调用 test_screenshot_ai_config');
  assert.ok(source.includes('settings.aiConfig.testSuccess'), '测试成功必须有成功提示');
  assert.ok(source.includes('settings.aiConfig.testFailed'), '测试失败必须有失败提示');
  assert.ok(!source.includes('setTimeout(() => setTesting(false), 2000)'), '测试按钮不得是仅转圈圈的假实现');
});

test('AI 配置页推荐模型必须是后端放行的视觉模型且不保留假刷新按钮', () => {
  assert.ok(source.includes('Qwen/Qwen2.5-VL-7B-Instruct'), '推荐模型必须是视觉模型');
  assert.ok(!source.includes('Qwen/Qwen2-7B-Instruct'), '纯文本模型不得作为推荐项');
  assert.ok(!source.includes('handleRefreshModels'), '后端无模型列表 API，刷新模型按钮属假功能必须移除');
});
