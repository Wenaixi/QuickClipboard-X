import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const sectionPath = join(here, '../../../windows/settings/sections/ClipboardSection.jsx');
const source = readFileSync(sectionPath, 'utf8');

// 剥行注释,避免注释字面误命中(§10.4 陷阱)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');

const body = strip(source);

test('贴边隐藏偏移清空输入必须保留现有值,不得把 NaN 写进设置', () => {
  // 正断言:清空输入(parseInt('') → NaN)必须被 Number.isFinite 拦截,
  // 非法值回退为现有设置,NaN 不进入 onSettingChange
  assert.match(
    body,
    /Number\.isFinite\(next\).*settings\.edgeHideOffset/,
    '清空输入必须保留现有设置(Number.isFinite 拦截 NaN)',
  );
  // 负断言:禁止裸 parseInt(e.target.value) 直接作为设置值(空串 → NaN)
  assert.doesNotMatch(
    body,
    /onSettingChange\('edgeHideOffset', parseInt\(e\.target\.value\)\)/,
    '禁止把 parseInt 结果直存(空串产生 NaN)',
  );
});
