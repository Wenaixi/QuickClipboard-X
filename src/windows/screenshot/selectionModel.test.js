import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  createRafWriter,
  isCurrentGesture,
  normalizeSelection,
  selectionForPointerGesture,
  selectionFromPhysical,
  selectionToPhysical,
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
