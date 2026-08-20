import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fullScreenSelection } from './fullScreenModel.js';

test('fullScreenSelection 生成覆盖整个边界的选区', () => {
  assert.deepEqual(fullScreenSelection({ width: 1920, height: 1080 }), { left: 0, top: 0, right: 1920, bottom: 1080, width: 1920, height: 1080 });
});

test('fullScreenSelection 支持小数边界并向上取整完整覆盖', () => {
  // 右/下边界向上取整，保证选区完全覆盖屏幕而不留空白边。
  const selection = fullScreenSelection({ width: 100.6, height: 50.4 });
  assert.equal(selection.right, 101);
  assert.equal(selection.bottom, 51);
  assert.equal(selection.width, 101);
  assert.equal(selection.height, 51);
});

test('fullScreenSelection 源码 ceil 完整覆盖且 left/top 恒 0 宽高由右/下推导', () => {
  const source = readFileSync(new URL('./fullScreenModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function fullScreenSelection');
  const body = source.slice(start, start + 600);
  // 源码护栏一：右/下边界必须向上取整（小数边界完整覆盖屏幕不留空白边）。
  assert.ok(body.includes('const right = Math.ceil(bounds.width);'), '右边界必须向上取整');
  assert.ok(body.includes('const bottom = Math.ceil(bounds.height);'), '下边界必须向上取整');
  // 源码护栏二：全屏选区必须从原点开始（left/top 恒为 0）。
  assert.ok(body.includes('return { left: 0, top: 0, right, bottom, width: right, height: bottom };'), '全屏选区必须从原点开始且宽高由右/下推导');
  // 行为属性：任意合法边界下选区完整覆盖 [0, width] x [0, height]，不越界不遗漏。
  for (const bounds of [{ width: 1920, height: 1080 }, { width: 100.6, height: 50.4 }, { width: 1, height: 1 }, { width: 0.2, height: 0.7 }]) {
    const selection = fullScreenSelection(bounds);
    assert.equal(selection.left, 0, 'left 必须为 0');
    assert.equal(selection.top, 0, 'top 必须为 0');
    assert.ok(selection.right >= bounds.width, '右边界必须覆盖屏幕宽度');
    assert.ok(selection.bottom >= bounds.height, '下边界必须覆盖屏幕高度');
    assert.equal(selection.width, selection.right, '宽度必须等于右边界');
    assert.equal(selection.height, selection.bottom, '高度必须等于下边界');
    assert.ok(selection.right - bounds.width < 1, '右边界不得多出整像素以上');
    assert.ok(selection.bottom - bounds.height < 1, '下边界不得多出整像素以上');
  }
  // 整数边界必须精确等于边界（无多余取整）。
  const exact = fullScreenSelection({ width: 1920, height: 1080 });
  assert.equal(exact.right, 1920);
  assert.equal(exact.bottom, 1080);
});

test('fullScreenSelection 拒绝无效边界', () => {
  assert.throws(() => fullScreenSelection(null), /边界/);
  assert.throws(() => fullScreenSelection({ width: 0, height: 1 }), /正数/);
  assert.throws(() => fullScreenSelection({ width: 1, height: Number.NaN }), /有限数字/);
});
