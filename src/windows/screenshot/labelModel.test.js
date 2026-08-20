import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('selectionLabelPlacement 源码默认高度 22 间隙 6 且方向判定语义完整', () => {
  const source = readFileSync(new URL('./labelModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function selectionLabelPlacement');
  const body = source.slice(start, start + 900);
  // 源码护栏一：默认标签高度必须为 22px（与坐标面板一致的紧凑标签尺寸）。
  assert.ok(body.includes('const labelHeight = options.height ?? 22;'), '默认标签高度必须为 22');
  // 源码护栏二：默认间隙必须为 6px（标签与选区边缘的视觉间距）。
  assert.ok(body.includes('const gap = options.gap ?? 6;'), '默认间隙必须为 6');
  // 源码护栏三：上方判定必须比较 top - height - gap >= 0（空间不足翻下方）。
  assert.ok(body.includes('const above = selection.top - labelHeight - gap >= 0;'), '上方判定必须比较剩余空间');
  // 源码护栏四：左对齐判定必须比较 right - labelWidth < 0（右对齐越界改左对齐）。
  assert.ok(body.includes('const alignLeft = selection.right - labelWidth < 0;'), '左对齐判定必须比较右缘与标签宽');
  // 行为属性：默认上方右对齐；顶部不足翻下方；窄选区改左对齐；阈值边界保持默认。
  assert.deepEqual(selectionLabelPlacement({ left: 800, top: 400, right: 1100, bottom: 700 }, bounds), { above: true, alignLeft: false });
  assert.equal(selectionLabelPlacement({ left: 800, top: 2, right: 1100, bottom: 300 }, bounds).above, false);
  assert.equal(selectionLabelPlacement({ left: 0, top: 400, right: 60, bottom: 460 }, bounds).alignLeft, true);
  assert.equal(selectionLabelPlacement({ left: 800, top: 30, right: 1100, bottom: 330 }, bounds, { height: 22, gap: 8 }).above, true);
});

test('selectionLabelPlacement 左上角选区上下与左右同时翻转且两判定相互独立', () => {
  const source = readFileSync(new URL('./labelModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function selectionLabelPlacement');
  const body = source.slice(start, start + 900);
  // 源码护栏一：上方判定必须只依赖 top/labelHeight/gap（不含 right），左对齐判定
  // 必须只依赖 right/labelWidth（不含 top）——两方向翻转独立，互不耦合。
  const aboveLine = body.split('\n').find((l) => l.includes('const above ='));
  const alignLine = body.split('\n').find((l) => l.includes('const alignLeft ='));
  assert.ok(aboveLine && !aboveLine.includes('right'), '上方判定不得依赖右缘');
  assert.ok(alignLine && !alignLine.includes('top'), '左对齐判定不得依赖上缘');
  // 行为验证：左上角选区（top=2 上方放不下、right=60 右对齐越界）必须同时翻下方 + 左对齐。
  const corner = selectionLabelPlacement(
    { left: 0, top: 2, right: 60, bottom: 62, width: 60, height: 60 },
    bounds
  );
  assert.deepEqual(corner, { above: false, alignLeft: true }, '左上角选区必须同时翻下方且左对齐');
  // 对照：仅右缘窄（top 充足）只翻左对齐不翻下方；仅上缘不足（right 充足）只翻下方不翻左对齐。
  assert.deepEqual(selectionLabelPlacement({ left: 0, top: 400, right: 60, bottom: 460 }, bounds), { above: true, alignLeft: true });
  assert.deepEqual(selectionLabelPlacement({ left: 800, top: 2, right: 1100, bottom: 302 }, bounds), { above: false, alignLeft: false });
});

test('selectionLabelPlacement 拒绝无效输入或负尺寸', () => {
  assert.throws(() => selectionLabelPlacement({ left: 1, top: 1, right: 2, bottom: 2 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => selectionLabelPlacement({ left: 1, top: 1, right: 2, bottom: 2 }, bounds, { height: -1 }), /标签宽度与高度必须为正数/);
  assert.throws(() => selectionLabelPlacement({ left: 1, top: 1, right: 2, bottom: 2 }, bounds, { gap: -1 }), /标签间隙不能为负数/);
});
