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
  // 清 stale 焦点语义改为"切换生效后统一清",不再在每个分支内重复写
  const fnStart = body.indexOf('const handleKbNav');
  const fnEnd = body.indexOf('useImperativeHandle', fnStart);
  const fnBody = body.slice(fnStart, fnEnd);
  // onTabChange 调用前不再要求 setTabbarFocusId(null)(G2 修后焦点先写后清,批处理只留最后一次)
  assert.ok(fnBody.includes('focusTabbarButton'), 'handleKbNav 应调用 focusTabbarButton');
  assert.ok(
    /onTabChange\(/.test(fnBody),
    'handleKbNav 应调 onTabChange'
  );
  assert.ok(
    /onEmojiModeChange\(/.test(fnBody),
    'handleKbNav 应调 onEmojiModeChange'
  );
  // G2: 5 个分支内的 null 写全部收敛为"切换生效后统一清"
  assert.equal(
    (fnBody.match(/setTabbarFocusId\(null\)/g) || []).length,
    1,
    'handleKbNav 内 setTabbarFocusId(null) 只能有 1 处(统一清),否则 React 批处理吞掉 focus 高亮'
  );
  // 统一清必须位于分支 if/else 链之后(以最后一个分支调用 onEmojiModeChange('images') 为锚)
  const lastImagesCallIdx = fnBody.indexOf("onEmojiModeChange('images')");
  assert.ok(
    lastImagesCallIdx >= 0 && fnBody.lastIndexOf('setTabbarFocusId(null)') > lastImagesCallIdx,
    '统一清 null 必须在分支链之后,否则高亮仍被吞'
  );
});

// G5: handleKbNav 硬编码 items=['emoji','symbols','images','favorites','clipboard']
// 与 :116 tabs 过滤(visibleOptionalTabs)脱钩:隐藏 favorites 后循环到它 →
// focus no-op + onTabChange('favorites') 被 App.jsx:88-93 守卫弹回 clipboard,瞬闪。
// 修复:items 从可见 tabs 派生(emoji 子模式 3 项在前 + 可见主标签在后)。
test('G5 handleKbNav items 从可见 tabs 派生,不再硬编码隐藏 tab', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabNavigation.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  const fnStart = body.indexOf('const handleKbNav');
  const fnEnd = body.indexOf('useImperativeHandle', fnStart);
  const fnBody = body.slice(fnStart, fnEnd);
  // 不再有硬编码的 favorites/clipboard 字面 items 数组
  assert.equal(
    /const items = \['emoji', 'symbols', 'images', 'favorites', 'clipboard'\]/.test(fnBody),
    false,
    'handleKbNav 不应再硬编码 5 项 items 数组'
  );
  // items 必须从 tabs 派生
  assert.ok(
    /tabs\.map\(tab => tab\.id\)/.test(fnBody),
    'items 必须从可见 tabs 派生(tabs.map)'
  );
  assert.ok(
    /\[\.\.\.emojiModeItems, \.\.\.visibleMainTabs\]/.test(fnBody),
    'items 应为 emoji 子模式 3 项在前 + 可见主标签在后'
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
