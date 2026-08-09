import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readSource } from './readSource.js';

// TabNavigation 死 props 与 stale tabbarFocusId 源码护栏测试
// 背景：spec #3 审查发现 handleKbNav 切走主标签后 tabbarFocusId 不清,
// 再次 ← 从 stale ID 起算漂移;standards #2 发现 onKbNav/onKbEnter 死 props。
// F4 删除整个 tabbar 键盘死码链(enter-tabbar 意图无处产生)后,handleKbNav 系列
// 已整体移除,以下 G2/G5 护栏断言改为"整链不存在"的否定形式。
test('TabNavigation 不声明 onKbNav/onKbEnter 死 props', async () => {
  const body = await readSource('../TabNavigation.jsx');
  assert.equal(body.includes('onKbNav'), false, '不应再声明 onKbNav prop');
  assert.equal(body.includes('onKbEnter'), false, '不应再声明 onKbEnter prop');
});

test('TabNavigation 整条 tabbar 键盘死码链已删(handleKbNav/tabbarFocusId/focusTabbar)', async () => {
  const body = await readSource('../TabNavigation.jsx');
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
  const body = await readSource('../TabNavigation.jsx');
  assert.equal(body.includes('handleKbEnter'), false, 'TabNavigation 不应有 handleKbEnter 函数');
  assert.equal(/kbEnter:\s*handleKbEnter/.test(body), false, 'useImperativeHandle 不应暴露 kbEnter');
});

test('F6 App.jsx 不再有 handleEmojiTabbarEnter', async () => {
  const body = await readSource('../../App.jsx');
  assert.equal(body.includes('handleEmojiTabbarEnter'), false, 'App 不应有 handleEmojiTabbarEnter');
  assert.equal(body.includes('kbEnter'), false, 'App 不应再调 kbEnter');
});

// F7: focusTabbarButton 对主标签调 el.focus?.(),但 TabButton 把 buttonRef 挂外层 div
// 无 tabIndex → .focus() no-op;emoji 模式 tabbarRefs.current[id] = el.querySelector?.('button')
// 拿到内层真 button。两路径不一致 → 主标签 tabbar 焦点无视觉/JS 生效。
// F4 已删整条 tabbar 键盘死码链(tabbarRefs/focusTabbarButton 均不存在),
// 护栏改为否定形式:TabButton 不再需要可编程 focus 的 tabIndex。
test('F7 TabButton 不再有 tabbar 编程焦点用 tabIndex=-1', async () => {
  const body = await readSource('../TabButton.jsx');
  assert.ok(/ref=\{buttonRef\}/.test(body), 'TabButton 外层 div 应仍挂 buttonRef');
  // F4 删死码链后,外层 div 的 tabIndex={-1} 不再有任何调用方(focusTabbarButton 已删)
  assert.equal(
    body.includes('tabIndex={-1}'),
    false,
    '外层 div 不应再有 tabIndex={-1}(唯一调用方 focusTabbarButton 已随死码链删除)'
  );
});

// F2-3: collapsedVisibleFilterCount 最小 4 后(commit 6af73f0f 产品决策:
// 全部/文本/图片/链接常驻,文件折叠),useFloatingExpandedFilters =
// !isFilterAutoExpanded && count<=2 && ... 恒 false,浮动展开过滤分支
// (absolute 浮层)成为死代码。保留 4 个设计,删除死分支与恒 false 派生。
test('TabNavigation 无 useFloatingExpandedFilters 恒 false 死逻辑与浮动展开分支', async () => {
  const body = await readSource('../TabNavigation.jsx');
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
  const body = await readSource('../TabNavigation.jsx');
  const fnStart = body.indexOf('const handleGroupRevealMouseMove');
  const fnEnd = body.indexOf('const handleGroupRevealMouseLeave', fnStart);
  const fnBody = body.slice(fnStart, fnEnd);
  // 提前 return 分支(isGroupButtonRevealed 为 true 时)必须先清定时器再 return
  assert.ok(
    /if \(isSidebarLayout \|\| isGroupsPanelOpen \|\| isGroupButtonRevealed\) \{[\s\S]{0,300}clearTimeout\(groupRevealTimerRef\.current\)[\s\S]{0,300}return;/.test(fnBody),
    '悬停中(已 reveal)移回时,提前 return 分支必须先 clearTimeout 隐藏定时器,否则悬停中按钮收起'
  );
});
