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


// C09: EmojiTab prev-mode 硬编码 ['emoji','symbols','images'],与 App 过滤路径/TabNavigation 常量脱钩。
// 修复:import cycleValue + EMOJI_MODE_IDS,cycleValue(EMOJI_MODE_IDS, ...) 复用单一真源。
test('C09 EmojiTab prev-mode 复用 EMOJI_MODE_IDS 与 cycleValue,不再硬编码子模式数组', async () => {
  const body = await readSource('../EmojiTab.jsx');
  const raw = await (async () => {
    const fs = await import('node:fs/promises');
    const path = await import('node:path');
    const { fileURLToPath } = await import('node:url');
    const here = path.dirname(fileURLToPath(import.meta.url));
    return fs.readFile(path.join(here, '../EmojiTab.jsx'), 'utf8');
  })();
  // import 必须有 cycleValue 与 EMOJI_MODE_IDS(raw 保留 import 行,body 已剥注释)
  assert.ok(
    /import\s*\{[^}]*\bcycleValue\b[^}]*\}\s*from\s*['"]\.\/emoji\/emojiKbNavigation['"]/.test(raw)
      || /import\s*\{[^}]*\bcycleValue\b/.test(raw),
    'EmojiTab 必须 import cycleValue'
  );
  assert.ok(
    /EMOJI_MODE_IDS/.test(raw) && /from\s*['"].*TabNavigation['"]/.test(raw),
    'EmojiTab 必须从 TabNavigation import EMOJI_MODE_IDS'
  );
  // prev-mode 分支必须 cycleValue(EMOJI_MODE_IDS, ...)
  const prevStart = body.indexOf("case 'prev-mode'");
  assert.ok(prevStart >= 0, '应有 prev-mode 分支');
  const prevEnd = body.indexOf("case 'deactivate'", prevStart);
  const prev = body.slice(prevStart, prevEnd > 0 ? prevEnd : prevStart + 800);
  assert.ok(
    /cycleValue\(\s*EMOJI_MODE_IDS\s*,/.test(prev),
    'prev-mode 必须 cycleValue(EMOJI_MODE_IDS, ...)'
  );
  assert.equal(
    prev.includes("'emoji', 'symbols', 'images'"),
    false,
    'prev-mode 不应再硬编码 emoji 子模式数组'
  );
});


// C12: applyNavIntent 闭包读 emojiMode,deps 缺 emojiMode → stale 子模式决策。
test('C12 applyNavIntent useCallback deps 必须含 emojiMode', async () => {
  const body = await readSource('../EmojiTab.jsx');
  const start = body.indexOf('const applyNavIntent = useCallback');
  assert.ok(start >= 0, '应有 applyNavIntent');
  // deps 数组在 useCallback 第二个参数
  const depsStart = body.indexOf('}, [', start);
  assert.ok(depsStart > start, 'applyNavIntent 应有 deps 数组');
  const depsEnd = body.indexOf(']);', depsStart);
  const deps = body.slice(depsStart, depsEnd + 3);
  assert.ok(
    /\bemojiMode\b/.test(deps),
    'applyNavIntent deps 必须含 emojiMode,否则 prev-mode 读到 stale 值'
  );
});
