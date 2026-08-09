import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readSource } from './readSource.js';

// F02: App.handleToggleSearch 调 searchRef.current.toggleFocus()(async)后
// 立刻同步读 searchRef.current.isFocused(),microtask 未 flush 必拿旧值
// (TitleBarSearch.jsx:107 toggleFocus 是 async,内含 await focusWindowImmediately
// 后才调 inputRef.focus()/select())。正确路径:不写 setIsSearchFocused 直接读,
// 由 TitleBarSearch 的 onFocus/onBlur → notifyFocusChange → onSearchFocusChange
// → setIsSearchFocused 走 single-write。
// 护栏:handleToggleSearch 函数体内不得出现同步读 ref.isFocused 的字面。

test('F02 handleToggleSearch 不直接同步读 searchRef.isFocused', async () => {
  const body = await readSource('../../App.jsx');
  const fnStart = body.indexOf('const handleToggleSearch');
  const fnEnd = body.indexOf('const handleEmojiModeChange', fnStart);
  assert.notEqual(fnStart, -1, '缺 const handleToggleSearch 声明');
  assert.notEqual(fnEnd, -1, '缺 const handleEmojiModeChange 锚点');
  const fn = body.slice(fnStart, fnEnd);
  assert.equal(
    fn.includes('isFocused?.()') || fn.includes('isFocused()'),
    false,
    'handleToggleSearch 不应同步读 ref.isFocused (async toggleFocus 后必拿旧值);交由 onSearchFocusChange 单写'
  );
});

test('F02 handleToggleSearch 仍调用 toggleFocus (行为存在)', async () => {
  const body = await readSource('../../App.jsx');
  const fnStart = body.indexOf('const handleToggleSearch');
  const fnEnd = body.indexOf('const handleEmojiModeChange', fnStart);
  const fn = body.slice(fnStart, fnEnd);
  assert.ok(
    fn.includes('searchRef.current.toggleFocus'),
    'handleToggleSearch 应保留 searchRef.current.toggleFocus 调用(行为本身正确)'
  );
});