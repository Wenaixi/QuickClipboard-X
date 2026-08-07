import { test } from 'node:test';
import assert from 'node:assert/strict';

// TabNavigation 死 props 与 stale tabbarFocusId 源码护栏测试
// 背景：spec #3 审查发现 handleKbNav 切走主标签后 tabbarFocusId 不清,
// 再次 ← 从 stale ID 起算漂移;standards #2 发现 onKbNav/onKbEnter 死 props。
// F4 删除整个 tabbar 键盘死码链(enter-tabbar 意图无处产生)后,handleKbNav 系列
// 已整体移除,以下 G2/G5 护栏断言改为"整链不存在"的否定形式。
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

test('TabNavigation 整条 tabbar 键盘死码链已删(handleKbNav/tabbarFocusId/focusTabbar)', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabNavigation.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  assert.equal(body.includes('handleKbNav'), false, '不应再有 handleKbNav');
  assert.equal(body.includes('tabbarFocusId'), false, '不应再有 tabbarFocusId state');
  assert.equal(body.includes('focusTabbarButton'), false, '不应再有 focusTabbarButton');
  assert.equal(body.includes('focusTabbar'), false, '不应再有 focusTabbar');
  assert.equal(body.includes('tabbarRefs'), false, '不应再有 tabbarRefs 收集');
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
