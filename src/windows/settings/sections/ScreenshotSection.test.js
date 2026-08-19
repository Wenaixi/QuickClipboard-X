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
  ];
  for (const field of fields) {
    assert.ok(
      source.includes(`onSettingChange('${field}'`) || source.includes(`onSettingChange('${field}',`),
      `${field} 必须有保存接线，不得是无消费的死设置`
    );
  }
});
