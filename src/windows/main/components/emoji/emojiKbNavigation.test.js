import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  resolveActivateKb,
  resolveSidebarCategoryId,
  resolveOutsideAppAction,
  shouldForwardNavToEmoji,
  resolveZoneNav,
  isSidebarCategoryActive,
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
    assert.deepEqual(resolveZoneNav('sidebar', 'tab-left'), { type: 'enter-search' });
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
});
