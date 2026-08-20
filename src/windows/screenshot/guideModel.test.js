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
  assert.equal(lines.horizontal.top, 1079, '水平线必须夹紧到 height-1 保持可见');
  assert.equal(lines.vertical.height, 1080);
  assert.equal(lines.horizontal.width, 1920);
});

test('guideLines 垂直线贯穿全高水平线贯穿全宽且坐标落在线内', () => {
  const source = readFileSync(new URL('./guideModel.js', import.meta.url), 'utf8');
  // 源码护栏：坐标必须夹紧到边界内（clamp 到 [0, width]/[0, height]），
  // 垂直线高度必须等于 bounds.height，水平线宽度必须等于 bounds.width（贯穿语义）。
  assert.ok(source.includes('const x = clamp(point.x, 0, bounds.width - 1);'), 'x 必须夹紧到边界内且保持 1px 线可见');
  assert.ok(source.includes('const y = clamp(point.y, 0, bounds.height - 1);'), 'y 必须夹紧到边界内且保持 1px 线可见');
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

test('guideLines 任意合法坐标下十字线整体在画布内可见', () => {
  // 不变量：引导线是 1px 宽的绘制元素，left/top 必须严格小于画布宽/高
  // （否则整条线画到画布外不可见）。右/下边缘（x=width, y=height）必须夹到 width-1/height-1。
  for (const point of [{ x: 0, y: 0 }, { x: 960, y: 540 }, { x: 1920, y: 1080 }, { x: -50, y: 5000 }, { x: 1919, y: 1079 }]) {
    const lines = guideLines(point, bounds);
    assert.ok(lines.vertical.left < bounds.width, `x=${point.x} 垂直线 left=${lines.vertical.left} 必须小于宽度`);
    assert.ok(lines.horizontal.top < bounds.height, `y=${point.y} 水平线 top=${lines.horizontal.top} 必须小于高度`);
    assert.equal(lines.vertical.height, bounds.height, '垂直线必须贯穿全高');
    assert.equal(lines.horizontal.width, bounds.width, '水平线必须贯穿全宽');
  }
});

test('guideLines 源码十字线线宽恒为 1px 且绘制元素完全包含于画布', () => {
  const source = readFileSync(new URL('./guideModel.js', import.meta.url), 'utf8');
  // 源码护栏一：垂直线宽度必须恒为 1（十字参考线是细线，若改宽会变成粗条遮挡选区）。
  assert.ok(source.includes('vertical: { left: x, top: 0, width: 1, height: bounds.height }'), '垂直线宽必须恒为 1px');
  // 源码护栏二：水平线高度必须恒为 1（同理）。
  assert.ok(source.includes('horizontal: { left: 0, top: y, width: bounds.width, height: 1 }'), '水平线高必须恒为 1px');
  // 行为属性：任意合法/越界输入下，两条线宽高恒为 1px，且整个绘制元素
  // （含右缘/下缘）完全包含在画布内：left+width <= bounds.width 且 top+height <= bounds.height。
  const bounds = { width: 1920, height: 1080 };
  for (const point of [{ x: 0, y: 0 }, { x: 960, y: 540 }, { x: 1919, y: 1079 }, { x: 1920, y: 1080 }, { x: -50, y: 5000 }, { x: 100, y: 800 }]) {
    const lines = guideLines(point, bounds);
    assert.equal(lines.vertical.width, 1, '垂直线宽必须恒为 1px');
    assert.equal(lines.horizontal.height, 1, '水平线高必须恒为 1px');
    assert.ok(lines.vertical.left + lines.vertical.width <= bounds.width, '垂直线右缘必须完全在画布内');
    assert.ok(lines.horizontal.top + lines.horizontal.height <= bounds.height, '水平线下缘必须完全在画布内');
  }
});

test('guideLines 拒绝无效坐标或边界', () => {
  assert.throws(() => guideLines({ x: Number.NaN, y: 1 }, bounds), /点 x 必须是有限数字/);
  assert.throws(() => guideLines({ x: 1, y: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});
