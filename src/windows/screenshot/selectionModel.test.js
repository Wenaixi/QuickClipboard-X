import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { physicalSize } from './sizeModel.js';
import {
  createRafWriter,
  hitSelectionEdge,
  hitSelectionInterior,
  isCurrentGesture,
  normalizeSelection,
  nudgeSelection,
  resizeSelection,
  selectionForPointerGesture,
  selectionFromPhysical,
  selectionToPhysical,
  squareSelection,
} from './selectionModel.js';

const bounds = { width: 800, height: 600 };

function selectionValues(selection) {
  return {
    left: selection.left,
    top: selection.top,
    right: selection.right,
    bottom: selection.bottom,
    width: selection.width,
    height: selection.height,
  };
}

test('normalizeSelection 支持反向拖拽并返回统一矩形', () => {
  const selection = normalizeSelection(
    { x: 300, y: 240 },
    { x: 100, y: 80 },
    bounds
  );

  assert.deepEqual(selectionValues(selection), {
    left: 100,
    top: 80,
    right: 300,
    bottom: 240,
    width: 200,
    height: 160,
  });
});

test('normalizeSelection 将起止点都夹在显示器边界内', () => {
  const selection = normalizeSelection(
    { x: -20, y: 700 },
    { x: 900, y: -30 },
    bounds
  );

  assert.deepEqual(selectionValues(selection), {
    left: 0,
    top: 0,
    right: 800,
    bottom: 600,
    width: 800,
    height: 600,
  });
});

test('normalizeSelection 在右下边缘仍保证最小 1x1', () => {
  const selection = normalizeSelection(
    { x: 800, y: 600 },
    { x: 800, y: 600 },
    bounds
  );

  assert.deepEqual(selectionValues(selection), {
    left: 799,
    top: 599,
    right: 800,
    bottom: 600,
    width: 1,
    height: 1,
  });
});

test('normalizeSelection 反选统一且起止点夹紧并最小 1x1', () => {
  const source = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  // 源码护栏：单轴归一化必须把起止两端都夹到 [0, limit] 再取 min/max（越界拖拽不产生负坐标或越界矩形），
  // 且退化点（high == low）必须扩成最小 1x1：右/下边缘向内侧扩（低端减一），非边缘向高端扩一。
  const axisStart = source.indexOf('function normalizeAxis(start, end, limit) {');
  const axisBody = source.slice(axisStart, axisStart + 400);
  assert.ok(axisBody.includes('Math.min(clamp(start, 0, limit), clamp(end, 0, limit))'), '低端必须是两端夹紧后的较小值');
  assert.ok(axisBody.includes('Math.max(clamp(start, 0, limit), clamp(end, 0, limit))'), '高端必须是两端夹紧后的较大值');
  assert.ok(axisBody.includes('if (low >= limit) {'), '退化点在右/下边缘必须走内侧扩展分支');
  assert.ok(axisBody.includes('return [Math.max(0, limit - 1), limit];'), '右/下边缘退化必须向内侧扩成 1x1');
  assert.ok(axisBody.includes('return [low, Math.min(limit, low + 1)];'), '非边缘退化必须向高端扩成 1x1');
  // 行为属性：反选统一、越界夹紧、四角与右/下边缘退化都保持最小 1x1 且不越界。
  const rev = normalizeSelection({ x: 400, y: 300 }, { x: 100, y: 50 }, bounds);
  assert.deepEqual(selectionValues(rev), { left: 100, top: 50, right: 400, bottom: 300, width: 300, height: 250 });
  const clamped = normalizeSelection({ x: -10, y: 610 }, { x: 810, y: -10 }, bounds);
  assert.deepEqual(selectionValues(clamped), { left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 });
  for (const corner of [{ x: 0, y: 0 }, { x: 800, y: 0 }, { x: 0, y: 600 }, { x: 800, y: 600 }]) {
    const s = normalizeSelection(corner, corner, bounds);
    assert.equal(s.width, 1, '退化点必须保持最小宽度 1');
    assert.equal(s.height, 1, '退化点必须保持最小高度 1');
    assert.ok(s.left >= 0 && s.top >= 0 && s.right <= 800 && s.bottom <= 600, '退化点不得越界');
  }
});

test('normalizeSelection 拒绝无效显示器尺寸', () => {
  assert.throws(
    () => normalizeSelection({ x: 0, y: 0 }, { x: 1, y: 1 }, { width: 0, height: 100 }),
    /边界尺寸必须为正数/
  );
});

test('isCurrentGesture 拒绝已被后续操作取代的异步结果', () => {
  assert.equal(isCurrentGesture(7, 7), true);
  assert.equal(isCurrentGesture(7, 8), false);
});

test('squareSelection 正向拖拽时以较长边为边长生成正方形', () => {
  const selection = squareSelection({ x: 100, y: 100 }, { x: 300, y: 180 }, bounds);
  assert.deepEqual(selectionValues(selection), {
    left: 100,
    top: 100,
    right: 300,
    bottom: 300,
    width: 200,
    height: 200,
  });
});

test('squareSelection 反向拖拽时沿反方向扩展正方形', () => {
  const selection = squareSelection({ x: 300, y: 240 }, { x: 100, y: 180 }, bounds);
  assert.deepEqual(selectionValues(selection), {
    left: 100,
    top: 40,
    right: 300,
    bottom: 240,
    width: 200,
    height: 200,
  });
});

test('squareSelection 起点贴边反向拖拽不越界到负坐标', () => {
  // 起点在屏幕左/上边缘反向拖拽时，left/top 不得越界到 -1。
  const selection = squareSelection({ x: 0, y: 0 }, { x: -100, y: -100 }, bounds);
  assert.equal(selection.left, 0);
  assert.equal(selection.top, 0);
  assert.equal(selection.width, 1);
  assert.equal(selection.height, 1);
  const rightEdge = squareSelection({ x: 799, y: 0 }, { x: 700, y: -100 }, bounds);
  assert.equal(rightEdge.left, 798);
  assert.equal(rightEdge.top, 0);
});

test('squareSelection 起点贴近右下边界时边长被可达范围夹紧', () => {
  const selection = squareSelection({ x: 5, y: 5 }, { x: 1000, y: 1000 }, bounds);
  assert.deepEqual(selectionValues(selection), {
    left: 5,
    top: 5,
    right: 600,
    bottom: 600,
    width: 595,
    height: 595,
  });
});

test('squareSelection 位移为零时保持最小 1px 正方形', () => {
  const selection = squareSelection({ x: 400, y: 300 }, { x: 400, y: 300 }, bounds);
  assert.deepEqual(selectionValues(selection), {
    left: 400,
    top: 300,
    right: 401,
    bottom: 301,
    width: 1,
    height: 1,
  });
});

test('squareSelection 拒绝无效输入', () => {
  assert.throws(() => squareSelection({ x: Number.NaN, y: 1 }, { x: 2, y: 2 }, bounds), /起点 x 必须是有限数字/);
  assert.throws(() => squareSelection({ x: 1, y: 1 }, { x: 2, y: 2 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});

test('selectionForPointerGesture 单击时选择鼠标下最上层窗口', () => {
  const selection = selectionForPointerGesture(
    { x: 180, y: 160 },
    { x: 181, y: 159 },
    bounds,
    [
      { left: 100, top: 100, right: 500, bottom: 400 },
      { left: 150, top: 120, right: 350, bottom: 300 },
    ]
  );

  assert.deepEqual(selectionValues(selection), {
    left: 100,
    top: 100,
    right: 500,
    bottom: 400,
    width: 400,
    height: 300,
  });
});

test('selectionForPointerGesture 拖动超过阈值时保持用户自由选区', () => {
  const selection = selectionForPointerGesture(
    { x: 180, y: 160 },
    { x: 310, y: 260 },
    bounds,
    [{ left: 100, top: 100, right: 500, bottom: 400 }]
  );

  assert.deepEqual(selectionValues(selection), {
    left: 180,
    top: 160,
    right: 310,
    bottom: 260,
    width: 130,
    height: 100,
  });
});

test('selectionFromPhysical 将原生窗口矩形还原为高 DPI 逻辑选区', () => {
  const selection = selectionFromPhysical(
    { left: 250, top: 125, right: 1000, bottom: 625 },
    1.25,
    { width: 1200, height: 800 }
  );

  assert.deepEqual(selectionValues(selection), {
    left: 200,
    top: 100,
    right: 800,
    bottom: 500,
    width: 600,
    height: 400,
  });
});

test('selectionToPhysical 使用 floor 起点和 ceil 终点覆盖完整像素范围', () => {
  const selection = {
    left: 10.2,
    top: 20.1,
    right: 100.1,
    bottom: 40.3,
    width: 89.9,
    height: 20.2,
  };

  assert.deepEqual(
    selectionToPhysical(selection, 1.25, { width: 1920, height: 1080 }),
    {
      left: 12,
      top: 25,
      right: 126,
      bottom: 51,
      width: 114,
      height: 26,
    }
  );
});

test('selectionToPhysical 源码护栏：floor 起点 ceil 终点且最小 1px 与 physicalSize 一致', () => {
  const source = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function selectionToPhysical');
  const body = source.slice(start, start + 900);
  // 源码护栏：物理取整必须是 floor 起点 + ceil 终点（选区覆盖 [left, right) 的物理像素，
  // 标签像素数与实际截图完全一致），退化点必须保留最小 1px。
  assert.ok(body.includes('Math.floor(selection.left * devicePixelRatio)'), '左缘必须向下取整');
  assert.ok(body.includes('Math.ceil(selection.right * devicePixelRatio)'), '右缘必须向上取整');
  assert.ok(body.includes('Math.floor(selection.top * devicePixelRatio)'), '上缘必须向下取整');
  assert.ok(body.includes('Math.ceil(selection.bottom * devicePixelRatio)'), '下缘必须向上取整');
  assert.ok(body.includes('Math.max(1, end - start)'), '退化点必须保留最小 1px');
  // 行为一致性：非退化选区下 selectionToPhysical 的宽高必须与 physicalSize 完全一致。
  const cases = [
    { left: 10.2, top: 20.1, right: 100.1, bottom: 40.3, dpr: 1.25 },
    { left: 3, top: 7, right: 200.5, bottom: 120.4, dpr: 1.5 },
    { left: 100.4, top: 50.6, right: 200.2, bottom: 120.8, dpr: 2 },
    { left: 0, top: 0, right: 800, bottom: 600, dpr: 1 },
  ];
  for (const c of cases) {
    const selection = { left: c.left, top: c.top, right: c.right, bottom: c.bottom, width: c.right - c.left, height: c.bottom - c.top };
    const physical = selectionToPhysical(selection, c.dpr, { width: 1920, height: 1080 });
    const sized = physicalSize(selection, c.dpr);
    assert.equal(physical.width, sized.width, '物理宽度必须与尺寸标签一致');
    assert.equal(physical.height, sized.height, '物理高度必须与尺寸标签一致');
  }
});

test('selectionToPhysical 不允许越过物理显示器边界', () => {
  const selection = {
    left: 799,
    top: 599,
    right: 800,
    bottom: 600,
    width: 1,
    height: 1,
  };

  assert.deepEqual(
    selectionToPhysical(selection, 2, { width: 1000, height: 1000 }),
    {
      left: 998,
      top: 998,
      right: 1000,
      bottom: 1000,
      width: 2,
      height: 2,
    }
  );
});

test('hitSelectionInterior 识别选区内部点并排除边缘与外部', () => {
  const selection = { left: 100, top: 80, right: 300, bottom: 240 };
  assert.equal(hitSelectionInterior({ x: 200, y: 160 }, selection), true);
  assert.equal(hitSelectionInterior({ x: 104, y: 84 }, selection), true);
  assert.equal(hitSelectionInterior({ x: 102, y: 160 }, selection), false);
  assert.equal(hitSelectionInterior({ x: 200, y: 82 }, selection), false);
  assert.equal(hitSelectionInterior({ x: 400, y: 160 }, selection), false);
});

test('hitSelectionInterior 自定义内部边距并可调大', () => {
  const selection = { left: 100, top: 80, right: 300, bottom: 240 };
  assert.equal(hitSelectionInterior({ x: 120, y: 100 }, selection, 8), true);
  assert.equal(hitSelectionInterior({ x: 106, y: 100 }, selection, 8), false);
});

test('hitSelectionInterior 拒绝无效点、选区或负边距', () => {
  assert.throws(() => hitSelectionInterior({ x: Number.NaN, y: 1 }, { left: 0, top: 0, right: 1, bottom: 1 }), /点 x 必须是有限数字/);
  assert.throws(() => hitSelectionInterior({ x: 1, y: 1 }, { left: 0, top: 0, right: 1 }, 0), /selection.bottom 必须是有限数字/);
  assert.throws(() => hitSelectionInterior({ x: 1, y: 1 }, { left: 0, top: 0, right: 1, bottom: 1 }, -1), /内部边距不能为负数/);
});

test('hitSelectionEdge 命中边缘并返回方向, 内部点返回 null', () => {
  const selection = { left: 100, top: 80, right: 300, bottom: 240 };
  assert.equal(hitSelectionEdge({ x: 102, y: 160 }, selection), 'w');
  assert.equal(hitSelectionEdge({ x: 298, y: 160 }, selection), 'e');
  assert.equal(hitSelectionEdge({ x: 200, y: 82 }, selection), 'n');
  assert.equal(hitSelectionEdge({ x: 200, y: 238 }, selection), 's');
  assert.equal(hitSelectionEdge({ x: 102, y: 82 }, selection), 'nw');
  assert.equal(hitSelectionEdge({ x: 298, y: 238 }, selection), 'se');
  assert.equal(hitSelectionEdge({ x: 200, y: 160 }, selection), null);
  assert.equal(hitSelectionEdge({ x: 50, y: 160 }, selection), null);
});

test('hitSelectionEdge 默认容差 4 且角点按 n/s 在前 e/w 在后并排除严格内部点', () => {
  const source = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function hitSelectionEdge');
  const body = source.slice(start, start + 1300);
  // 源码护栏：默认边缘容差必须为 4（ShareX 贴近的边缘探测阈值）。
  assert.ok(body.includes('tolerance = 4'), '默认边缘容差必须为 4');
  // 角点方向命名约定：n/s 在前 e/w 在后（nw/ne/sw/se），禁止 w 在前产生 wn/we 混淆。
  assert.ok(body.includes("edges.sort((a, b) => 'nsew'.indexOf(a) - 'nsew'.indexOf(b))"), '角点必须按 n/s 在前 e/w 在后排序');
  // 严格内部点必须排除（距四边都超过容差时留作平移，不进入调整模式）。
  assert.ok(body.includes('point.x > selection.left + tolerance'), '必须排除严格内部点');
  // 行为属性：默认容差 4 下边缘/角点命中、内部与外部返回 null、角点顺序固定。
  const selection = { left: 100, top: 80, right: 300, bottom: 240 };
  assert.equal(hitSelectionEdge({ x: 104, y: 160 }, selection), 'w', '容差内左缘必须命中');
  assert.equal(hitSelectionEdge({ x: 105, y: 160 }, selection), null, '容差外左缘不得命中');
  assert.equal(hitSelectionEdge({ x: 104, y: 84 }, selection), 'nw', '左上角必须为 nw');
  assert.equal(hitSelectionEdge({ x: 296, y: 236 }, selection), 'se', '右下角必须为 se');
  assert.equal(hitSelectionEdge({ x: 200, y: 160 }, selection), null, '严格内部点必须返回 null');
  assert.equal(hitSelectionEdge({ x: 95, y: 160 }, selection), null, '容差外外部点必须返回 null');
});

test('hitSelectionEdge 自定义容差并拒绝负容差', () => {
  const selection = { left: 100, top: 80, right: 300, bottom: 240 };
  assert.equal(hitSelectionEdge({ x: 108, y: 160 }, selection, 8), 'w');
  assert.equal(hitSelectionEdge({ x: 200, y: 160 }, selection, 8), null);
  assert.throws(() => hitSelectionEdge({ x: 1, y: 1 }, selection, -1), /边缘容差不能为负数/);
});

test('resizeSelection 从中心缩放时拖动右边界左右对称扩展', () => {
  const resized = resizeSelection(
    { left: 100, top: 100, right: 300, bottom: 300 },
    'e',
    { x: 350, y: 200 },
    bounds,
    { fromCenter: true }
  );
  assert.deepEqual(resized, { left: 50, top: 100, right: 350, bottom: 300, width: 300, height: 200 });
});

test('resizeSelection 从中心缩放时拖动左边界对称收窄', () => {
  const resized = resizeSelection(
    { left: 100, top: 100, right: 300, bottom: 300 },
    'w',
    { x: 150, y: 200 },
    bounds,
    { fromCenter: true }
  );
  assert.deepEqual(resized, { left: 150, top: 100, right: 250, bottom: 300, width: 100, height: 200 });
});

test('resizeSelection 从中心缩放时拖动下边界上下对称扩展', () => {
  const resized = resizeSelection(
    { left: 100, top: 100, right: 300, bottom: 300 },
    's',
    { x: 200, y: 400 },
    bounds,
    { fromCenter: true }
  );
  assert.deepEqual(resized, { left: 100, top: 0, right: 300, bottom: 400, width: 200, height: 400 });
});

test('resizeSelection 从中心缩放时拖动角点宽高都对称扩展', () => {
  const resized = resizeSelection(
    { left: 100, top: 100, right: 300, bottom: 300 },
    'se',
    { x: 350, y: 400 },
    bounds,
    { fromCenter: true }
  );
  assert.deepEqual(resized, { left: 50, top: 0, right: 350, bottom: 400, width: 300, height: 400 });
});

test('resizeSelection 从中心缩放时整体夹紧到显示器边界', () => {
  const localBounds = { width: 1920, height: 1080 };
  const resized = resizeSelection(
    { left: 100, top: 100, right: 300, bottom: 300 },
    'e',
    { x: 2000, y: 200 },
    localBounds,
    { fromCenter: true }
  );
  assert.deepEqual(resized, { left: 0, top: 100, right: 1920, bottom: 300, width: 1920, height: 200 });
});

test('resizeSelection 从中心缩放且保持比例时中心固定', () => {
  const resized = resizeSelection(
    { left: 100, top: 100, right: 300, bottom: 300 },
    'e',
    { x: 350, y: 200 },
    bounds,
    { keepAspectRatio: true, fromCenter: true }
  );
  assert.deepEqual(resized, { left: 75, top: 75, right: 325, bottom: 325, width: 250, height: 250 });
});

test('resizeSelection 拖动右边界向右增大宽度', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    'e',
    { x: 400, y: 200 },
    bounds
  );
  assert.deepEqual(selectionValues(resized), {
    left: 100,
    top: 80,
    right: 400,
    bottom: 240,
    width: 300,
    height: 160,
  });
});

test('resizeSelection 拖动左边界向左扩展且不翻转', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    'w',
    { x: 50, y: 160 },
    bounds
  );
  assert.deepEqual(selectionValues(resized), {
    left: 50,
    top: 80,
    right: 300,
    bottom: 240,
    width: 250,
    height: 160,
  });
});

test('resizeSelection 拖动角点同时调整两个方向', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    'nw',
    { x: 80, y: 60 },
    bounds
  );
  assert.deepEqual(selectionValues(resized), {
    left: 80,
    top: 60,
    right: 300,
    bottom: 240,
    width: 220,
    height: 180,
  });
});

test('resizeSelection 越过对边时夹到最小 1px 且边界夹紧', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    'e',
    { x: 50, y: 160 },
    bounds
  );
  assert.deepEqual(selectionValues(resized), {
    left: 100,
    top: 80,
    right: 101,
    bottom: 240,
    width: 1,
    height: 160,
  });
});

test('resizeSelection 保持比例时拖动右边界按宽高比调整高度', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    'e',
    { x: 400, y: 160 },
    bounds,
    { keepAspectRatio: true }
  );
  assert.deepEqual(selectionValues(resized), {
    left: 100,
    top: 80,
    right: 400,
    bottom: 320,
    width: 300,
    height: 240,
  });
});

test('resizeSelection 保持比例时拖动下边界按比例调整宽度', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    's',
    { x: 200, y: 400 },
    bounds,
    { keepAspectRatio: true }
  );
  assert.deepEqual(selectionValues(resized), {
    left: 100,
    top: 80,
    right: 500,
    bottom: 400,
    width: 400,
    height: 320,
  });
});

test('resizeSelection 保持比例时拖动角点取长轴驱动另一维', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    'se',
    { x: 600, y: 340 },
    bounds,
    { keepAspectRatio: true }
  );
  assert.deepEqual(selectionValues(resized), {
    left: 100,
    top: 80,
    right: 600,
    bottom: 480,
    width: 500,
    height: 400,
  });
});

test('resizeSelection 保持比例时整体夹紧到显示器边界', () => {
  const localBounds = { width: 2200, height: 1120 };
  const resized = resizeSelection(
    { left: 1700, top: 1000, right: 1900, bottom: 1100, width: 200, height: 100 },
    'e',
    { x: 2000, y: 1050 },
    localBounds,
    { keepAspectRatio: true }
  );
  assert.deepEqual(selectionValues(resized), {
    left: 1700,
    top: 970,
    right: 2000,
    bottom: 1120,
    width: 300,
    height: 150,
  });
});

test('resizeSelection 不开启比例时保持原有自由调整行为', () => {
  const resized = resizeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    'e',
    { x: 400, y: 160 },
    bounds
  );
  assert.deepEqual(selectionValues(resized), {
    left: 100,
    top: 80,
    right: 400,
    bottom: 240,
    width: 300,
    height: 160,
  });
});

test('resizeSelection 拒绝非法边缘或无效输入', () => {
  assert.throws(() => resizeSelection({ left: 0, top: 0, right: 1, bottom: 1 }, 'x', { x: 1, y: 1 }, bounds), /edge 必须是 n\/s\/e\/w 组合/);
  assert.throws(() => resizeSelection({ left: 0, top: 0, right: 1, bottom: 1 }, '', { x: 1, y: 1 }, bounds), /edge 必须是 n\/s\/e\/w 组合/);
  assert.throws(() => resizeSelection({ left: 0, top: 0, right: 1, bottom: 1 }, 'e', { x: 1, y: Number.NaN }, bounds), /点 y 必须是有限数字/);
});

test('nudgeSelection 按 1px 移动选区并保持尺寸不变', () => {
  const moved = nudgeSelection(
    { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 },
    1,
    1,
    bounds
  );

  assert.deepEqual(selectionValues(moved), {
    left: 101,
    top: 81,
    right: 301,
    bottom: 241,
    width: 200,
    height: 160,
  });
});

test('nudgeSelection 在边界处夹紧且不改变尺寸', () => {
  const moved = nudgeSelection(
    { left: 0, top: 0, right: 200, bottom: 160, width: 200, height: 160 },
    -5,
    -5,
    bounds
  );

  assert.deepEqual(selectionValues(moved), {
    left: 0,
    top: 0,
    right: 200,
    bottom: 160,
    width: 200,
    height: 160,
  });
});

test('nudgeSelection 支持 10px 快速步长并夹在右下方边界', () => {
  const moved = nudgeSelection(
    { left: 700, top: 500, right: 780, bottom: 580, width: 80, height: 80 },
    10,
    10,
    bounds
  );

  assert.deepEqual(selectionValues(moved), {
    left: 710,
    top: 510,
    right: 790,
    bottom: 590,
    width: 80,
    height: 80,
  });
});

test('nudgeSelection 尺寸守恒且边界按尺寸推导并先归一化', () => {
  const source = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function nudgeSelection');
  const body = source.slice(start, start + 1000);
  // 源码护栏：必须先归一化当前选区（脏输入如 left>right 也能得到合法矩形），
  // 边界必须按选区尺寸推导（maxLeft = bounds.width - current.width，贴边时 clamp 才不越界）。
  assert.ok(body.includes('const current = normalizeSelection('), '微调前必须先归一化当前选区');
  assert.ok(body.includes('const maxLeft = Math.max(0, bounds.width - current.width);'), '横向边界必须按尺寸推导');
  assert.ok(body.includes('const maxTop = Math.max(0, bounds.height - current.height);'), '纵向边界必须按尺寸推导');
  assert.ok(body.includes('right: left + current.width'), '右边缘必须由左边缘加宽度得到');
  assert.ok(body.includes('bottom: top + current.height'), '下边缘必须由上边缘加高度得到');
  // 行为属性：任意位移下尺寸守恒（width/height 不变），且各方向都贴边界时夹紧不越界。
  const base = { left: 100, top: 80, right: 300, bottom: 240, width: 200, height: 160 };
  for (const [dx, dy] of [[1, 1], [-5, -5], [10, -10], [500, 500], [-500, -500]]) {
    const moved = nudgeSelection(base, dx, dy, bounds);
    assert.equal(moved.width, 200, '移动不得改变宽度');
    assert.equal(moved.height, 160, '移动不得改变高度');
    assert.ok(moved.left >= 0 && moved.top >= 0 && moved.right <= 800 && moved.bottom <= 600, '移动不得越界');
  }
  const corner = nudgeSelection({ left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 }, 10, 10, bounds);
  assert.deepEqual(selectionValues(corner), { left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 }, '占满边界时移动必须原地夹紧');
});

test('nudgeSelection 拒绝无效边界或非有限位移', () => {
  assert.throws(
    () => nudgeSelection({ left: 0, top: 0, right: 1, bottom: 1 }, 1, 1, { width: 0, height: 100 }),
    /边界尺寸必须为正数/
  );
  assert.throws(
    () => nudgeSelection({ left: 0, top: 0, right: 1, bottom: 1 }, Number.NaN, 1, bounds),
    /横向位移 必须是有限数字/
  );
});

test('全部选区变换函数必须夹紧到显示器边界内', () => {
  const source = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  // 拖拽/调整/移动/方形四种变换都必须以边界为约束，禁止裸输出越界矩形。
  const sections = [
    ['normalizeSelection', 'normalizeAxis(start.x, end.x, bounds.width)'],
    ['squareSelection', 'bounds.height - side'],
    ['resizeSelection', 'clamp(point.x, 0, bounds.width)'],
    ['nudgeSelection', 'clamp(current.left + dx, 0, maxLeft)'],
  ];
  for (const [name, marker] of sections) {
    const start = source.indexOf(`export function ${name}`);
    assert.ok(start !== -1, `缺少变换函数 ${name}`);
    const tail = source.slice(start);
    const endOffset = tail.indexOf('\nexport function ');
    const body = endOffset === -1 ? tail : tail.slice(0, endOffset);
    assert.ok(body.includes(marker), `${name} 必须把结果夹紧到边界内（${marker}）`);
  }
});

test('createRafWriter 源码代数递增且 cancel 复位调度标志并丢弃过期帧', () => {
  const source = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function createRafWriter');
  const body = source.slice(start, start + 900);
  // 源码护栏一：cancel 必须递增代数（generation += 1），过期帧通过代数比较丢弃。
  assert.ok(body.includes('generation += 1;'), 'cancel 必须递增代数');
  // 源码护栏二：帧回调必须比较代数，不匹配时丢弃且不触碰共享 pendingValue。
  assert.ok(body.includes('if (scheduledGeneration !== generation) {'), '帧回调必须比较代数');
  // 源码护栏三：cancel 必须复位调度标志，否则 cancel 后同帧内再次 schedule 会静默丢值。
  assert.ok(body.includes('scheduled = false;'), 'cancel 必须复位调度标志');
  // 行为属性：代数递增后旧帧丢弃、cancel 后同帧再调度不丢值。
  const callbacks = [];
  const writes = [];
  const writer = createRafWriter(
    (value) => writes.push(value),
    (callback) => { callbacks.push(callback); return callbacks.length; }
  );
  writer.schedule('one');
  writer.schedule('two');
  assert.equal(callbacks.length, 1, '同帧多次调度必须只排一帧');
  callbacks.shift()();
  assert.deepEqual(writes, ['two'], '一帧内必须只提交最后一次');
  writer.schedule('three');
  writer.cancel();
  writer.schedule('four');
  callbacks.shift()();
  assert.deepEqual(writes, ['two'], 'cancel 前旧帧必须丢弃');
  callbacks.shift()();
  assert.deepEqual(writes, ['two', 'four'], 'cancel 后新调度必须生效');
});

test('createRafWriter 在一帧内只提交最后一次几何', () => {
  const callbacks = [];
  const writes = [];
  const writer = createRafWriter(
    (value) => writes.push(value),
    (callback) => {
      callbacks.push(callback);
      return callbacks.length;
    }
  );

  writer.schedule('first');
  writer.schedule('second');
  assert.equal(callbacks.length, 1);
  assert.deepEqual(writes, []);

  callbacks.shift()();
  assert.deepEqual(writes, ['second']);

  writer.schedule('third');
  callbacks.shift()();
  assert.deepEqual(writes, ['second', 'third']);
});

test('createRafWriter cancel 后同帧内再次调度不丢值', () => {
  const callbacks = [];
  const writes = [];
  const writer = createRafWriter(
    (value) => writes.push(value),
    (callback) => {
      callbacks.push(callback);
      return callbacks.length;
    }
  );

  writer.schedule('first');
  writer.cancel();
  writer.schedule('second');
  // 第一帧回调是 cancel 前的过期帧：必须被丢弃。
  callbacks.shift()();
  assert.deepEqual(writes, []);
  // 第二帧回调写入 cancel 后的新值，不能因 cancel 复位调度标志而丢值。
  callbacks.shift()();
  assert.deepEqual(writes, ['second']);
});

test('createRafWriter 取消后不再写入过期几何', () => {
  const callbacks = [];
  const writes = [];
  const writer = createRafWriter(
    (value) => writes.push(value),
    (callback) => {
      callbacks.push(callback);
      return callbacks.length;
    }
  );

  writer.schedule('stale');
  writer.cancel();
  callbacks.shift()();

  assert.deepEqual(writes, []);
});
