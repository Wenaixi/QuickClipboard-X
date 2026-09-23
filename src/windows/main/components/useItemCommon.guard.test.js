// 时间戳文案语言包护栏：useItemCommon.formatTime 输出的「今天/昨天/周X/
// 个文件」必须经 i18n 注入渲染,剪贴板与收藏列表每行时间戳 en 用户 100%
// 暴露,裸中文会让英文界面混入中文。对应的双语键必须双端齐全。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const hookPath = join(here, '../../../shared/hooks/useItemCommon.jsx');
const hook = readFileSync(hookPath, 'utf8');
const bareHook = hook
  .split('\n')
  .filter((line) => !line.trimStart().startsWith('//'))
  .join('\n');

const clipboardItemSrc = readFileSync(join(here, './ClipboardItem.jsx'), 'utf8');
const favoriteItemSrc = readFileSync(join(here, './FavoriteItem.jsx'), 'utf8');
const zh = JSON.parse(readFileSync(join(here, '../../../shared/locales/zh-CN.json'), 'utf8'));
const en = JSON.parse(readFileSync(join(here, '../../../shared/locales/en-US.json'), 'utf8'));

test('formatTime 必须注入 t 渲染时间文案(截断锚点环绕 const t 判断)', () => {
  const tPos = bareHook.indexOf("typeof options.t === 'function' ? options.t : (key) => ''");
  assert.ok(tPos >= 0, 'formatTime 必须经 options.t 注入才用语言包');
  const todayPos = bareHook.indexOf("t('common.today')");
  assert.ok(todayPos >= 0, '必须经 t 读 today 键');
  assert.ok(todayPos > tPos, 't 必须先注入再调用(截断锚点)');
});

test('时间模板不得直接内联中文(今天/昨天/个文件裸写不得出现)', () => {
  const templateBody = hook.slice(
    hook.indexOf('const formatTime = () => {'),
    hook.indexOf('// 渲染内容组件'),
  );
  // 剥注释后:裸"今天"仅允许出现在回退分支(|| '今天'),不得作为唯一文案。
  for (const literal of ['今天', '昨天', '个文件']) {
    const stripped = templateBody
      .split('\n')
      .filter((line) => !line.trimStart().startsWith('//'))
      .join('\n');
    const occurrences = stripped.split(literal).length - 1;
    assert.ok(
      occurrences <= 2,
      `formatTime 内裸中文「${literal}」至多 2 处(注入失败回退),实际 ${occurrences}`,
    );
  }
});

test('双语包必须含今天/昨天/周X/个文件/error 键', () => {
  const zhKeys = ['today', 'yesterday', 'weekdaySunday', 'weekdayMonday', 'weekdayTuesday', 'weekdayWednesday', 'weekdayThursday', 'weekdayFriday', 'weekdaySaturday', 'fileCount', 'error'];
  for (const key of zhKeys) {
    assert.ok(
      zh.common && typeof zh.common[key] === 'string' && zh.common[key].length > 0,
      `zh common.${key} 必须存在且非空`,
    );
    assert.ok(
      en.common && typeof en.common[key] === 'string' && en.common[key].length > 0,
      `en common.${key} 必须存在且非空`,
    );
  }
  // en 值必须为英文(时间文案不是数据)。
  assert.strictEqual(en.common.today, 'Today');
  assert.strictEqual(en.common.yesterday, 'Yesterday');
});

test('ClipboardItem/FavoriteItem 必须把 t 传给 useItemCommon', () => {
  assert.match(clipboardItemSrc, /useItemCommon\(item, \{ t \}\)/, 'ClipboardItem 必须注入 t');
  assert.match(favoriteItemSrc, /useItemCommon\(item, \{ isFavorite: true, t \}\)/, 'FavoriteItem 必须注入 t');
});