import { test } from 'node:test';
import assert from 'node:assert/strict';
import { rulerMajorStep, rulerTicks } from './rulerModel.js';

test('rulerMajorStep 按屏幕尺寸自适应主刻度间隔', () => {
  assert.equal(rulerMajorStep(800), 50);
  assert.equal(rulerMajorStep(900), 100);
  assert.equal(rulerMajorStep(1600), 100);
  assert.equal(rulerMajorStep(1700), 200);
  assert.equal(rulerMajorStep(3840), 200);
});

test('rulerMajorStep 拒绝非正或非法长度', () => {
  assert.throws(() => rulerMajorStep(0), /标尺长度必须为正数/);
  assert.throws(() => rulerMajorStep(-10), /标尺长度必须为正数/);
  assert.throws(() => rulerMajorStep(Number.NaN), /标尺长度 必须是有限数字/);
});

test('rulerTicks 输出从 0 到长度的全部刻度并标记主刻度标签', () => {
  const ticks = rulerTicks(300);
  assert.equal(ticks.length, 31);
  assert.deepEqual(ticks[0], { position: 0, label: '0' });
  assert.deepEqual(ticks[5], { position: 50, label: '50' });
  assert.deepEqual(ticks[10], { position: 100, label: '100' });
  assert.equal(ticks[3].label, null);
  assert.equal(ticks[30].label, '300');
});

test('rulerTicks 主刻度间隔为 100 时标签只出现在整百位置', () => {
  const ticks = rulerTicks(1080);
  assert.equal(ticks.length, 55);
  const labeled = ticks.filter((tick) => tick.label !== null);
  assert.deepEqual(labeled.map((tick) => tick.position), [0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000]);
});

test('rulerTicks 拒绝非正或非法长度', () => {
  assert.throws(() => rulerTicks(0), /标尺长度必须为正数/);
  assert.throws(() => rulerTicks(Number.NaN), /标尺长度 必须是有限数字/);
});
