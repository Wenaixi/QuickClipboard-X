import { test } from 'node:test';
import assert from 'node:assert/strict';

test('标准构建始终包含截图窗口页面', async () => {
  const { default: config } = await import(new URL('../../../vite.config.js?standard-screenshot-test', import.meta.url));
  const inputs = config.build.rollupOptions.input;

  assert.equal(
    typeof inputs.screenshot,
    'string',
    'Rust 侧会创建截图窗口，标准构建必须包含其 HTML 入口'
  );
  assert.match(inputs.screenshot.replaceAll('\\', '/'), /src\/windows\/screenshot\/index\.html$/);
});
