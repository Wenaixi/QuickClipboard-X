import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

// 分组面板语言包护栏:侧边栏收起/展开文案与「颜色」标签此前是硬编码中文,
// en 用户可感知。护栏断言组件已接语言包键、双语键均存在——防硬编码复活。

const popup = readFileSync(new URL('./GroupsPopup.jsx', import.meta.url), 'utf8');
const modal = readFileSync(new URL('./GroupEditModal.jsx', import.meta.url), 'utf8');
const zh = readFileSync(new URL('../../../shared/locales/zh-CN.json', import.meta.url), 'utf8');
const en = readFileSync(new URL('../../../shared/locales/en-US.json', import.meta.url), 'utf8');

test('分组侧边栏收起/展开文案必须走语言包', () => {
  assert.ok(
    popup.includes("t('groups.sidebarExpand')") && popup.includes("t('groups.sidebarCollapse')"),
    '侧边栏 tooltip 与按钮文本必须接语言包键'
  );
  assert.ok(!popup.includes("'展开分组栏'") && !popup.includes("'收起分组栏'"), '不得再内联中文');
  assert.match(zh, /"sidebarExpand"\s*:\s*"/, 'zh 必须含 sidebarExpand');
  assert.match(zh, /"sidebarCollapse"\s*:\s*"/, 'zh 必须含 sidebarCollapse');
  assert.match(en, /"sidebarCollapse"\s*:\s*"/, 'en 必须含 sidebarCollapse');
});

test('分组编辑「颜色」标签必须走语言包', () => {
  assert.ok(
    modal.includes("{t('groups.modal.colorLabel')}"),
    '颜色标签必须接语言包键'
  );
  assert.ok(!modal.includes('颜色'), '不得再内联中文「颜色」');
  assert.match(zh, /"colorLabel"\s*:\s*"/, 'zh 语言包必须含 colorLabel');
  assert.match(en, /"colorLabel"\s*:\s*"/, 'en 语言包必须含 colorLabel');
});
