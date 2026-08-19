import { test } from 'node:test';
import assert from 'node:assert/strict';
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

test('toolbarStyle 拒绝无效输入', () => {
  assert.throws(() => toolbarStyle({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 1920, height: 1080 }, 'x'), /placement 必须是 above 或 below/);
  assert.throws(() => toolbarStyle({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 1920, height: 1080 }, 'below', -1), /工具栏宽度必须为正数/);
});
