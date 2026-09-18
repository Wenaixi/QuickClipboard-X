import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const sectionPath = join(here, '../../../windows/settings/sections/SoundSection.jsx');
const source = readFileSync(sectionPath, 'utf8');

// 剥行注释,避免注释字面误命中(§10.4 陷阱)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');

const body = strip(source);

test('音量滑杆 0 值必须保留:0 是合法静音值,不得被 || 兜底吞掉', () => {
  // 正断言:音量字段必须用空值合并(??)而非逻辑或(||),0 才能传到后端
  assert.match(body, /settings\.soundVolume \?\? 50/, '音量滑杆 value 必须用 ?? 保留 0 值');
  // 负断言:逻辑或 || 会把 0 吞成 50,静音设置永远无法保存
  assert.doesNotMatch(body, /settings\.soundVolume \|\| 50/, '音量滑杆不得用 || 兜底(0 被误吞)');
});
