import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readSource, readSourceRaw } from './readSource.js';
import {
  resolveActivateKb,
  resolveSidebarCategoryId,
  resolveOutsideAppAction,
  shouldForwardNavToEmoji,
  resolveZoneNav,
  resolveGridActivation,
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
  it('越界 clamp 到最后一个合法槽位', () => {
    // 回归:搜索缩小结果集后旧 index 可能 >= total,旧实现原样返回越界 index,
    // grid 激活写越界 index → 高亮消失 + Enter 失效
    assert.deepEqual(resolveActivateKb(5, 3), { ok: true, index: 2 });
    assert.deepEqual(resolveActivateKb(99, 1), { ok: true, index: 0 });
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

describe('resolveGridActivation', () => {
  it('内容尚未加载时等待，不把第一次方向键降级到旧搜索区', () => {
    assert.deepEqual(resolveGridActivation(0), { type: 'pending' });
  });

  it('内容已加载时一次激活第一个网格项', () => {
    assert.deepEqual(resolveGridActivation(3), { type: 'activate', index: 0 });
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
  it('outside 按一次 down 直接进入 grid', () => {
    assert.deepEqual(resolveZoneNav('outside', 'navigate-down'), { type: 'enter-grid' });
    assert.deepEqual(resolveZoneNav('outside', 'tab-left'), { type: 'none' });
  });
  it('已取消的 search zone 不再参与导航', () => {
    for (const action of ['navigate-up', 'navigate-down', 'tab-left', 'tab-right']) {
      assert.deepEqual(resolveZoneNav('search', action), { type: 'none' });
    }
  });
  it('grid 移动与越界回退', () => {
    assert.deepEqual(resolveZoneNav('grid', 'navigate-down'), { type: 'grid-move', dRow: 1, dCol: 0 });
    assert.deepEqual(resolveZoneNav('grid', 'navigate-up'), {
      type: 'grid-move', dRow: -1, dCol: 0, onFail: 'deactivate',
    });
    assert.deepEqual(resolveZoneNav('grid', 'tab-left'), {
      type: 'grid-move', dRow: 0, dCol: -1, onFail: 'switch-tab-left',
    });
    assert.deepEqual(resolveZoneNav('grid', 'tab-right'), {
      type: 'grid-move', dRow: 0, dCol: 1, onFail: 'switch-tab-right',
    });
  });
  it('sidebar 移动与回搜索', () => {
    assert.deepEqual(resolveZoneNav('sidebar', 'navigate-down'), { type: 'sidebar-move', delta: 1 });
    assert.deepEqual(resolveZoneNav('sidebar', 'navigate-up'), {
      type: 'sidebar-move', delta: -1, onFail: 'deactivate',
    });
    assert.deepEqual(resolveZoneNav('sidebar', 'tab-right'), { type: 'enter-grid' });
    assert.deepEqual(resolveZoneNav('sidebar', 'tab-left'), { type: 'prev-mode' });
  });
  it('grid 首列 ← 与最右列 → 都请求顶层标签切换', () => {
    assert.deepEqual(resolveZoneNav('grid', 'tab-left'), {
      type: 'grid-move', dRow: 0, dCol: -1, onFail: 'switch-tab-left',
    });
    assert.deepEqual(resolveZoneNav('grid', 'tab-right'), {
      type: 'grid-move', dRow: 0, dCol: 1, onFail: 'switch-tab-right',
    });
  });
  // tabbar zone 不再参与 Emoji 网格导航,未知 zone 对任意动作均无操作
  it('tabbar zone 不可达:任何 action 返回 none(防死码复活)', () => {
    for (const a of ['navigate-up', 'navigate-down', 'tab-left', 'tab-right']) {
      assert.deepEqual(resolveZoneNav('tabbar', a), { type: 'none' });
    }
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
  it('outside→↓grid→左右边界请求顶层切换', () => {
    let zone = 'outside';
    const step = (action) => {
      const intent = resolveZoneNav(zone, action);
      if (intent.type === 'enter-grid') zone = 'grid';
      else if (intent.type === 'deactivate') zone = 'outside';
      return intent;
    };
    assert.equal(resolveOutsideAppAction('navigate-down'), 'activate');
    step('navigate-down');
    assert.equal(zone, 'grid');
    assert.deepEqual(step('tab-left'), {
      type: 'grid-move', dRow: 0, dCol: -1, onFail: 'switch-tab-left',
    });
    assert.deepEqual(step('tab-right'), {
      type: 'grid-move', dRow: 0, dCol: 1, onFail: 'switch-tab-right',
    });
    assert.equal(shouldForwardNavToEmoji(false, 'tab-left'), false);
    assert.equal(resolveOutsideAppAction('tab-left'), 'passthrough');
  });
});

// F3 resetKbToOutside 收敛(去重置四连重复):blurSearchInput/resetKbNav/emojiMode
// effect/越界 effect 四处内联同一重置四连,收敛为共享 resetKbToOutside
describe('F3 resetKbToOutside 收敛(去重置四连重复)', () => {
  // 回归护栏:blurSearchInput/resetKbNav/emojiMode effect/越界 effect 四处内联同一
  // 重置四连(setKbZone outside + kbRow -1 + kbCol 0 + resetKbIndex)。收敛为共享
  // resetKbToOutside 后,四连只应出现在定义处,其余调用点一律复用。
  it('重置四连只出现一次,且 blurSearchInput/resetKbNav 为共享函数别名', async () => {
    const body = await readSource('../EmojiTab.jsx');

    const resetBody = /setKbZone\('outside'\)[\s\S]{0,80}?setKbRow\(-1\)[\s\S]{0,80}?setKbCol\(0\)[\s\S]{0,80}?imageLibraryRef\.current\?\.resetKbIndex\?\.\(\)/g;
    const count = (body.match(resetBody) || []).length;
    assert.equal(count, 1, '重置四连应只存在于 resetKbToOutside 定义处,其余调用点必须复用');
    assert.ok(body.includes('const resetKbToOutside = useCallback('), '缺共享重置函数 resetKbToOutside');
    assert.equal(body.includes('blurSearchInput'), false, '已取消的 search 退出别名不应残留');
    assert.ok(body.includes('const resetKbNav = resetKbToOutside'), 'resetKbNav 应复用共享函数,不再内联重复体');
  });
});

// F4: tabbar zone 死码已删(enter-tabbar 意图无处产生),护栏改为否定形式
describe('F4 tabbar 死码已删(enter-tabbar 意图无处产生)', () => {
  it('EmojiTab applyNavIntent 不再有 enter-tabbar/tabbar-move 分支', async () => {
    const body = await readSource('../EmojiTab.jsx');
    assert.equal(body.includes("case 'enter-tabbar':"), false, 'applyNavIntent 不应再有 enter-tabbar 分支');
    assert.equal(body.includes("case 'tabbar-move':"), false, 'applyNavIntent 不应再有 tabbar-move 分支');
    assert.equal(body.includes("intent.onFail === 'enter-tabbar'"), false, 'grid-move 不应再有 onFail enter-tabbar');
    assert.equal(body.includes('onEnterTabbar'), false, 'EmojiTab 不应再引用 onEnterTabbar prop');
    assert.equal(body.includes('onTabbarMove'), false, 'EmojiTab 不应再引用 onTabbarMove prop');
  });

  it('App.jsx 不再有 handleEmojiEnterTabbar/handleEmojiTabbarMove 与 props 转发', async () => {
    const body = await readSource('../../App.jsx');
    assert.equal(body.includes('handleEmojiEnterTabbar'), false, 'App 不应再有 handleEmojiEnterTabbar');
    assert.equal(body.includes('handleEmojiTabbarMove'), false, 'App 不应再有 handleEmojiTabbarMove');
    assert.equal(body.includes('onEnterTabbar='), false, 'App 不应再传 onEnterTabbar prop');
    assert.equal(body.includes('onTabbarMove='), false, 'App 不应再传 onTabbarMove prop');
    assert.equal(body.includes('focusTabbar'), false, 'App 不应再调 focusTabbar');
    assert.equal(body.includes('kbNav'), false, 'App 不应再调 kbNav');
  });

  it('TabNavigation 不再暴露 focusTabbar/kbNav 死接口', async () => {
    const body = await readSource('../TabNavigation.jsx');
    assert.equal(body.includes('focusTabbar'), false, 'TabNavigation 不应再有 focusTabbar');
    assert.equal(body.includes('handleKbNav'), false, 'TabNavigation 不应再有 handleKbNav');
    assert.equal(body.includes('kbNav'), false, 'useImperativeHandle 不应再暴露 kbNav');
  });
});

describe('App 转发契约源码护栏', () => {
  it('App.jsx 含 dispatchEmojiNav 与四向 action', async () => {
    const body = await readSource('../../App.jsx');
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
    const body = await readSource('../EmojiTab.jsx');
    assert.ok(body.includes('handleNavAction'), '缺 handleNavAction');
    assert.ok(body.includes('resolveZoneNav'), '缺 resolveZoneNav');
    // 禁止再挂裸 Arrow 主路径(会与热键双触发)
    assert.equal(body.includes("addEventListener('keydown'"), false, '不应再 window keydown 吃方向键');
    assert.ok(body.includes('resolveSidebarCategoryId'), '进侧栏应保留当前分类');
    assert.ok(body.includes('isSidebarCategoryActive'), '侧栏应受控高亮');
  });

  it('ImageLibraryTab activateKb 只激活可执行图片并同步写 ref', async () => {
    const body = await readSource('ImageLibraryTab.jsx');
    assert.ok(body.includes('resolveActivateKb'), 'activateKb 必须用 resolveActivateKb');
    assert.ok(body.includes('getKeyboardItemIndexes'), 'activateKb 应复用可执行图片索引 helper');
    assert.ok(body.includes('const targetIndex = indexes[result.index]'), 'activateKb 应把逻辑位置映射回图片原始索引');
    assert.ok(body.includes('kbImageIndexRef.current = targetIndex'), '必须同步写 ref');
    assert.ok(body.includes('kbImageIndexRef.current = -1'), 'resetKbIndex 必须同步清 ref');
  });
});

// F5 图片网格边界现在由 EmojiTab 统一交给 App 切换顶层标签。
describe('F5 图片网格边界契约', () => {
  it('EmojiTab 网格边界分支调用相邻顶层切换方向', async () => {
    const body = await readSource('../EmojiTab.jsx');
    const start = body.indexOf("if (!ok && intent.onFail === 'switch-tab-left')");
    const end = body.indexOf("case 'sidebar-move':", start);
    assert.notEqual(start, -1, '缺少左边界顶层切换分支');
    assert.notEqual(end, -1, '缺少网格意图结束锚点');
    const fn = body.slice(start, end);
    assert.ok(fn.includes("onSwitchTab?.('previous')"), '左边界应请求上一个顶层标签');
    assert.ok(fn.includes("onSwitchTab?.('next')"), '右边界应请求下一个顶层标签');
  });


  it('旧 gridHome 回行首死链已删除', async () => {
    const emojiBody = await readSource('../EmojiTab.jsx');
    const imageBody = await readSource('ImageLibraryTab.jsx');
    assert.equal(emojiBody.includes('const gridHome = useCallback'), false, 'EmojiTab 不应再保留 gridHome');
    assert.equal(imageBody.includes('goHome:'), false, 'ImageLibraryTab 不应再暴露无调用方的 goHome');
  });
});

// F9: resolveTabbarMove 生产 0 调用(仅测试用),TabNavigation handleKbNav 已内联
// (idx + delta + items.length) % items.length。未知 current 时 resolveTabbarMove 未知
// fallback idx=0(恒从 emoji 开始),与 cycleValue delta-aware fallback 分叉。删死代码
// 函数 + 测试 describe 块 + 文件 import 一行。
// F4 后续:handleKbNav 整链已随 tabbar 死码删除,内联公式断言一并移除(无对象可断言)。
describe('F9 resolveTabbarMove 已删(死导出清理)', () => {
  it('emojiKbNavigation.js 不导出 resolveTabbarMove', async () => {
    const body = await readSource('emojiKbNavigation.js');
    assert.equal(
      body.includes('resolveTabbarMove'),
      false,
      'emojiKbNavigation.js 不应再导出 resolveTabbarMove,生产 0 调用'
    );
  });

  it('emojiKbNavigation.test.js 不再 import/describe resolveTabbarMove', async () => {
    const src = await readSourceRaw('emojiKbNavigation.test.js');
    // 只看静态 import 语句行(浅扫 import { ... } from 'emojiKbNavigation.js')
    const importLineMatch = src.match(/import\s*\{[\s\S]*?\}\s*from\s*['"`]\.\/emojiKbNavigation/);
    assert.ok(importLineMatch, '应能找到 import { ... } from emojiKbNavigation 行');
    assert.equal(
      importLineMatch[0].includes('resolveTabbarMove'),
      false,
      '测试文件 import 不应再含 resolveTabbarMove'
    );
    assert.equal(
      /describe\(\s*['"`]resolveTabbarMove['"`]\s*,/.test(src),
      false,
      '不应再 describe("resolveTabbarMove"),description 文本除外'
    );
  });
});
