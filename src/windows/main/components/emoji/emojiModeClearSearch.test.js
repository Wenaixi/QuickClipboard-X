import { test } from 'node:test';
import assert from 'node:assert/strict';

// F1-3: 切子模式(emoji/symbols/images)后搜索态残留。
// 旧版 main 的 emojiMode effect 第一行 setSearchQuery(''),commit 80acf679
// 把搜索框合并到顶栏后删除,无重建。现在 searchQuery 是 App 顶层 state,
// 切子模式仍按旧关键词过滤 → 无匹配时渲染 no-results 空网格。
// 修复:App 在 emojiMode 变化的回调里清空搜索(搜索框归 App 所有)。

async function readAppBody() {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../../App.jsx'), 'utf8');
  return src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
}

test('F1-3 App 有 handleEmojiModeChange 且在模式变化时清空 searchQuery', async () => {
  const body = await readAppBody();
  const fnStart = body.indexOf('const handleEmojiModeChange');
  assert.notEqual(fnStart, -1, '缺 const handleEmojiModeChange');
  const fnEnd = body.indexOf('const handleFilterLeft', fnStart);
  const fn = body.slice(fnStart, fnEnd);
  assert.ok(fn.includes('setSearchQuery(\'\')'), 'handleEmojiModeChange 应清空搜索');
  assert.ok(fn.includes('setEmojiMode'), 'handleEmojiModeChange 应转发 setEmojiMode');
});

test('F1-3 App 用 handleEmojiModeChange 取代全部裸 setEmojiMode', async () => {
  const body = await readAppBody();
  // TabNavigation / EmojiTab 拿到的是带清搜索的包装回调
  const tabNavProps = body.slice(body.indexOf('const TabNavigationComponent'), body.indexOf('const ContentComponent'));
  assert.ok(tabNavProps.includes('onEmojiModeChange={handleEmojiModeChange}'), 'TabNavigation 应接 handleEmojiModeChange');
  const contentProps = body.slice(body.indexOf('const ContentComponent'), body.indexOf('const ActionBarComponent'));
  assert.ok(contentProps.includes('onEmojiModeChange={handleEmojiModeChange}'), 'EmojiTab 应接 handleEmojiModeChange');
  // handler 内部转发 setEmojiMode 属预期;其余裸调用点必须走包装回调
  const filterSection = body.slice(body.indexOf('const handleFilterLeft'), body.indexOf('const handleToggleSearch'));
  assert.ok(filterSection.includes('handleEmojiModeChange('), '过滤热键切子模式应走 handleEmojiModeChange');
});
