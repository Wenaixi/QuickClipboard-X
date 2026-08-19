import { test } from 'node:test';
import assert from 'node:assert/strict';
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
