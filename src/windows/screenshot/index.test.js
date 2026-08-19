import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const source = readFileSync(new URL('./index.jsx', import.meta.url), 'utf8');

test('截图窗口启动必须同步用户语言设置', () => {
  assert.ok(source.includes("import { initSettings } from '../../shared/store/settingsStore.js'"), '必须导入设置初始化');
  assert.ok(source.includes('initSettings()'), '必须调用 initSettings');
  assert.ok(source.includes('root.render('), '语言同步完成后才渲染');
  const initIndex = source.indexOf('initSettings()');
  const renderIndex = source.indexOf('root.render(');
  assert.ok(initIndex !== -1 && renderIndex !== -1 && initIndex < renderIndex, '必须先同步语言再渲染，避免启动用错语言');
});
