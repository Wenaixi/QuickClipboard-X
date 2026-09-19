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
