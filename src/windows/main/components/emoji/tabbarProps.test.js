import { test } from 'node:test';
import assert from 'node:assert/strict';

// EmojiTab 注入 → props 源码护栏(spec #2 懒加载时序修复)
// background: e99b773e 用 App useEffect + setEnterTabbarHandler 注入,
// EmojiTab lazy 挂载后 App 无重渲触发,onEnterTabbarRef 永远 null,
// grid ← 越界进 tabbar 静默失败。改 props 直传根治。

test('EmojiTab 用 onEnterTabbar/onTabbarMove props 而非 setter 注入', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  // 声明新 props
  assert.ok(/function EmojiTab\(\{ emojiMode, onEmojiModeChange, onEnterTabbar, onTabbarMove \}/.test(body), '应声明 onEnterTabbar/onTabbarMove props');
  // 删 setter 注入
  assert.equal(body.includes('setEnterTabbarHandler'), false, '不应再有 setEnterTabbarHandler');
  assert.equal(body.includes('setTabbarMoveHandler'), false, '不应再有 setTabbarMoveHandler');
  // 意图分发直接调 props
  assert.ok(body.includes('onEnterTabbar?.()'), 'enter-tabbar 应直接调 onEnterTabbar prop');
  assert.ok(body.includes('onTabbarMove?.(intent.delta)'), 'tabbar-move 应直接调 onTabbarMove prop');
});

test('App.jsx 直传 onEnterTabbar/onTabbarMove props 且删注入 useEffect', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../../App.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  assert.ok(body.includes('onEnterTabbar={handleEmojiEnterTabbar}'), 'EmojiTab 应直传 onEnterTabbar');
  assert.ok(body.includes('onTabbarMove={handleEmojiTabbarMove}'), 'EmojiTab 应直传 onTabbarMove');
  assert.equal(body.includes('setEnterTabbarHandler'), false, 'App 不应再调 setter 注入');
  assert.equal(body.includes('setTabbarMoveHandler'), false, 'App 不应再调 setter 注入');
});
