import { test } from 'node:test';
import assert from 'node:assert/strict';
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

test('lineStyle 拒绝无效输入', () => {
  assert.throws(() => lineStyle(Number.NaN, '#fff', 1), /线宽 必须是有限数字/);
  assert.throws(() => lineStyle(1, Number.NaN, 0.5), /颜色/);
  assert.throws(() => lineStyle(-1, '#0000ff', 0.5), /线宽不能为负数/);
  assert.throws(() => lineStyle(1, '#fff', 2), /透明度必须在 0 到 1 之间/);
  assert.throws(() => lineStyle(1, '#fff', -0.1), /透明度必须在 0 到 1 之间/);
});
