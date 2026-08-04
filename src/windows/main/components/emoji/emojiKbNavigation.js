/**
 * 表情页键盘区域导航纯逻辑(可 node:test 单测)。
 * 组件只负责 DOM/ref;决策全在这里。
 */

/** 图库 activateKb:同步决定是否成功与目标 index */
export function resolveActivateKb(currentIndex, total) {
  if (total <= 0) return { ok: false, index: -1 };
  if (currentIndex < 0) return { ok: true, index: 0 };
  return { ok: true, index: currentIndex };
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
    if (action === 'tab-right') return { type: 'grid-move', dRow: 0, dCol: 1 };
    if (action === 'tab-left') return { type: 'grid-move', dRow: 0, dCol: -1, onFail: 'enter-sidebar' };
    return { type: 'none' };
  }

  if (kbZone === 'sidebar') {
    if (action === 'navigate-down') return { type: 'sidebar-move', delta: 1 };
    if (action === 'navigate-up') return { type: 'sidebar-move', delta: -1, onFail: 'enter-search' };
    if (action === 'tab-right') return { type: 'enter-grid' };
    if (action === 'tab-left') return { type: 'enter-search' };
    return { type: 'none' };
  }

  return { type: 'none' };
}

/** 侧栏高亮是否应显示在该 cat */
export function isSidebarCategoryActive(catId, activeCategoryId, fallbackFirstId) {
  if (activeCategoryId) return catId === activeCategoryId;
  return catId === fallbackFirstId;
}
