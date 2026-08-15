import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./ScreenshotSection.jsx', import.meta.url), 'utf8');

test('截图 AI 配置缺失时提供跳转到 AI 配置页的入口', () => {
  assert.match(source, /onNavigateAiConfig/);
  assert.match(source, /configureAi/);
});
