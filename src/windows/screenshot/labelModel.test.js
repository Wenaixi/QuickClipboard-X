import { test } from 'node:test';
import assert from 'node:assert/strict';
import { selectionLabelPlacement } from './labelModel.js';

const bounds = { width: 1920, height: 1080 };

test('selectionLabelPlacement 中央选区默认放在上方且右对齐', () => {
  const placement = selectionLabelPlacement(
    { left: 800, top: 400, right: 1100, bottom: 700, width: 300, height: 300 },
    bounds
  );
  assert.deepEqual(placement, { above: true, alignLeft: false });
});

test('selectionLabelPlacement 顶部空间不足时翻到选区下方', () => {
  const placement = selectionLabelPlacement(
    { left: 800, top: 2, right: 1100, bottom: 300, width: 300, height: 298 },
    bounds
  );
  assert.equal(placement.above, false);
});

test('selectionLabelPlacement 左侧窄选区右对齐会越界时改为左对齐', () => {
  const placement = selectionLabelPlacement(
    { left: 0, top: 400, right: 60, bottom: 460, width: 60, height: 60 },
    bounds
  );
  assert.equal(placement.alignLeft, true);
  assert.equal(placement.above, true);
});

test('selectionLabelPlacement 恰好等于阈值时保持默认方向', () => {
  const placement = selectionLabelPlacement(
    { left: 800, top: 30, right: 1100, bottom: 330, width: 300, height: 300 },
    bounds,
    { height: 22, gap: 8 }
  );
  assert.equal(placement.above, true, 'top - height - gap = 0 时应保持在上方');
});

test('selectionLabelPlacement 拒绝无效输入或负尺寸', () => {
  assert.throws(() => selectionLabelPlacement({ left: 1, top: 1, right: 2, bottom: 2 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => selectionLabelPlacement({ left: 1, top: 1, right: 2, bottom: 2 }, bounds, { height: -1 }), /标签宽度与高度必须为正数/);
  assert.throws(() => selectionLabelPlacement({ left: 1, top: 1, right: 2, bottom: 2 }, bounds, { gap: -1 }), /标签间隙不能为负数/);
});
