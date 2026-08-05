import { test } from 'node:test';
import assert from 'node:assert/strict';

// App 守卫收敛护栏(standards #3/#4)
// background: handleTabLeft/Right 的 emojiKbActive 前置守卫与
// dispatchEmojiNav 内部判断重复,同一条件检查两遍。删守卫统一走 dispatch。

test('App handleTabLeft/Right 不再有 emojiKbActive 前置守卫', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../../App.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  // handleTabLeft/Right 体内不得再出现 emojiKbActive 守卫
  const tabLeft = body.slice(body.indexOf('const handleTabLeft'), body.indexOf('const handleTabRight'));
  const tabRight = body.slice(body.indexOf('const handleTabRight'), body.indexOf('const handleNavigateUp'));
  assert.equal(tabLeft.includes('emojiKbActive'), false, 'handleTabLeft 不应再检查 emojiKbActive');
  assert.equal(tabRight.includes('emojiKbActive'), false, 'handleTabRight 不应再检查 emojiKbActive');
  // 统一走 dispatchEmojiNav
  assert.ok(tabLeft.includes("dispatchEmojiNav('tab-left')"), 'handleTabLeft 应走 dispatchEmojiNav');
  assert.ok(tabRight.includes("dispatchEmojiNav('tab-right')"), 'handleTabRight 应走 dispatchEmojiNav');
});

test('App dispatchEmojiNav 内保留 emojiKbActive 门控决策', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../../App.jsx'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  const dispatch = body.slice(body.indexOf('const dispatchEmojiNav'), body.indexOf('// EmojiTab 请求'));
  assert.ok(dispatch.includes('shouldForwardNavToEmoji'), 'dispatchEmojiNav 应保留转发决策');
  assert.ok(dispatch.includes('resolveOutsideAppAction'), 'dispatchEmojiNav 应保留 outside 决策');
});
