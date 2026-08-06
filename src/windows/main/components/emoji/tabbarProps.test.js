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

// G1: executeCurrentItem 必须加 kbZone==='grid' 守卫,否则 tabbar/search 态按 Enter
// (后端 Enter 热键 → handleExecuteItem)会粘贴 grid 上次停留的陈旧项
test('G1 executeCurrentItem 开头必须有 kbZone==="grid" 守卫', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  const execStart = body.indexOf('const executeCurrentItem');
  const execEnd = body.indexOf('useImperativeHandle', execStart);
  const exec = body.slice(execStart, execEnd);
  // 守卫必须出现在函数体最前(粘贴逻辑之前),且读 kbZoneRef 避开 stale closure
  assert.ok(
    /const zone = kbZoneRef\.current;[\s\S]*?if \(zone !== 'grid'\) return;/.test(exec),
    'executeCurrentItem 函数体开头必须有 kbZoneRef 守卫,否则 tabbar 态 Enter 粘贴陈旧项'
  );
  // 守卫必须位于 showImages 分支之前(所有粘贴路径都被拦截)
  assert.ok(
    exec.indexOf("if (zone !== 'grid') return;") < exec.indexOf('if (showImages)'),
    'kbZone 守卫必须早于 showImages 分支'
  );
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
