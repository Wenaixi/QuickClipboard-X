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

// G3: 过滤热键(handleFilterLeft/Right)切子模式会触发 emojiMode effect 无条件
// setKbZone('outside'),grid 态按 Ctrl+← 后键盘导航焦点丢失且 ←/→ 变切主标签。
// 修复:App 在 setEmojiMode 前调 EmojiTab.resetKbNav() 把 kbZone 先置 outside,
// 让 effect 同值短路不跑,键盘导航态得以保留。EmojiTab 必须暴露 resetKbNav/getKbZone。
test('G3 EmojiTab 暴露 resetKbNav/getKbZone 供 App 过滤热键路径使用', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  // 必须暴露 resetKbNav 与 getKbZone(useImperativeHandle 内)
  assert.ok(
    /resetKbNav[\s\S]*?getKbZone[\s\S]*?useImperativeHandle[\s\S]*?resetKbNav[\s\S]*?getKbZone/.test(body),
    'EmojiTab 必须定义并暴露 resetKbNav/getKbZone'
  );
});

test('G3 App handleFilterLeft/Right 在 setEmojiMode 前调 resetKbNav(过滤热键不踢出键盘导航态)', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../../App.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  const filterLeft = body.slice(body.indexOf('const handleFilterLeft'), body.indexOf('const handleFilterRight'));
  const filterRight = body.slice(body.indexOf('const handleFilterRight'), body.indexOf('const handleToggleSearch'));
  // 两个 handler 都必须在 setEmojiMode 前调用 resetKbNav
  assert.ok(
    /resetKbNav[\s\S]*?setEmojiMode/.test(filterLeft),
    'handleFilterLeft 必须在 setEmojiMode 前调 resetKbNav'
  );
  assert.ok(
    /resetKbNav[\s\S]*?setEmojiMode/.test(filterRight),
    'handleFilterRight 必须在 setEmojiMode 前调 resetKbNav'
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
