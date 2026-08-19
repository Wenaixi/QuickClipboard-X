import { test } from 'node:test';
import assert from 'node:assert/strict';
import { selectionPhysicalPosition, formatSelectionPosition } from './positionModel.js';

test('selectionPhysicalPosition 默认按 1:1 与原点换算', () => {
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }), { x: 100, y: 50 });
  assert.deepEqual(selectionPhysicalPosition({ left: 0, top: 0 }), { x: 0, y: 0 });
});

test('selectionPhysicalPosition 按 DPR 换算并向下取整', () => {
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }, { dpr: 1.5 }), { x: 150, y: 75 });
  assert.deepEqual(selectionPhysicalPosition({ left: 100.5, top: 50 }, { dpr: 1.5 }), { x: 150, y: 75 });
});

test('selectionPhysicalPosition 支持负坐标副屏的显示器偏移', () => {
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }, { monitorLeft: -1920 }), { x: -1820, y: 50 });
  assert.deepEqual(selectionPhysicalPosition({ left: 100, top: 50 }, { monitorLeft: -1920, dpr: 1.5 }), { x: -2730, y: 75 });
});

test('formatSelectionPosition 输出固定格式位置文案', () => {
  assert.equal(formatSelectionPosition({ left: 100, top: 50 }, { dpr: 1.5 }), 'X: 150  Y: 75');
  assert.equal(formatSelectionPosition({ left: 0, top: 0 }), 'X: 0  Y: 0');
});

test('selectionPhysicalPosition 拒绝非法输入', () => {
  assert.throws(() => selectionPhysicalPosition(null), /选区/);
  assert.throws(() => selectionPhysicalPosition({ left: Number.NaN, top: 1 }), /left 必须是有限数字/);
  assert.throws(() => selectionPhysicalPosition({ left: 1, top: 1 }, { dpr: 0 }), /dpr 必须是正数/);
  assert.throws(() => selectionPhysicalPosition({ left: 1, top: 1 }, { monitorTop: Number.NaN }), /monitorTop 必须是有限数字/);
});
