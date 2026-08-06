/**
 * 表情页键盘区域导航纯逻辑(可 node:test 单测)。
 * 组件只负责 DOM/ref;决策全在这里。
 */

/** 图库 activateKb:同步决定是否成功与目标 index */
export function resolveActivateKb(currentIndex, total) {
  if (total <= 0) return { ok: false, index: -1 };
  if (currentIndex < 0) return { ok: true, index: 0 };
  // 越界 clamp 到最后一个合法槽位:搜索缩小结果集后旧 index 可能 >= total,
  // 不 clamp 会把越界 index 写进 kbImageIndexRef → 高亮消失 + Enter 失效
  return { ok: true, index: Math.min(currentIndex, total - 1) };
}

/** 进侧栏时保留当前分类,没有则用第一个 */
export function resolveSidebarCategoryId(categories, activeId) {
  if (!categories || categories.length === 0) return null;
  if (activeId && categories.some((c) => c.id === activeId)) return activeId;
  return categories[0].id;
}

/**
 * outside 态 App 收到后端导航事件时怎么处理。
 * - navigate-down: 激活进 search
 * - tab-left/right: 让 App 切主标签(不接管)
 * - 其它: no-op
 */
export function resolveOutsideAppAction(action) {
  if (action === 'navigate-down') return 'activate';
  if (action === 'tab-left' || action === 'tab-right') return 'passthrough';
  return 'ignore';
}

/**
 * 已激活时 App 是否应把方向键转给 EmojiTab(而不是自己处理)。
 */
export function shouldForwardNavToEmoji(emojiKbActive, action) {
  if (!emojiKbActive) return false;
  return (
    action === 'navigate-up' ||
    action === 'navigate-down' ||
    action === 'tab-left' ||
    action === 'tab-right'
  );
}

/**
 * zone 状态机:根据当前 zone + 导航 action 给出下一步意图。
 * action: navigate-up | navigate-down | tab-left | tab-right
 * 返回 { type, ... } 由组件执行。
 */
export function resolveZoneNav(kbZone, action) {
  if (kbZone === 'outside') {
    if (action === 'navigate-down') return { type: 'activate-search' };
    return { type: 'none' };
  }

  if (kbZone === 'search') {
    if (action === 'navigate-down' || action === 'tab-right') return { type: 'enter-grid' };
    if (action === 'tab-left') return { type: 'enter-sidebar' };
    if (action === 'navigate-up') return { type: 'deactivate' };
    return { type: 'none' };
  }

  if (kbZone === 'grid') {
    if (action === 'navigate-down') return { type: 'grid-move', dRow: 1, dCol: 0 };
    if (action === 'navigate-up') return { type: 'grid-move', dRow: -1, dCol: 0, onFail: 'enter-search' };
    // 最右列 → 越界切下一个子模式(emoji→符号→图片→emoji 循环),由组件 onEmojiModeChange 执行
    if (action === 'tab-right') return { type: 'grid-move', dRow: 0, dCol: 1, onFail: 'next-mode' };
    // grid 首列 ← 越界进侧栏(与搜索框 ← 同一目的地);侧栏再 ← 才进 tabbar 模式层
    if (action === 'tab-left') return { type: 'grid-move', dRow: 0, dCol: -1, onFail: 'enter-sidebar' };
    return { type: 'none' };
  }

  if (kbZone === 'sidebar') {
    if (action === 'navigate-down') return { type: 'sidebar-move', delta: 1 };
    if (action === 'navigate-up') return { type: 'sidebar-move', delta: -1, onFail: 'enter-search' };
    if (action === 'tab-right') return { type: 'enter-grid' };
    // 侧栏再 ← 切上一个子模式(图片→符号→表情),表情(最左)再 ← 切收藏主标签。
    // 目标由组件按当前 emojiMode 决定(prev-mode 意图)。
    if (action === 'tab-left') return { type: 'prev-mode' };
    return { type: 'none' };
  }

  if (kbZone === 'tabbar') {
    if (action === 'navigate-up') return { type: 'enter-search' };
    if (action === 'navigate-down') return { type: 'enter-grid' };
    if (action === 'tab-left') return { type: 'tabbar-move', delta: -1 };
    if (action === 'tab-right') return { type: 'tabbar-move', delta: 1 };
    return { type: 'none' };
  }

  return { type: 'none' };
}

/** 侧栏高亮是否应显示在该 cat */
export function isSidebarCategoryActive(catId, activeCategoryId, fallbackFirstId) {
  // 空串与 null/undefined 同等视为无 active
  if (activeCategoryId) return catId === activeCategoryId;
  return catId === fallbackFirstId;
}

/**
 * tabbar 焦点序列:模式(emoji/symbols/images) + 主标签(收藏/剪贴板)。
 * 返回移动 delta 后的目标 id;越界循环。
 */
// F9 删除:resolveTabbarMove 生产 0 调用,TabNavigation handleKbNav 自管内联循环公式
//  export function resolveTabbarMove(items, currentId, delta) {
//   if (!items || items.length === 0) return null;
//   let idx = items.findIndex((item) => item === currentId);
//   if (idx < 0) idx = 0;
//   const next = (idx + delta + items.length) % items.length;
//   return items[next];
// }

/** 循环切换数组元素:prev 不在数组中时从头开始,越界回绕;空数组返回 undefined(调用方 self-guard) */
export function cycleValue(arr, prev, delta) {
  if (arr.length === 0) return undefined;
  const currentIndex = arr.indexOf(prev);
  if (currentIndex === -1) return arr[delta > 0 ? 0 : arr.length - 1];
  const next = (currentIndex + delta + arr.length) % arr.length;
  return arr[next];
}
