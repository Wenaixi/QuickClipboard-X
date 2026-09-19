import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./ScreenshotSection.jsx', import.meta.url), 'utf8');

test('截图 AI 配置缺失时提供跳转到 AI 配置页的入口', () => {
  assert.match(source, /onNavigateAiConfig/);
  assert.match(source, /configureAi/);
});

test('截图设置节每个字段都通过 onSettingChange 保存', () => {
  const fields = [
    'screenshotEnabled',
    'screenshotElementDetection',
    'screenshotMagnifierEnabled',
    'screenshotHintsEnabled',
    'screenshotColorIncludeFormat',
    'screenshotAiEnabled',
    'screenshotAiPrompt',
    'screenshotWindowLifecycleMode',
    'screenshotAutoDisposeMinutes',
    'screenshotAfterCaptureActions',
  ];
  for (const field of fields) {
    assert.ok(
      source.includes(`onSettingChange('${field}'`) || source.includes(`onSettingChange('${field}',`),
      `${field} 必须有保存接线，不得是无消费的死设置`
    );
  }
});

test('截图后动作链设置项提供多选并保持默认复制', () => {
  // 动作链配置必须走 MultiSegmentedControl 多选，且后端默认值仅复制。
  assert.match(source, /MultiSegmentedControl/);
  assert.match(source, /onSettingChange\('screenshotAfterCaptureActions'/);
  assert.match(source, /afterCaptureActions/);
  // 空数组必须回退复制（防止静默不动作）。
  assert.match(source, /next\.length > 0 \? next : \['copy'\]/);
});
