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

test('TabNavigation handleKbNav 保留 tabbarFocusId 不清 null(批处理吞高亮)', async () => {
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
  assert.ok(fnBody.includes('focusTabbarButton'), 'handleKbNav 应调用 focusTabbarButton');
  assert.ok(
    /onTabChange\(/.test(fnBody),
    'handleKbNav 应调 onTabChange'
  );
  assert.ok(
    /onEmojiModeChange\(/.test(fnBody),
    'handleKbNav 应调 onEmojiModeChange'
  );
  // F2 修:React 19 自动批处理把同 handler 内两次 setState 合并为最后一次,
  // focusTabbarButton(nextId) 写 + 尾部 null 写 → 渲染值恒 null,
  // isTabbarActive(id) 恒 false,tabbar 键盘遍历高亮永不渲染。
  // 修复:null 写全部删除,tabbarFocusId 常驻当前焦点(再次 ← 正好从原位起算)。
  assert.equal(
    (fnBody.match(/setTabbarFocusId\(null\)/g) || []).length,
    0,
    'handleKbNav 内不得再有 setTabbarFocusId(null),React 19 批处理会把同帧 nextId 写覆盖为 null'
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

// F2-3: collapsedVisibleFilterCount 最小 4 后(commit 6af73f0f 产品决策:
// 全部/文本/图片/链接常驻,文件折叠),useFloatingExpandedFilters =
// !isFilterAutoExpanded && count<=2 && ... 恒 false,浮动展开过滤分支
// (absolute 浮层)成为死代码。保留 4 个设计,删除死分支与恒 false 派生。
test('TabNavigation 无 useFloatingExpandedFilters 恒 false 死逻辑与浮动展开分支', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabNavigation.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  // 恒 false 派生已删
  assert.equal(body.includes('useFloatingExpandedFilters'), false, '不应再有 useFloatingExpandedFilters(最小过滤数 4 后恒 false)');
  // 浮动展开浮层(absolute 弹出)已删,只保留内联展开
  assert.equal(
    /top-\[calc\(100%\+6px\)\]/.test(body),
    false,
    '浮动展开浮层 div 已删除(死代码)'
  );
  // 最小 4 个的常驻设计保留
  assert.ok(body.includes("count >= 4"), 'getVisibleFilterCountByWidth 最小 4 应保留(产品决策)');
  assert.ok(body.includes('return 4;'), 'fallback 4 应保留');
});
// F2-2: handleGroupRevealMouseMove 提前 return 分支(按钮已滑出/面板打开时)
// 必须清除挂起的 300ms 隐藏定时器,否则按钮滑出→鼠标移出→300ms 内移回
// (命中提前 return)→定时器到期→按钮在悬停中收起。对称语义:边缘命中分支
// (502-505)移回即取消隐藏,提前 return 分支同样应取消。
test('TabNavigation 分组按钮悬停中移回不清除隐藏定时器', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../TabNavigation.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  const fnStart = body.indexOf('const handleGroupRevealMouseMove');
  const fnEnd = body.indexOf('const handleGroupRevealMouseLeave', fnStart);
  const fnBody = body.slice(fnStart, fnEnd);
  // 提前 return 分支(isGroupButtonRevealed 为 true 时)必须先清定时器再 return
  assert.ok(
    /if \(isSidebarLayout \|\| isGroupsPanelOpen \|\| isGroupButtonRevealed\) \{[\s\S]{0,300}clearTimeout\(groupRevealTimerRef\.current\)[\s\S]{0,300}return;/.test(fnBody),
    '悬停中(已 reveal)移回时,提前 return 分支必须先 clearTimeout 隐藏定时器,否则悬停中按钮收起'
  );
});
