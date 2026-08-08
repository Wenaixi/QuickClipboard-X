import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readSource } from './readSource.js';

// F1-5: App handleFilterLeft/Right 硬编码过滤器数组,与 TabNavigation 常量重复。
// 修复:从 TabNavigation 导出 FILTER_IDS / EMOJI_MODE_IDS,App import 复用。

test('F1-5 TabNavigation 导出 FILTER_IDS/EMOJI_MODE_IDS 常量', async () => {
  const body = await readSource('../TabNavigation.jsx');
  assert.ok(/export const FILTER_IDS/.test(body), '应导出 FILTER_IDS');
  assert.ok(/export const EMOJI_MODE_IDS/.test(body), '应导出 EMOJI_MODE_IDS');
});

test('F1-5 App 过滤热键复用常量,不再硬编码数组', async () => {
  const body = await readSource('../../App.jsx');
  const filterSection = body.slice(body.indexOf('const handleFilterLeft'), body.indexOf('const handleToggleSearch'));
  assert.ok(/cycleValue\(FILTER_IDS/.test(filterSection), 'contentFilter 切换应复用 FILTER_IDS');
  assert.ok(/cycleValue\(EMOJI_MODE_IDS/.test(filterSection), 'emoji 子模式切换应复用 EMOJI_MODE_IDS');
  // 硬编码数组不应再出现在过滤热键路径
  assert.equal(
    filterSection.includes("'all', 'text', 'image', 'file', 'link'"),
    false,
    '不应再硬编码过滤器数组'
  );
  assert.equal(
    filterSection.includes("'emoji', 'symbols', 'images'"),
    false,
    '不应再硬编码 emoji 子模式数组'
  );
});
