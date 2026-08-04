import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  resolveActivateKb,
  resolveSidebarCategoryId,
  resolveOutsideAppAction,
  shouldForwardNavToEmoji,
  resolveZoneNav,
  isSidebarCategoryActive,
  resolveSidebarCycleStep,
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
      type: 'grid-move', dRow: 0, dCol: -1, onFail: 'enter-sidebar',
    });
  });
  it('sidebar 移动与回搜索', () => {
    assert.deepEqual(resolveZoneNav('sidebar', 'navigate-down'), { type: 'sidebar-move', delta: 1 });
    assert.deepEqual(resolveZoneNav('sidebar', 'navigate-up'), {
      type: 'sidebar-move', delta: -1, onFail: 'enter-search',
    });
    assert.deepEqual(resolveZoneNav('sidebar', 'tab-right'), { type: 'enter-grid' });
    assert.deepEqual(resolveZoneNav('sidebar', 'tab-left'), { type: 'cycle-mode-or-escape' });
  });
});

describe('resolveSidebarCycleStep', () => {
  it('emoji(自然序首位,反向终点)→ escape favorites', () => {
    assert.deepEqual(resolveSidebarCycleStep('emoji'), { action: 'escape', tab: 'favorites' });
  });
  it('symbols → emoji(反向)', () => {
    assert.deepEqual(resolveSidebarCycleStep('symbols'), { action: 'set-mode', mode: 'emoji' });
  });
  it('images → symbols(反向)', () => {
    assert.deepEqual(resolveSidebarCycleStep('images'), { action: 'set-mode', mode: 'symbols' });
  });
  it('未知模式直接 escape', () => {
    assert.deepEqual(resolveSidebarCycleStep('foo'), { action: 'escape', tab: 'favorites' });
  });
  it('自定义 escapeTab', () => {
    assert.deepEqual(resolveSidebarCycleStep('emoji', 'clipboard'), {
      action: 'escape', tab: 'clipboard',
    });
  });
  it('完整反向序列 images→symbols→emoji→escape', () => {
    const trace = [];
    let mode = 'images';
    for (let i = 0; i < 4; i += 1) {
      const step = resolveSidebarCycleStep(mode);
      trace.push(step);
      if (step.action === 'set-mode') mode = step.mode;
      else break;
    }
    assert.deepEqual(trace, [
      { action: 'set-mode', mode: 'symbols' },
      { action: 'set-mode', mode: 'emoji' },
      { action: 'escape', tab: 'favorites' },
    ]);
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
  it('outside→↓search→↓grid→←sidebar(首列越界)→cycle→↑outside', () => {
    let zone = 'outside';
    const step = (action) => {
      const intent = resolveZoneNav(zone, action);
      if (intent.type === 'activate-search' || intent.type === 'enter-search') zone = 'search';
      else if (intent.type === 'enter-grid') zone = 'grid';
      else if (intent.type === 'enter-sidebar') zone = 'sidebar';
      else if (intent.type === 'cycle-mode-or-escape') {
        // 委托给 resolveSidebarCycleStep;组件层不在 zone 状态机里改 zone
      } else if (intent.type === 'deactivate') zone = 'outside';
      else if (intent.type === 'grid-move' && intent.onFail === 'enter-sidebar') {
        zone = 'sidebar';
      } else if (intent.type === 'sidebar-move' && intent.onFail === 'enter-search') {
        // 顶部越界回搜索
        zone = 'search';
      }
      return intent;
    };
    assert.equal(resolveOutsideAppAction('navigate-down'), 'activate');
    step('navigate-down');
    assert.equal(zone, 'search');
    step('navigate-down');
    assert.equal(zone, 'grid');
    step('tab-left'); // onFail enter-sidebar(首列越界)
    assert.equal(zone, 'sidebar');
    // 新行为:sidebar tab-left 返回 cycle-mode-or-escape,zone 保持
    assert.deepEqual(step('tab-left'), { type: 'cycle-mode-or-escape' });
    assert.equal(zone, 'sidebar');
    // 侧栏顶↑回搜索
    step('navigate-up');
    assert.equal(zone, 'search');
    // 搜索↑反激活
    step('navigate-up');
    assert.equal(zone, 'outside');
    assert.equal(shouldForwardNavToEmoji(false, 'tab-left'), false);
    assert.equal(resolveOutsideAppAction('tab-left'), 'passthrough');
  });
});

describe('App 转发契约源码护栏', () => {
  it('App.jsx 含 dispatchEmojiNav + 队列重放 + Escape 回调', async () => {
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
    assert.ok(body.includes('resolveOutsideAppAction'), '缺 resolveOutsideAppAction');
    // lazy-load 竞态修复:队列 + callback ref
    assert.ok(body.includes('pendingEmojiNavRef'), '缺 pendingEmojiNavRef 队列');
    assert.ok(body.includes('setEmojiTabRef'), '缺 setEmojiTabRef callback ref');
    // 跳出回调
    assert.ok(body.includes('onEscapeFromSidebar={setActiveTab}'), '缺 onEscapeFromSidebar 传出');
  });

  it('EmojiTab 暴露 handleNavAction + 不挂 keydown + 接收 onEscapeFromSidebar', async () => {
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
    assert.equal(body.includes("addEventListener('keydown'"), false, '不应再 window keydown 吃方向键');
    assert.ok(body.includes('resolveSidebarCategoryId'), '进侧栏应保留当前分类');
    assert.ok(body.includes('isSidebarCategoryActive'), '侧栏应受控高亮');
    assert.ok(body.includes('resolveSidebarCycleStep'), '侧栏 ← 应委托 resolveSidebarCycleStep');
    assert.ok(body.includes('onEscapeFromSidebar'), '缺 onEscapeFromSidebar 回调');
    // enterSidebar 必须主动同步 activeCategory
    assert.ok(
      body.match(/setActiveCategory\(catId\)/),
      'enterSidebar 应主动 setActiveCategory(catId),避免依赖 updateSidebarHighlight 早返',
    );
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
