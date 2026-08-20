import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { toolbarPlacement, toolbarStyle } from './toolbarModel.js';

test('toolbarPlacement 选区靠近上缘时工具栏放到选区下方', () => {
  assert.equal(toolbarPlacement({ left: 200, top: 30, right: 500, bottom: 300, width: 300, height: 270 }, { width: 1920, height: 1080 }), 'below');
});

test('toolbarPlacement 选区靠近下缘时工具栏放到选区上方', () => {
  assert.equal(toolbarPlacement({ left: 200, top: 800, right: 500, bottom: 1070, width: 300, height: 270 }, { width: 1920, height: 1080 }), 'above');
});

test('toolbarPlacement 选区贴近下缘且上方空间更大时翻转到上方', () => {
  assert.equal(toolbarPlacement({ left: 200, top: 700, right: 500, bottom: 1000, width: 300, height: 300 }, { width: 1920, height: 1080 }), 'above');
});

test('toolbarPlacement 选区贴近上缘且高度很大时仍优先下方', () => {
  assert.equal(toolbarPlacement({ left: 100, top: 10, right: 400, bottom: 900, width: 300, height: 890 }, { width: 1920, height: 1080 }), 'below');
});

test('toolbarPlacement 拒绝无效输入', () => {
  assert.throws(() => toolbarPlacement(null, { width: 1920, height: 1080 }), /选区/);
  assert.throws(() => toolbarPlacement({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});

test('toolbarStyle 下方放置返回选区下缘坐标', () => {
  const style = toolbarStyle({ left: 200, top: 100, right: 500, bottom: 400 }, { width: 1920, height: 1080 }, 'below');
  assert.equal(style.left, '208px');
  assert.equal(style.top, '408px');
});

test('toolbarStyle 上方放置返回选区上缘坐标并向上让出工具栏高度', () => {
  const style = toolbarStyle({ left: 200, top: 100, right: 500, bottom: 400 }, { width: 1920, height: 1080 }, 'above');
  assert.equal(style.left, '208px');
  assert.equal(style.top, '52px');
});

test('toolbarPlacement 上方空间不足以容纳工具栏时仍放下方避免越界', () => {
  // 选区顶部 20px 无法容纳工具栏自身高度与间距，必须放下方。
  assert.equal(toolbarPlacement({ left: 200, top: 20, right: 500, bottom: 200, width: 300, height: 180 }, { width: 1920, height: 1080 }), 'below');
});

test('toolbarStyle 选区贴近右缘时工具栏左移夹紧到显示器内', () => {
  const style = toolbarStyle({ left: 1800, top: 100, right: 1900, bottom: 400 }, { width: 1920, height: 1080 }, 'below');
  assert.equal(style.left, '1612px');
  assert.equal(style.top, '408px');
});

test('toolbarPlacement 默认下方且仅下方不足且上方更大才翻转的源码语义', () => {
  const source = readFileSync(new URL('./toolbarModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function toolbarPlacement');
  const body = source.slice(start, start + 800);
  // 源码护栏一：工具栏间隔与高度常量必须存在（上方放置时上移自身高度与间距）。
  assert.ok(source.includes('const TOOLBAR_GAP = 8;'), '工具栏间隔常量必须为 8');
  assert.ok(source.includes('const TOOLBAR_HEIGHT = 40;'), '工具栏高度常量必须为 40');
  // 源码护栏二：默认放下方；仅当上方空间不足（<= 间隔+高度）或下方空间更大时维持下方。
  assert.ok(body.includes("return 'below';"), '默认必须返回 below');
  assert.ok(body.includes('spaceAbove <= TOOLBAR_GAP + TOOLBAR_HEIGHT'), '上方空间不足必须维持下方');
  assert.ok(body.includes('spaceBelow >= spaceAbove'), '下方空间不小于上方时维持下方');
  // 行为属性：贴近上缘放下方、贴近下缘且上方更大翻上方、上方不足仍下方、拒绝非法。
  const bounds = { width: 1920, height: 1080 };
  assert.equal(toolbarPlacement({ left: 200, top: 30, right: 500, bottom: 300, width: 300, height: 270 }, bounds), 'below');
  assert.equal(toolbarPlacement({ left: 200, top: 800, right: 500, bottom: 1070, width: 300, height: 270 }, bounds), 'above');
  assert.equal(toolbarPlacement({ left: 200, top: 20, right: 500, bottom: 200, width: 300, height: 180 }, bounds), 'below', '上方空间不足必须维持下方');
  assert.throws(() => toolbarPlacement(null, bounds), /选区/);
});

test('toolbarStyle 常量单一来源且两侧夹紧的源码语义', () => {
  const source = readFileSync(new URL('./toolbarModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function toolbarStyle');
  const body = source.slice(start, start + 900);
  // 源码护栏一：左右夹紧必须用同一常量（间隔 8 与工具栏宽度边界）。
  assert.ok(body.includes('const maxLeft = Math.max(TOOLBAR_GAP, bounds.width - toolbarWidth - TOOLBAR_GAP);'), '右缘夹紧必须推导最大左缘');
  assert.ok(body.includes('clamp(selection.left + TOOLBAR_GAP, TOOLBAR_GAP, maxLeft)'), '左缘必须夹紧到间隔内');
  // 源码护栏二：上方放置必须上移间隔 + 高度（避免遮挡选区上边缘）。
  assert.ok(body.includes('selection.top - TOOLBAR_GAP - TOOLBAR_HEIGHT'), '上方放置必须让出间隔与高度');
  assert.ok(body.includes('selection.bottom + TOOLBAR_GAP'), '下方放置必须贴选区下缘');
  // 行为属性：下方 408、上方 52、贴近右缘夹紧、拒绝非法 placement。
  const bounds = { width: 1920, height: 1080 };
  assert.equal(toolbarStyle({ left: 200, top: 100, right: 500, bottom: 400 }, bounds, 'below').top, '408px');
  assert.equal(toolbarStyle({ left: 200, top: 100, right: 500, bottom: 400 }, bounds, 'above').top, '52px');
  assert.equal(toolbarStyle({ left: 1800, top: 100, right: 1900, bottom: 400 }, bounds, 'below').left, '1612px');
  assert.throws(() => toolbarStyle({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, 'x'), /placement 必须是 above 或 below/);
});

test('toolbarStyle 拒绝无效输入', () => {
  assert.throws(() => toolbarStyle({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 1920, height: 1080 }, 'x'), /placement 必须是 above 或 below/);
  assert.throws(() => toolbarStyle({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 1920, height: 1080 }, 'below', -1), /工具栏宽度必须为正数/);
});
