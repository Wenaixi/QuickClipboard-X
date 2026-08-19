import { test } from 'node:test';
import assert from 'node:assert/strict';
import { selectionHandles } from './handleModel.js';

test('selectionHandles 输出八个角点与边中点手柄', () => {
  const handles = selectionHandles({ left: 100, top: 100, right: 400, bottom: 300 });
  assert.equal(handles.length, 8);
  assert.deepEqual(handles.find((h) => h.edge === 'nw'), { edge: 'nw', left: 100, top: 100 });
  assert.deepEqual(handles.find((h) => h.edge === 'ne'), { edge: 'ne', left: 400, top: 100 });
  assert.deepEqual(handles.find((h) => h.edge === 'se'), { edge: 'se', left: 400, top: 300 });
  assert.deepEqual(handles.find((h) => h.edge === 'sw'), { edge: 'sw', left: 100, top: 300 });
  assert.deepEqual(handles.find((h) => h.edge === 'n'), { edge: 'n', left: 250, top: 100 });
  assert.deepEqual(handles.find((h) => h.edge === 'e'), { edge: 'e', left: 400, top: 200 });
  assert.deepEqual(handles.find((h) => h.edge === 's'), { edge: 's', left: 250, top: 300 });
  assert.deepEqual(handles.find((h) => h.edge === 'w'), { edge: 'w', left: 100, top: 200 });
});

test('selectionHandles 奇数尺寸时边中点取半像素坐标', () => {
  const handles = selectionHandles({ left: 100, top: 100, right: 401, bottom: 301 });
  assert.deepEqual(handles.find((h) => h.edge === 'e'), { edge: 'e', left: 401, top: 200.5 });
  assert.deepEqual(handles.find((h) => h.edge === 's'), { edge: 's', left: 250.5, top: 301 });
});

test('selectionHandles 单像素选区仍输出全部手柄', () => {
  const handles = selectionHandles({ left: 100, top: 100, right: 101, bottom: 101 });
  assert.equal(handles.length, 8);
  assert.equal(handles.find((h) => h.edge === 'nw').left, 100);
  assert.equal(handles.find((h) => h.edge === 'se').left, 101);
  assert.equal(handles.find((h) => h.edge === 'e').top, 100.5);
});

test('selectionHandles 拒绝无效输入', () => {
  assert.throws(() => selectionHandles(null), /选区/);
  assert.throws(() => selectionHandles({ left: 0, top: 0, right: 1, bottom: Number.NaN }), /bottom 必须是有限数字/);
});
