// 标题栏贴边隐藏快捷开关护栏:关闭 hide 必须连带关闭 hover(对齐 §5.6
// 设置页语义——后端归一化 hide=false⇒hover=false 只压后端,前端 emit
// 原始 payload 会把各窗口 store 的 edgeHoverPopupEnabled 弹回 true,
// store 层残留 hide=false/hover=true 违规组合并持续)。开启分支只写
// hide、绝不强制打开 hover(反向不蕴含,保留用户上次偏好,对齐后端
// enabling_edge_hide_keeps_hover_off 锁死决策)。失败路径同步回滚
// hide/hover 双键,不留违规组合。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./TitleBar.jsx', import.meta.url), 'utf8');
const handlerStart = source.indexOf('const handleToggleEdgeHide');
const handlerBody = source.slice(handlerStart, source.indexOf('const handleOpenSettings'));

test('标题栏关贴边隐藏必须连带关闭悬浮弹出,开贴边不得强制打开悬浮', () => {
  assert.ok(
    handlerBody.includes('edgeHoverPopupEnabled: false'),
    '关闭分支必须 saveSettings 同时写 hover=false(不得只写 hide)',
  );
  assert.ok(
    handlerBody.includes('edgeHoverPopupEnabled: true') === false,
    '开启分支不得强制打开 hover(反向不蕴含,保留用户偏好)',
  );
  assert.ok(
    handlerBody.includes('nextValue\n          ? { edgeHideEnabled: true }') ||
      handlerBody.includes('? { edgeHideEnabled: true }'),
    '开启分支必须只写 hide',
  );
});

test('标题栏切贴边隐藏失败必须回滚 hide 与 hover 双键(不留违规组合)', () => {
  assert.ok(
    handlerBody.includes('settingsStore.edgeHoverPopupEnabled = previousHover;'),
    '失败路径必须同步回滚 hover',
  );
});
