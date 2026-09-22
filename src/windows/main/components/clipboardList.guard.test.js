import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const clipboardList = readFileSync(join(here, './ClipboardList.jsx'), 'utf8');
const favoritesList = readFileSync(join(here, './FavoritesList.jsx'), 'utf8');
// 剥行注释,避免注释字面误命中(与 Rust 侧 §10.4 陷阱同构)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');

test('ClipboardList 数字快捷键角标在粘贴状态过滤视图下必须隐藏(与搜索/内容类型同处理,否则角标暗示可直达但数字快捷键按全量前 9 处取数,联动错位)', () => {
  const code = strip(clipboardList);
  assert.match(
    code,
    /showShortcut = settings\.showListShortcuts !== false && !clipSnap\.filter && clipSnap\.contentType === 'all' && clipSnap\.pasteStatus === 'all';/,
    'showShortcut 判定必须同时排除 filter/contentType/pasteStatus 三个过滤维度,任一过滤激活都不显示数字角标'
  );
});

test('ClipboardList loadSelectionEntries 必须透传 pasteStatus(否则过滤视图下 Shift 范围多选按全量序拉回,错选非可视条目)', () => {
  const code = strip(clipboardList);
  assert.match(
    code,
    /pasteStatus: clipSnap\.pasteStatus !== 'all' \? clipSnap\.pasteStatus : undefined/,
    '请求参数必须把粘贴状态过滤透传给 getClipboardHistory——后端无 paste_status 时不过滤,全量序与过滤视图索引错位'
  );
  assert.match(
    code,
    /}, \[clipSnap\.contentType, clipSnap\.filter, clipSnap\.pasteStatus\]\);/,
    'useCallback deps 必须含 clipSnap.pasteStatus,否则过滤切换后仍按旧参数拉取'
  );
});

test('FavoritesList loadSelectionEntries 必须透传 pasteStatus(与剪贴板同源缺陷,收藏过滤视图下错选)', () => {
  const code = strip(favoritesList);
  assert.match(
    code,
    /pasteStatus: favSnap\.pasteStatus !== 'all' \? favSnap\.pasteStatus : undefined/,
    '请求参数必须把粘贴状态过滤透传给 getFavoritesHistory——后端无 paste_status 时不过滤,全量序与过滤视图索引错位'
  );
  assert.match(
    code,
    /}, \[favSnap\.contentType, favSnap\.filter, groupsSnap\.currentGroup, favSnap\.pasteStatus\]\);/,
    'useCallback deps 必须含 favSnap.pasteStatus,否则过滤切换后仍按旧参数拉取'
  );
});

test('粘贴计数事件在过滤视图下必须实时剔除条目(剪贴板/收藏双域同构,否则「未粘贴/已粘贴」视图滞留需切过滤或重启才恢复)', () => {
  const clipStore = readFileSync(join(here, '../../../shared/store/clipboardStore.js'), 'utf8');
  const favStore = readFileSync(join(here, '../../../shared/store/favoritesStore.js'), 'utf8');
  const clipBare = strip(clipStore);
  const favBare = strip(favStore);
  // 剪贴板:监听必须先判过滤激活,过滤激活时调剔除并短路,不重复 +1。
  assert.match(
    clipBare,
    /listen\('paste-count-updated', \(event\) => \{/,
    '剪贴板必须监听 paste-count-updated'
  );
  assert.match(
    clipBare,
    /if \(clipboardStore\.pasteStatus === 'unpasted'\) \{/,
    '剪贴板监听必须仅在「未粘贴」视图激活时剔除(已粘贴视图 N→N+1 仍匹配,不得剔除)'
  );
  assert.match(
    clipBare,
    /clipboardStore\.removePastedItemIfFiltered\(id\)/,
    '剪贴板监听未粘贴视图激活时必须调用剔除'
  );
  assert.ok(
    clipBare.includes('removePastedItemIfFiltered(id)') && clipBare.includes('if (removed) return'),
    '剔除成功必须短路,避免未粘贴视图下残留 +1'
  );
  assert.match(
    clipBare,
    /removePastedItemIfFiltered\(id\) \{\s*\n\s*if \(this\.pasteStatus !== 'unpasted'\) return false[\s\S]*?this\.removeItem\(id\)[\s\S]*?return true/,
    '剪贴板剔除方法必须:非「未粘贴」视图不剔除、未粘贴视图按 id 移除条目并返回成功'
  );
  // 收藏:同构断言。
  assert.match(
    favBare,
    /listen\('favorite-paste-count-updated', \(event\) => \{/,
    '收藏必须监听 favorite-paste-count-updated'
  );
  assert.match(
    favBare,
    /if \(favoritesStore\.pasteStatus === 'unpasted'\) \{/,
    '收藏监听必须仅在「未粘贴」视图激活时剔除(已粘贴视图 N→N+1 仍匹配,不得剔除)'
  );
  assert.match(
    favBare,
    /favoritesStore\.removePastedItemIfFiltered\(id\)/,
    '收藏监听未粘贴视图激活时必须调用剔除'
  );
  assert.ok(
    favBare.includes('removePastedItemIfFiltered(id)') && favBare.includes('if (removed) return'),
    '收藏剔除成功必须短路'
  );
  assert.match(
    favBare,
    /removePastedItemIfFiltered\(id\) \{\s*\n\s*if \(this\.pasteStatus !== 'unpasted'\) return false[\s\S]*?this\.removeItem\(id\)[\s\S]*?return true/,
    '收藏剔除方法必须与剪贴板同构'
  );
});

test('ClipboardList 加载失败必须渲染错误态与重试(不得静默伪装空历史)', () => {
  // B-候选3:store 层 catch 只写 error 字段,UI 空态只判 totalCount===0,
  // 用户无法区分真实空与加载失败,无重试入口。必须:空态分支内先判 error
  // 渲染错误提示 + 重试按钮(点击调 initClipboardItems 重新加载)。
  const code = strip(clipboardList);
  assert.match(
    code,
    /if \(clipSnap\.error\) \{/,
    '空态分支必须先判加载错误'
  );
  assert.match(
    code,
    /clipboardList\.loadFailed/,
    '错误态必须渲染加载失败文案(走语言包)'
  );
  assert.match(
    code,
    /onClick=\{\(\) => initClipboardItems\(\)\}/,
    '错误态必须提供重试按钮并触发重新加载'
  );
  assert.match(
    code,
    /clipboardList\.retry/,
    '重试按钮必须走语言包键'
  );
  assert.ok(
    code.includes('initClipboardItems') && code.includes('@shared/store/clipboardStore'),
    'ClipboardList 必须引入 initClipboardItems(重试依赖)'
  );
});
