import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readSource } from './readSource.js';

// App 守卫收敛护栏(standards #3/#4)
// background: handleTabLeft/Right 的 emojiKbActive 前置守卫与
// dispatchEmojiNav 内部判断重复,同一条件检查两遍。删守卫统一走 dispatch。

test('App handleTabLeft/Right 不再有 emojiKbActive 前置守卫', async () => {
  const body = await readSource('../../App.jsx');
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
  const body = await readSource('../../App.jsx');
  // 边界锚点改用函数名 + 下一个 const 声明:注释锚点 '// EmojiTab 请求' 会被剥行注释
  // 提前去掉,indexOf 恒 -1,slice 会一直切到 EOF,断言作用域漂移有假绿窗口。
  // F4 删 tabbar 死码后 handleEmojiEnterTabbar 已不存在,改用下一个 const 声明锚点。
  const dispatchStart = body.indexOf('const dispatchEmojiNav');
  const dispatchEnd = body.indexOf('const handleEmojiSwitchTab', dispatchStart);
  assert.notEqual(dispatchStart, -1, '缺 const dispatchEmojiNav 声明');
  assert.notEqual(dispatchEnd, -1, '缺 const handleEmojiSwitchTab 锚点');
  const dispatch = body.slice(dispatchStart, dispatchEnd);
  assert.ok(dispatch.includes('shouldForwardNavToEmoji'), 'dispatchEmojiNav 应保留转发决策');
  assert.ok(dispatch.includes('resolveOutsideAppAction'), 'dispatchEmojiNav 应保留 outside 决策');
});

// F8: EmojiTab 在 enterGrid/enterSidebar/focusSearchInput/blurSearchInput/emojiMode 切换 effect
// 中显式 navigationStore.setEmojiKbActive(...),但 useEffect([kbZone]) 已统一兜底。
// 双写竞态:store 同步写领先于 React render,连续两次 ↓(间隔 < commit)时
// useNavigationKeyboard listen 读 emojiKbActive=true 但 useRef kbZoneRef 仍是旧值,resolveZoneNav
// 决策基于旧 zone,按键被吞或跳区。
// 修复:删函数体内所有显式 setEmojiKbActive,全部交由 useEffect([kbZone]) 兜底。
test('F8 EmojiTab setEmojiKbActive 只在 useEffect 依赖内出现(单写)', async () => {
  const body = await readSource('../EmojiTab.jsx');

  const calls =
    body.match(/navigationStore\.setEmojiKbActive\([^)]*\)/g) || [];

  assert.equal(calls.length, 1);

  const effectBlock = body.match(
    /useEffect\(\(\) => \{[\s\S]{0,150}setEmojiKbActive[\s\S]{0,80}\}, \[kbZone\]\)/
  );

  assert.ok(effectBlock);
  assert.ok(
    effectBlock[0].includes("setEmojiKbActive(kbZone !== 'outside')")
  );
});

test('EmojiTab 侧栏最左切换使用 App 约定的 previous 方向', async () => {
  const body = await readSource('../EmojiTab.jsx');
  const prevModeStart = body.indexOf("if (emojiMode === 'emoji')");
  const prevModeEnd = body.indexOf('} else {', prevModeStart);
  assert.notEqual(prevModeStart, -1, '缺少 emoji 最左 prev-mode 分支');
  assert.notEqual(prevModeEnd, -1, '缺少 prev-mode 分支结束边界');
  const prevMode = body.slice(prevModeStart, prevModeEnd);
  assert.ok(prevMode.includes("onSwitchTab?.('previous')"), '最左子模式应请求 previous 顶层标签');
  assert.equal(prevMode.includes("onSwitchTab?.('favorites')"), false, '不得继续传旧 favorites 标签值');
});

test('App Emoji 边界切换使用相邻方向而非固定 tab', async () => {
  const body = await readSource('../../App.jsx');
  const start = body.indexOf('const handleEmojiSwitchTab');
  const end = body.indexOf('const handleTabLeft', start);
  assert.notEqual(start, -1, '缺 handleEmojiSwitchTab');
  assert.notEqual(end, -1, '缺 handleTabLeft 锚点');
  const fn = body.slice(start, end);
  assert.ok(fn.includes("direction !== 'previous' && direction !== 'next'"), '应只接受两个相邻切换方向');
  assert.ok(fn.includes('visibleTabs'), '应复用可见顶层标签顺序');
  assert.ok(fn.includes('setActiveTab'), '应由 App 更新顶层标签');
});
