import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

test('selectionHandles 源码八手柄数据驱动且顺序固定', () => {
  const source = readFileSync(new URL('./handleModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function selectionHandles');
  const body = source.slice(start, start + 800);
  // 源码护栏：八个手柄必须由数据数组驱动（禁止手工逐行渲染），角点与边中点公式必须存在。
  assert.ok(body.includes("const centerX = (left + right) / 2;"), '中心 x 必须为两角均值');
  assert.ok(body.includes("const centerY = (top + bottom) / 2;"), '中心 y 必须为两角均值');
  assert.ok(body.includes("{ edge: 'nw', left, top }"), 'nw 角点必须落在左上角');
  assert.ok(body.includes("{ edge: 'n', left: centerX, top }"), 'n 边中点必须水平居中');
  assert.ok(body.includes("{ edge: 'ne', left: right, top }"), 'ne 角点必须落在右上角');
  assert.ok(body.includes("{ edge: 'e', left: right, top: centerY }"), 'e 边中点必须垂直居中');
  assert.ok(body.includes("{ edge: 'se', left: right, top: bottom }"), 'se 角点必须落在右下角');
  assert.ok(body.includes("{ edge: 's', left: centerX, top: bottom }"), 's 边中点必须水平居中');
  assert.ok(body.includes("{ edge: 'sw', left, top: bottom }"), 'sw 角点必须落在左下角');
  assert.ok(body.includes("{ edge: 'w', left, top: centerY }"), 'w 边中点必须垂直居中');
  // 行为属性：任意合法选区输出 8 个手柄、顺序固定 nw→w、角点精确落点、边中点精确居中。
  const selection = { left: 100, top: 200, right: 500, bottom: 400 };
  const handles = selectionHandles(selection);
  assert.deepEqual(handles.map((h) => h.edge), ['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']);
  assert.equal(handles.find((h) => h.edge === 'nw').left, 100);
  assert.equal(handles.find((h) => h.edge === 'nw').top, 200);
  assert.equal(handles.find((h) => h.edge === 'se').left, 500);
  assert.equal(handles.find((h) => h.edge === 'se').top, 400);
  assert.equal(handles.find((h) => h.edge === 'e').left, 500);
  assert.equal(handles.find((h) => h.edge === 'e').top, 300);
  assert.equal(handles.find((h) => h.edge === 'w').left, 100);
  assert.equal(handles.find((h) => h.edge === 'w').top, 300);
});

test('selectionHandles 八手柄边缘集与 resizeSelection 支持的四边完全对齐', () => {
  const handleSource = readFileSync(new URL('./handleModel.js', import.meta.url), 'utf8');
  const resizeSource = readFileSync(new URL('./selectionModel.js', import.meta.url), 'utf8');
  // 源码护栏一：resizeSelection 必须分别处理 w/e/n/s 四条边（手柄拖动调整的语义基础）。
  const resizeStart = resizeSource.indexOf('export function resizeSelection');
  const resizeBody = resizeSource.slice(resizeStart, resizeStart + 1500);
  for (const edge of ['w', 'e', 'n', 's']) {
    assert.ok(resizeBody.includes(`edge.includes('${edge}')`), `resizeSelection 必须处理 ${edge} 边`);
  }
  // 源码护栏二：手柄必须来自数据数组且每个 edge 都只能由 n/s/e/w 组成（角点为组合边）。
  const handleStart = handleSource.indexOf('export function selectionHandles');
  const handleBody = handleSource.slice(handleStart, handleStart + 700);
  assert.ok(/edge: '[nsew]+'/.test(handleBody), '手柄 edge 必须由 n/s/e/w 组成');
  // 行为属性：8 个手柄恰好覆盖 4 个角点组合 + 4 条边中点，且全部被 resizeSelection 支持。
  const edges = selectionHandles({ left: 0, top: 0, right: 100, bottom: 100 }).map((h) => h.edge);
  assert.equal(edges.length, 8);
  const expected = new Set(['nw', 'n', 'ne', 'e', 'se', 's', 'sw', 'w']);
  assert.deepEqual(new Set(edges), expected, '手柄边缘集必须完整覆盖四边组合');
  for (const edge of edges) {
    for (const ch of edge) {
      assert.ok('nsew'.includes(ch), `手柄 edge ${edge} 含非法边 ${ch}`);
    }
  }
});

test('selectionHandles 拒绝无效输入', () => {
  assert.throws(() => selectionHandles(null), /选区/);
  assert.throws(() => selectionHandles({ left: 0, top: 0, right: 1, bottom: Number.NaN }), /bottom 必须是有限数字/);
});
