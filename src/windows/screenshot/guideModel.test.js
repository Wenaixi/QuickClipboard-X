import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { guideLines } from './guideModel.js';

const bounds = { width: 1920, height: 1080 };

test('guideLines 光标居中时垂直线贯穿高度且水平线贯穿宽度', () => {
  const lines = guideLines({ x: 960, y: 540 }, bounds);
  assert.deepEqual(lines.vertical, { left: 960, top: 0, width: 1, height: 1080 });
  assert.deepEqual(lines.horizontal, { left: 0, top: 540, width: 1920, height: 1 });
});

test('guideLines 光标越出边界时夹紧到边界内', () => {
  const lines = guideLines({ x: -50, y: 5000 }, bounds);
  assert.equal(lines.vertical.left, 0);
  assert.equal(lines.horizontal.top, 1080);
  assert.equal(lines.vertical.height, 1080);
  assert.equal(lines.horizontal.width, 1920);
});

test('guideLines 垂直线贯穿全高水平线贯穿全宽且坐标落在线内', () => {
  const source = readFileSync(new URL('./guideModel.js', import.meta.url), 'utf8');
  // 源码护栏：坐标必须夹紧到边界内（clamp 到 [0, width]/[0, height]），
  // 垂直线高度必须等于 bounds.height，水平线宽度必须等于 bounds.width（贯穿语义）。
  assert.ok(source.includes('const x = clamp(point.x, 0, bounds.width);'), 'x 必须夹紧到边界内');
  assert.ok(source.includes('const y = clamp(point.y, 0, bounds.height);'), 'y 必须夹紧到边界内');
  assert.ok(source.includes('vertical: { left: x, top: 0, width: 1, height: bounds.height }'), '垂直线必须贯穿整个高度');
  assert.ok(source.includes('horizontal: { left: 0, top: y, width: bounds.width, height: 1 }'), '水平线必须贯穿整个宽度');
  // 行为属性：任意合法坐标下，两条线必须贯穿对应全轴，且光标坐标精确落在线上。
  for (const point of [{ x: 0, y: 0 }, { x: 960, y: 540 }, { x: 1919, y: 1079 }, { x: 100, y: 800 }]) {
    const lines = guideLines(point, bounds);
    assert.equal(lines.vertical.height, bounds.height, '垂直线必须贯穿全高');
    assert.equal(lines.horizontal.width, bounds.width, '水平线必须贯穿全宽');
    assert.equal(lines.vertical.top, 0, '垂直线必须从顶部开始');
    assert.equal(lines.horizontal.left, 0, '水平线必须从左侧开始');
    assert.ok(point.x >= lines.vertical.left && point.x <= lines.vertical.left + 1, '光标 x 必须落在垂直线内');
    assert.ok(point.y >= lines.horizontal.top && point.y <= lines.horizontal.top + 1, '光标 y 必须落在水平线内');
  }
});

test('guideLines 拒绝无效坐标或边界', () => {
  assert.throws(() => guideLines({ x: Number.NaN, y: 1 }, bounds), /点 x 必须是有限数字/);
  assert.throws(() => guideLines({ x: 1, y: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});
