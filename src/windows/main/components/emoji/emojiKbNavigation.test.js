import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  resolveActivateKb,
  resolveSidebarCategoryId,
  resolveOutsideAppAction,
  shouldForwardNavToEmoji,
  resolveZoneNav,
  isSidebarCategoryActive,
  resolveTabbarMove,
} from './emojiKbNavigation.js';

describe('resolveActivateKb', () => {
  it('total=0 失败', () => {
    assert.deepEqual(resolveActivateKb(-1, 0), { ok: false, index: -1 });
  });
  it('current<0 且有图时选 0 且 ok', () => {
    // 回归:旧实现 setState 后立刻读 ref 仍是 -1 → 永远进不去
    assert.deepEqual(resolveActivateKb(-1, 12), { ok: true, index: 0 });
  });
  it('已有 index 保持', () => {
    assert.deepEqual(resolveActivateKb(3, 12), { ok: true, index: 3 });
  });
});

describe('resolveSidebarCategoryId', () => {
  const cats = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];
  it('空列表 null', () => {
    assert.equal(resolveSidebarCategoryId([], 'a'), null);
  });
  it('保留当前 active', () => {
    assert.equal(resolveSidebarCategoryId(cats, 'b'), 'b');
  });
  it('active 不在列表时回退第一个', () => {
    assert.equal(resolveSidebarCategoryId(cats, 'missing'), 'a');
  });
  it('无 active 用第一个(不是永远 cats[0] 覆盖已有)', () => {
    assert.equal(resolveSidebarCategoryId(cats, null), 'a');
  });
});

describe('resolveOutsideAppAction', () => {
  it('↓ 激活', () => {
    assert.equal(resolveOutsideAppAction('navigate-down'), 'activate');
  });
  it('左右 passthrough 切主标签', () => {
    assert.equal(resolveOutsideAppAction('tab-left'), 'passthrough');
    assert.equal(resolveOutsideAppAction('tab-right'), 'passthrough');
  });
  it('↑ ignore', () => {
    assert.equal(resolveOutsideAppAction('navigate-up'), 'ignore');
  });
});

describe('shouldForwardNavToEmoji', () => {
  it('未激活不转发', () => {
    assert.equal(shouldForwardNavToEmoji(false, 'navigate-down'), false);
  });
  it('激活时四向转发', () => {
    for (const a of ['navigate-up', 'navigate-down', 'tab-left', 'tab-right']) {
      assert.equal(shouldForwardNavToEmoji(true, a), true);
    }
  });
  it('激活时 filter 不转发(App 自己切子模式)', () => {
    assert.equal(shouldForwardNavToEmoji(true, 'filter-left'), false);
  });
});

describe('resolveZoneNav', () => {
  it('outside 仅 down 激活 search', () => {
    assert.deepEqual(resolveZoneNav('outside', 'navigate-down'), { type: 'activate-search' });
    assert.deepEqual(resolveZoneNav('outside', 'tab-left'), { type: 'none' });
  });
  it('search: down/right→grid left→sidebar up→deactivate', () => {
    assert.deepEqual(resolveZoneNav('search', 'navigate-down'), { type: 'enter-grid' });
    assert.deepEqual(resolveZoneNav('search', 'tab-right'), { type: 'enter-grid' });
    assert.deepEqual(resolveZoneNav('search', 'tab-left'), { type: 'enter-sidebar' });
    assert.deepEqual(resolveZoneNav('search', 'navigate-up'), { type: 'deactivate' });
  });
  it('grid 移动与越界回退', () => {
    assert.deepEqual(resolveZoneNav('grid', 'navigate-down'), { type: 'grid-move', dRow: 1, dCol: 0 });
    assert.deepEqual(resolveZoneNav('grid', 'navigate-up'), {
      type: 'grid-move', dRow: -1, dCol: 0, onFail: 'enter-search',
    });
    assert.deepEqual(resolveZoneNav('grid', 'tab-left'), {
      type: 'grid-move', dRow: 0, dCol: -1, onFail: 'enter-tabbar',
    });
  });
  it('sidebar 移动与回搜索', () => {
    assert.deepEqual(resolveZoneNav('sidebar', 'navigate-down'), { type: 'sidebar-move', delta: 1 });
    assert.deepEqual(resolveZoneNav('sidebar', 'navigate-up'), {
      type: 'sidebar-move', delta: -1, onFail: 'enter-search',
    });
    assert.deepEqual(resolveZoneNav('sidebar', 'tab-right'), { type: 'enter-grid' });
    assert.deepEqual(resolveZoneNav('sidebar', 'tab-left'), { type: 'enter-tabbar' });
  });
  it('grid 首列 ← 越界进 tabbar(不再是 sidebar)', () => {
    assert.deepEqual(resolveZoneNav('grid', 'tab-left'), {
      type: 'grid-move', dRow: 0, dCol: -1, onFail: 'enter-tabbar',
    });
  });
  it('tabbar: up→search down→grid 左右→tabbar-move', () => {
    assert.deepEqual(resolveZoneNav('tabbar', 'navigate-up'), { type: 'enter-search' });
    assert.deepEqual(resolveZoneNav('tabbar', 'navigate-down'), { type: 'enter-grid' });
    assert.deepEqual(resolveZoneNav('tabbar', 'tab-left'), { type: 'tabbar-move', delta: -1 });
    assert.deepEqual(resolveZoneNav('tabbar', 'tab-right'), { type: 'tabbar-move', delta: 1 });
  });
});

describe('resolveTabbarMove', () => {
  const items = ['emoji', 'symbols', 'images', 'favorites', 'clipboard'];
  it('右移', () => {
    assert.equal(resolveTabbarMove(items, 'emoji', 1), 'symbols');
    assert.equal(resolveTabbarMove(items, 'images', 1), 'favorites');
  });
  it('左移', () => {
    assert.equal(resolveTabbarMove(items, 'symbols', -1), 'emoji');
    assert.equal(resolveTabbarMove(items, 'favorites', -1), 'images');
  });
  it('越界循环', () => {
    assert.equal(resolveTabbarMove(items, 'clipboard', 1), 'emoji');
    assert.equal(resolveTabbarMove(items, 'emoji', -1), 'clipboard');
  });
  it('未知 current 从第一个开始', () => {
    assert.equal(resolveTabbarMove(items, 'unknown', 1), 'symbols');
  });
  it('空列表 null', () => {
    assert.equal(resolveTabbarMove([], 'emoji', 1), null);
  });
});

describe('isSidebarCategoryActive', () => {
  it('有 active 时精确匹配', () => {
    assert.equal(isSidebarCategoryActive('b', 'b', 'a'), true);
    assert.equal(isSidebarCategoryActive('a', 'b', 'a'), false);
  });
  it('无 active 时 fallback 第一项', () => {
    assert.equal(isSidebarCategoryActive('a', null, 'a'), true);
    assert.equal(isSidebarCategoryActive('b', null, 'a'), false);
  });
  it('空字符串 active 视为无,走 fallback', () => {
    assert.equal(isSidebarCategoryActive('a', '', 'a'), true);
    assert.equal(isSidebarCategoryActive('b', '', 'a'), false);
  });
});

describe('端到端状态机路径(模拟用户)', () => {
  it('outside→↓search→↓grid→←tabbar→←search→↑outside', () => {
    let zone = 'outside';
    const step = (action) => {
      const intent = resolveZoneNav(zone, action);
      if (intent.type === 'activate-search' || intent.type === 'enter-search') zone = 'search';
      else if (intent.type === 'enter-grid') zone = 'grid';
      else if (intent.type === 'enter-sidebar') zone = 'sidebar';
      else if (intent.type === 'enter-tabbar') zone = 'tabbar';
      else if (intent.type === 'deactivate') zone = 'outside';
      else if (intent.type === 'grid-move' && intent.onFail === 'enter-tabbar') {
        zone = 'tabbar';
      }
      return intent;
    };
    assert.equal(resolveOutsideAppAction('navigate-down'), 'activate');
    step('navigate-down');
    assert.equal(zone, 'search');
    step('navigate-down');
    assert.equal(zone, 'grid');
    step('tab-left'); // onFail enter-tabbar 路径由组件在 move 失败时触发;这里直接模拟
    assert.equal(zone, 'tabbar');
    step('tab-left'); // tabbar 内 ←/→ 是模式/主标签移动,zone 不变
    assert.equal(zone, 'tabbar');
    step('navigate-up'); // tabbar ↑ 回搜索
    assert.equal(zone, 'search');
    step('navigate-up');
    assert.equal(zone, 'outside');
    assert.equal(shouldForwardNavToEmoji(false, 'tab-left'), false);
    assert.equal(resolveOutsideAppAction('tab-left'), 'passthrough');
  });
});

describe('App 转发契约源码护栏', () => {
  it('App.jsx 含 dispatchEmojiNav 与四向 action', async () => {
    const fs = await import('node:fs/promises');
    const path = await import('node:path');
    const { fileURLToPath } = await import('node:url');
    const here = path.dirname(fileURLToPath(import.meta.url));
    const appSrc = await fs.readFile(path.join(here, '../../App.jsx'), 'utf8');
    // 剥行注释再匹配
    const body = appSrc
      .split('\n')
      .filter((l) => !l.trimStart().startsWith('//'))
      .join('\n');
    assert.ok(body.includes('dispatchEmojiNav'), '缺 dispatchEmojiNav');
    assert.ok(body.includes("dispatchEmojiNav('navigate-down')"), '↓ 未转发');
    assert.ok(body.includes("dispatchEmojiNav('navigate-up')"), '↑ 未转发');
    assert.ok(body.includes("dispatchEmojiNav('tab-left')"), '← 未转发');
    assert.ok(body.includes("dispatchEmojiNav('tab-right')"), '→ 未转发');
    assert.ok(body.includes('handleNavAction'), '未调 EmojiTab.handleNavAction');
    assert.ok(body.includes('shouldForwardNavToEmoji'), '缺 shouldForwardNavToEmoji');
    assert.ok(body.includes('resolveOutsideAppAction'), '缺 resolveOutsideAppAction');
  });

  it('EmojiTab 暴露 handleNavAction 且不挂 arrow keydown 主路径', async () => {
    const fs = await import('node:fs/promises');
    const path = await import('node:path');
    const { fileURLToPath } = await import('node:url');
    const here = path.dirname(fileURLToPath(import.meta.url));
    const src = await fs.readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');
    const body = src
      .split('\n')
      .filter((l) => !l.trimStart().startsWith('//'))
      .join('\n');
    assert.ok(body.includes('handleNavAction'), '缺 handleNavAction');
    assert.ok(body.includes('resolveZoneNav'), '缺 resolveZoneNav');
    // 禁止再挂裸 Arrow 主路径(会与热键双触发)
    assert.equal(body.includes("addEventListener('keydown'"), false, '不应再 window keydown 吃方向键');
    assert.ok(body.includes('resolveSidebarCategoryId'), '进侧栏应保留当前分类');
    assert.ok(body.includes('isSidebarCategoryActive'), '侧栏应受控高亮');
  });

  it('ImageLibraryTab activateKb 走 resolveActivateKb', async () => {
    const fs = await import('node:fs/promises');
    const path = await import('node:path');
    const { fileURLToPath } = await import('node:url');
    const here = path.dirname(fileURLToPath(import.meta.url));
    const src = await fs.readFile(path.join(here, 'ImageLibraryTab.jsx'), 'utf8');
    const body = src
      .split('\n')
      .filter((l) => !l.trimStart().startsWith('//'))
      .join('\n');
    assert.ok(body.includes('resolveActivateKb'), 'activateKb 必须用 resolveActivateKb');
    assert.ok(body.includes('kbImageIndexRef.current = result.index'), '必须同步写 ref');
    assert.ok(body.includes('kbImageIndexRef.current = -1'), 'resetKbIndex 必须同步清 ref');
  });
});
