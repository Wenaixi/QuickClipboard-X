import { test } from 'node:test';
import assert from 'node:assert/strict';
import { magnifierScaleForWheel } from './magnifierZoomModel.js';

test('magnifierScaleForWheel 向上滚轮放大一步', () => {
  assert.equal(magnifierScaleForWheel(6, -1), 7);
});

test('magnifierScaleForWheel 向下滚轮缩小一步', () => {
  assert.equal(magnifierScaleForWheel(6, 1), 5);
});

test('magnifierScaleForWheel 顶到最大上限夹紧', () => {
  assert.equal(magnifierScaleForWheel(24, -10), 24);
});

test('magnifierScaleForWheel 顶到最小下限夹紧', () => {
  assert.equal(magnifierScaleForWheel(2, 10), 2);
});

test('magnifierScaleForWheel 零增量保持不变', () => {
  assert.equal(magnifierScaleForWheel(8, 0), 8);
});

test('magnifierScaleForWheel 自定义范围与步长生效', () => {
  assert.equal(magnifierScaleForWheel(5, -1, { min: 1, max: 10, step: 2 }), 7);
  assert.equal(magnifierScaleForWheel(5, 1, { min: 1, max: 10, step: 2 }), 3);
});

test('magnifierScaleForWheel 拒绝无效输入或非法范围', () => {
  assert.throws(() => magnifierScaleForWheel(Number.NaN, -1), /当前缩放倍率 必须是有限数字/);
  assert.throws(() => magnifierScaleForWheel(6, -1, { min: 5, max: 4 }), /最大值不小于最小值/);
  assert.throws(() => magnifierScaleForWheel(6, -1, { step: 0 }), /步长/);
});
