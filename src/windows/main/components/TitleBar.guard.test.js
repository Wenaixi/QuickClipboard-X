// 标题栏贴边隐藏快捷开关护栏:关闭 hide 必须连带关闭 hover(对齐 §5.6
// 设置页语义)。后端 save_settings 的归一化只压后端持久化,前端
// saveSettingsToBackend emit 原始 payload 会把各窗口 store 的
// edgeHoverPopupEnabled 弹回 true,store 层残留 hide=false/hover=true
// 违规组合并持续——设置页开关显示开启、UI 与后端语义分叉。快捷开关
// 必须 saveSettings 一次写两个键(hide 与 hover 同值)。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./TitleBar.jsx', import.meta.url), 'utf8');

test('标题栏关贴边隐藏必须连带关闭悬浮弹出', () => {
  const handlerStart = source.indexOf('const handleToggleEdgeHide');
  assert.ok(handlerStart !== -1, '必须存在 handleToggleEdgeHide');
  const handlerBody = source.slice(handlerStart, source.indexOf('const handleOpenSettings'));
  assert.ok(
    handlerBody.includes('saveSettings({') &&
      handlerBody.includes('edgeHideEnabled: nextValue') &&
      handlerBody.includes('edgeHoverPopupEnabled: nextValue'),
    '关闭 hide 必须 saveSettings 同时写 hover 同值(不得只写 hide)',
  );
});
