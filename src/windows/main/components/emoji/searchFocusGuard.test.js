import { test } from 'node:test';
import assert from 'node:assert/strict';

// F1-1: 顶栏搜索框聚焦时全局热键全部变为输入(commit 3a6f67c5 只给 7 个 handler
// 加了 isSearchFocused 守卫),Esc(hide-window)/Ctrl+P(toggle-pin)/Ctrl+↑↓
// (previous/next-group)仍会打断输入。护栏:这些 handler 必须带同样的守卫。

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

test('F1-1 App handleTogglePin 有 isSearchFocused 守卫', async () => {
  const body = await readAppBody();
  const fnStart = body.indexOf('const handleTogglePin');
  const fnEnd = body.indexOf('const handlePreviousGroup', fnStart);
  assert.notEqual(fnStart, -1, '缺 const handleTogglePin 声明');
  assert.notEqual(fnEnd, -1, '缺 const handlePreviousGroup 锚点');
  const fn = body.slice(fnStart, fnEnd);
  assert.ok(fn.includes('isSearchFocused'), 'handleTogglePin 应检查 isSearchFocused');
});

test('F1-1 App handlePreviousGroup 有 isSearchFocused 守卫', async () => {
  const body = await readAppBody();
  const fnStart = body.indexOf('const handlePreviousGroup');
  const fnEnd = body.indexOf('const handleNextGroup', fnStart);
  assert.notEqual(fnStart, -1, '缺 const handlePreviousGroup 声明');
  assert.notEqual(fnEnd, -1, '缺 const handleNextGroup 锚点');
  const fn = body.slice(fnStart, fnEnd);
  assert.ok(fn.includes('isSearchFocused'), 'handlePreviousGroup 应检查 isSearchFocused');
});

test('F1-1 App handleNextGroup 有 isSearchFocused 守卫', async () => {
  const body = await readAppBody();
  const fnStart = body.indexOf('const handleNextGroup');
  const fnEnd = body.indexOf('// 设置全局键盘导航', fnStart);
  assert.notEqual(fnStart, -1, '缺 const handleNextGroup 声明');
  const fn = body.slice(fnStart, fnEnd);
  assert.ok(fn.includes('isSearchFocused'), 'handleNextGroup 应检查 isSearchFocused');
});

test('F1-1 App 向 useNavigationKeyboard 传带守卫的 onHideWindow', async () => {
  const body = await readAppBody();
  const hookStart = body.indexOf('useNavigationKeyboard({');
  const hookEnd = body.indexOf('enabled: true', hookStart);
  assert.notEqual(hookStart, -1, '缺 useNavigationKeyboard({ 调用');
  const hook = body.slice(hookStart, hookEnd);
  assert.ok(hook.includes('onHideWindow'), 'useNavigationKeyboard 应传 onHideWindow 回调');
});

test('F1-1 useNavigationKeyboard hide-window 走回调而非直接 hideMainWindow', async () => {
  const fs = await import('node:fs/promises');
  const path = await import('node:path');
  const { fileURLToPath } = await import('node:url');
  const here = path.dirname(fileURLToPath(import.meta.url));
  const src = await fs.readFile(path.join(here, '../../../../shared/hooks/useNavigationKeyboard.js'), 'utf8');
  const body = src
    .split('\n')
    .filter((l) => !l.trimStart().startsWith('//'))
    .join('\n');
  const caseStart = body.indexOf("case 'hide-window':");
  const caseEnd = body.indexOf('case \'toggle-pin\'', caseStart);
  assert.notEqual(caseStart, -1, '缺 hide-window case');
  const caseBody = body.slice(caseStart, caseEnd);
  assert.ok(caseBody.includes('onHideWindow'), 'hide-window 应调 handlers.onHideWindow');
  assert.equal(caseBody.includes('hideMainWindow()'), false, 'hide-window 不应直接调 hideMainWindow');
});
