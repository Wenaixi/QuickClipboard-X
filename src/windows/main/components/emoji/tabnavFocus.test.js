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

// F6: handleEmojiTabbarEnter / TabNavigation.kbEnter 死代码
// 实证:handleKbNav 每次方向键都先 setTabbarFocusId(null) + 立即调 onTabChange/onEmojiModeChange,
// handleKbEnter 再读 tabbarFocusId 永远 null,fallback 到 emojiMode,但 emojiMode 已被 handleKbNav
// 改到目标态,Enter 等价 no-op。TabNavigation useImperativeHandle 暴露的 kbEnter 也无 caller
// (App.jsx handleEmojiTabbarEnter 无引用,EmojiTab 不知 onTabbarEnter prop)。
test('F6 TabNavigation 不暴露 kbEnter 给外部', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabNavigation.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  assert.equal(body.includes('handleKbEnter'), false, 'TabNavigation 不应有 handleKbEnter 函数');
  assert.equal(/kbEnter:\s*handleKbEnter/.test(body), false, 'useImperativeHandle 不应暴露 kbEnter');
});

test('F6 App.jsx 不再有 handleEmojiTabbarEnter', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../../App.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  assert.equal(body.includes('handleEmojiTabbarEnter'), false, 'App 不应有 handleEmojiTabbarEnter');
  assert.equal(body.includes('kbEnter'), false, 'App 不应再调 kbEnter');
});

// F7: focusTabbarButton 对主标签调 el.focus?.(),但 TabButton 把 buttonRef 挂外层 div
// 无 tabIndex → .focus() no-op;emoji 模式 tabbarRefs.current[id] = el.querySelector?.('button')
// 拿到内层真 button。两路径不一致 → 主标签 tabbar 焦点无视觉/JS 生效。
// 修复:TabButton.jsx outer div 加 tabIndex={-1} 让 div 编程可达,el.focus?.() 真正生效。
test('F7 TabButton outer div 可编程 focus(tabIndex=-1)', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabButton.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  assert.ok(/ref=\{buttonRef\}/.test(body), 'TabButton 外层 div 应仍挂 buttonRef');
  const outerDivMatch = body.match(/<div\s+ref=\{buttonRef\}[\s\S]{0,200}>/);
  assert.ok(outerDivMatch, '应找到外层 div 标签块');
  assert.ok(
    outerDivMatch[0].includes('tabIndex={-1}'),
    '外层 div 必须 tabIndex={-1},否则 focus no-op'
  );
});
