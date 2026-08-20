import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('magnifierScaleForWheel 源码默认范围方向符号与夹紧语义锁定', () => {
  const source = readFileSync(new URL('./magnifierZoomModel.js', import.meta.url), 'utf8');
  // 源码护栏一：默认缩放范围必须为 min=2 max=24 step=1（ShareX 放大镜合理范围）。
  assert.ok(source.includes('const DEFAULT_MIN = 2;'), '默认最小倍率必须为 2');
  assert.ok(source.includes('const DEFAULT_MAX = 24;'), '默认最大倍率必须为 24');
  assert.ok(source.includes('const DEFAULT_STEP = 1;'), '默认步长必须为 1');
  // 源码护栏二：滚轮方向符号必须为 deltaY<0 放大（direction=+1）、deltaY>0 缩小（direction=-1），
  // 符号反转会让滚轮方向整体反向（向上滚反而缩小）。
  assert.ok(source.includes('const direction = deltaY < 0 ? 1 : -1;'), '方向符号必须为 deltaY<0 放大');
  // 源码护栏三：结果必须夹紧到 [min, max]（Math.min(max, Math.max(min, ...)) 双层夹紧）。
  assert.ok(source.includes('return Math.min(max, Math.max(min, currentScale + direction * step));'), '结果必须夹紧到范围内');
  // 行为验证：大幅滚轮增量只按步长移动一步（步长固定语义）；上下限夹紧；零增量保持。
  assert.equal(magnifierScaleForWheel(6, -100), 7, '大幅向上滚只放大一步');
  assert.equal(magnifierScaleForWheel(6, 100), 5, '大幅向下滚只缩小一步');
  assert.equal(magnifierScaleForWheel(2, -100), 3, '放大越过下限按步长走');
  assert.equal(magnifierScaleForWheel(23, 100), 22, '缩小越过上限按步长走');
  assert.equal(magnifierScaleForWheel(6, 0), 6, '零增量必须保持不变');
});

test('magnifierScaleForWheel 拒绝无效输入或非法范围', () => {
  assert.throws(() => magnifierScaleForWheel(Number.NaN, -1), /当前缩放倍率 必须是有限数字/);
  assert.throws(() => magnifierScaleForWheel(6, -1, { min: 5, max: 4 }), /最大值不小于最小值/);
  assert.throws(() => magnifierScaleForWheel(6, -1, { step: 0 }), /步长/);
});
