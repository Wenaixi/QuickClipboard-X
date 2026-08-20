import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { lineStyle } from './annotationModel.js';

test('lineStyle 宽度与透明度映射为 CSS 描边', () => {
  assert.deepEqual(lineStyle(3, '#ff0000', 0.8), {
    borderWidth: '3px',
    borderColor: '#ff0000',
    opacity: 0.8,
  });
});

test('lineStyle 过宽宽度夹紧到 64px 上限', () => {
  assert.deepEqual(lineStyle(200, '#00ff00', 0.9), {
    borderWidth: '64px',
    borderColor: '#00ff00',
    opacity: 0.9,
  });
});

test('lineStyle 零宽度产生隐藏描边', () => {
  const style = lineStyle(0, '#ffffff', 1);
  assert.equal(style.borderWidth, '0px');
});

test('lineStyle 源码线宽夹紧上限 64 且零宽度隐藏描边语义锁定', () => {
  const source = readFileSync(new URL('./annotationModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function lineStyle');
  const body = source.slice(start, start + 600);
  // 源码护栏一：线宽必须夹紧到 64px 上限（Math.min(width, 64)），防止异常输入撑爆边框渲染。
  assert.ok(body.includes('const safeWidth = Math.min(width, 64);'), '线宽必须夹紧到 64px 上限');
  // 源码护栏二：返回对象必须用夹紧后的宽度（borderWidth: `${safeWidth}px`），且颜色透传、透明度透传。
  assert.ok(body.includes('borderWidth: `${safeWidth}px`,'), '边框宽度必须使用夹紧后的值');
  assert.ok(body.includes('borderColor: color,'), '边框颜色必须透传');
  assert.ok(body.includes('opacity,'), '透明度必须透传');
  // 行为验证：宽于 64 夹紧到 64px、恰好 64 保持、零宽度隐藏描边、颜色与透明度原样透传。
  assert.deepEqual(lineStyle(200, '#ff0000', 0.8), { borderWidth: '64px', borderColor: '#ff0000', opacity: 0.8 }, '超宽必须夹紧到 64px');
  assert.deepEqual(lineStyle(64, '#00ff00', 1), { borderWidth: '64px', borderColor: '#00ff00', opacity: 1 }, '恰好 64 必须保持');
  assert.deepEqual(lineStyle(0, '#ffffff', 0), { borderWidth: '0px', borderColor: '#ffffff', opacity: 0 }, '零宽度隐藏描边且透明度 0 透传');
  assert.deepEqual(lineStyle(5, 'rgba(1,2,3,0.5)', 0.5), { borderWidth: '5px', borderColor: 'rgba(1,2,3,0.5)', opacity: 0.5 }, '任意 CSS 颜色字符串必须透传');
});

test('lineStyle 拒绝无效输入', () => {
  assert.throws(() => lineStyle(Number.NaN, '#fff', 1), /线宽 必须是有限数字/);
  assert.throws(() => lineStyle(1, Number.NaN, 0.5), /颜色/);
  assert.throws(() => lineStyle(-1, '#0000ff', 0.5), /线宽不能为负数/);
  assert.throws(() => lineStyle(1, '#fff', 2), /透明度必须在 0 到 1 之间/);
  assert.throws(() => lineStyle(1, '#fff', -0.1), /透明度必须在 0 到 1 之间/);
});
