import { test } from 'node:test';
import assert from 'node:assert/strict';

// TabNavigation 死 props 与 stale tabbarFocusId 源码护栏测试
// 背景：spec #3 审查发现 handleKbNav 切走主标签后 tabbarFocusId 不清,
// 再次 ← 从 stale ID 起算漂移;standards #2 发现 onKbNav/onKbEnter 死 props。
test('TabNavigation 不声明 onKbNav/onKbEnter 死 props', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabNavigation.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  assert.equal(body.includes('onKbNav'), false, '不应再声明 onKbNav prop');
  assert.equal(body.includes('onKbEnter'), false, '不应再声明 onKbEnter prop');
});

test('TabNavigation 切走主标签/子模式时清 tabbarFocusId', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabNavigation.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  // onTabChange 调用时必须先 setTabbarFocusId(null)(清 stale 焦点)
  assert.ok(
    /setTabbarFocusId\(null\)[\s\S]*?onTabChange\(/.test(body),
    'onTabChange 前必须 setTabbarFocusId(null)'
  );
  // onEmojiModeChange 调用时同样清
  assert.ok(
    /setTabbarFocusId\(null\)[\s\S]*?onEmojiModeChange\(/.test(body),
    'onEmojiModeChange 前必须 setTabbarFocusId(null)'
  );
});
