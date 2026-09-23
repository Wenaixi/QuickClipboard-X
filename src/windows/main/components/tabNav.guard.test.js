import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const tabNav = readFileSync(join(here, './TabNavigation.jsx'), 'utf8');
const app = readFileSync(join(here, '../App.jsx'), 'utf8');
// 剥行注释,避免注释字面误命中(与 Rust 侧 §10.4 陷阱同构)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');

test('TabNavigation 不得残留未声明的 compactFilters 引用(merge 咬掉参数却保留引用,白屏根因)', () => {
  const code = strip(tabNav);
  assert.doesNotMatch(code, /isCompactFiltersLayout/, 'compactFilters 无参数、无使用点,死引用必须删除');
});

test('TabNavigation 必须声明 filterCollapseTimerRef(折叠定时器是本地过滤折叠交互的必需 ref)', () => {
  const code = strip(tabNav);
  assert.match(code, /const filterCollapseTimerRef = useRef\(null\)/, 'merge 咬掉 useRef 声明但保留 6 处引用');
});

test('TabNavigation 必须定义 handleFilterAreaMouseLeave(mouseleave 折叠回调,handleControlsContainerMouseLeave 仍调用)', () => {
  const code = strip(tabNav);
  assert.match(code, /const handleFilterAreaMouseLeave = \(event\) =>/, 'merge 咬掉定义但调用点保留');
});

test('TabNavigation 必须定义粘贴状态过滤(pasteFilters 数组 + 两个选中集合 + 两个 handler)', () => {
  const code = strip(tabNav);
  assert.match(code, /const pasteFilters = \[/, '上游 v0.5 新增的未粘贴/已粘贴过滤数组必须接入');
  assert.match(code, /id: 'unpasted'/, '未粘贴过滤项');
  assert.match(code, /id: 'pasted'/, '已粘贴过滤项');
  assert.match(code, /const selectedFilters = /, '普通类型过滤选中集合');
  assert.match(code, /const isFilterSelected = /, '普通类型选中判断');
  assert.match(code, /const selectedPasteFilters = /, '粘贴状态选中集合');
  assert.match(code, /const isPasteFilterSelected = /, '粘贴状态选中判断');
  assert.match(code, /const handleFilterChange = /, '普通类型切换 handler');
  assert.match(code, /const handlePasteFilterChange = /, '粘贴状态切换 handler');
});

test('TabNavigation sidebar 分支必须渲染合并的 filters+pasteFilters(否则粘贴过滤按钮全部缺失)', () => {
  const code = strip(tabNav);
  assert.match(code, /\[\.\.\.filters, \.\.\.pasteFilters\]\.map/, 'sidebar 必须合并渲染两类过滤器');
});

test('TabNavigation 水平导航过滤按钮必须用多选感知的 isFilterSelected(否则多选时高亮全部消失)', () => {
  const code = strip(tabNav);
  const activeCount = (code.match(/isActive=\{isFilterSelected\(filter\.id\)\}/g) || []).length;
  assert.ok(activeCount >= 3, '水平分支三处(展开/收起/溢出)过滤按钮必须全部用多选感知判断');
  assert.doesNotMatch(code, /isActive=\{contentFilter === filter\.id\}/, '不允许残留全等比较(多选时活动指示失效)');
});

test('App.jsx 必须接线粘贴状态过滤全链路(否则 TabNavigation 切换的过滤不会生效)', () => {
  const code = strip(app);
  assert.match(code, /pasteFilter=\{pasteFilter\}/, 'TabNavigation 必须收到 pasteFilter');
  assert.match(code, /onPasteFilterChange=\{setPasteFilter\}/, 'TabNavigation 必须收到 onPasteFilterChange');
  assert.match(code, /<ClipboardTab[^>]*pasteFilter=\{pasteFilter\}/, 'ClipboardTab 必须收到 pasteFilter(消费端)');
  assert.match(code, /<FavoritesTab[^>]*pasteFilter=\{pasteFilter\}/, 'FavoritesTab 必须收到 pasteFilter(消费端)');
});

test('App.jsx 不得残留 isCompactFilters 死链路(merge 咬掉消费方后已是无人接线的死状态)', () => {
  const code = strip(app);
  assert.doesNotMatch(code, /isCompactFilters/, 'isCompactFilters state/mediaQuery/监听必须整体删除');
});

test('粘贴状态过滤 label 必须走语言包(未粘贴/已粘贴 en 用户可见,不得裸中文)', () => {
  const code = strip(tabNav);
  assert.match(code, /label: t\('filter\.unpasted'\)/, '未粘贴 label 必须走 filter.unpasted 键');
  assert.match(code, /label: t\('filter\.pasted'\)/, '已粘贴 label 必须走 filter.pasted 键');
});

test('TabNavigation 侧边栏收起/展开/分组文案必须走语言包', () => {
  const code = strip(tabNav);
  assert.match(code, /t\('tabNav\.collapseSidebar'\)/, '收起侧边栏 tooltip 必须走 tabNav.collapseSidebar');
  assert.match(code, /t\('tabNav\.expandSidebar'\)/, '展开侧边栏 tooltip 必须走 tabNav.expandSidebar');
  assert.match(code, /t\('tabNav\.groups'\)/, '分组 tooltip/文本必须走 tabNav.groups');
  // 该文件不得残留裸"收起/展开"tooltip 字面(侧边栏收起按钮文案)。
  assert.doesNotMatch(code, /content=\{sidebarShowLabel \? '收起侧边栏'/, '收起侧边栏 tooltip 不得裸中文');
});

test('双语包必须含 tabNav 收起/展开/分组键与 filter 粘贴状态键', () => {
  const zh = JSON.parse(readFileSync(join(here, '../../shared/locales/zh-CN.json'), 'utf8'));
  const en = JSON.parse(readFileSync(join(here, '../../shared/locales/en-US.json'), 'utf8'));
  for (const [pack, label] of [[zh, 'zh'], [en, 'en']]) {
    for (const key of ['collapseSidebar', 'expandSidebar', 'groups']) {
      assert.ok(
        pack.tabNav && typeof pack.tabNav[key] === 'string' && pack.tabNav[key].length > 0,
        `${label} tabNav.${key} 必须存在且非空`,
      );
    }
    assert.ok(pack.filter && typeof pack.filter.unpasted === 'string', `${label} filter.unpasted 必须存在`);
    assert.ok(pack.filter && typeof pack.filter.pasted === 'string', `${label} filter.pasted 必须存在`);
  }
  // en 值必须是英文(界面文案,非数据)。
  assert.strictEqual(en.tabNav.groups, 'Groups');
  assert.strictEqual(en.filter.pasted, 'Pasted');
});