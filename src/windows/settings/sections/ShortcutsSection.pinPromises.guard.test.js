import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

// 贴图操作设置页承诺一致性护栏:GDI 贴图窗口尚未接线「切换缩略图」(右键
// 菜单实现侧已隐藏入口,见 menu.rs 注释)与整组滚轮缩放(WM_MOUSEWHEEL 走
// DefWindowProc,需渲染管线配合)。设置页此前印制了这些交互承诺,用户照做
// 无任何反应——承诺与实现背离。本护栏断言设置页不再印制未实现的贴图操作,
// 渲染管线接好前不允许重新向用户展示失效承诺。

const sectionSource = readFileSync(new URL('./ShortcutsSection.jsx', import.meta.url), 'utf8');
const zhLocale = readFileSync(new URL('../../../shared/locales/zh-CN.json', import.meta.url), 'utf8');
const enLocale = readFileSync(new URL('../../../shared/locales/en-US.json', import.meta.url), 'utf8');

test('设置页不得印制未接线的「切换缩略图」操作', () => {
  assert.ok(
    !sectionSource.includes("t('settings.shortcuts.pinThumbnail')"),
    '缩略图切换未接线,设置页不得印制该操作'
  );
});

test('设置页不得印制未接线的滚轮缩放操作家族', () => {
  for (const key of [
    'pinZoom',
    'pinZoomFast',
    'pinZoomFine',
    'pinInnerZoom',
    'pinInnerZoomFast',
    'pinInnerDrag',
  ]) {
    assert.ok(
      !sectionSource.includes(`t('settings.shortcuts.${key}')`),
      `${key} 未接线,设置页不得印制该缩放操作`
    );
  }
});

test('未接线操作的绑定语言包键已从双语包移除', () => {
  for (const key of [
    'pinThumbnail',
    'pinThumbnailDesc',
    'pinZoom',
    'pinZoomDesc',
    'pinZoomFast',
    'pinZoomFastDesc',
    'pinZoomFine',
    'pinZoomFineDesc',
    'pinInnerZoom',
    'pinInnerZoomDesc',
    'pinInnerZoomFast',
    'pinInnerZoomFastDesc',
    'pinInnerDrag',
    'pinInnerDragDesc',
  ]) {
    const zh = new RegExp(`"${key}"\\s*:`);
    assert.ok(!zh.test(zhLocale), `zh 语言包不得残留未接线键 ${key}`);
    assert.ok(!zh.test(enLocale), `en 语言包不得残留未接线键 ${key}`);
  }
});