import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { layerToRenderCommands, flattenRenderCommands } from './renderModel.js';

test('layerToRenderCommands 把类型参数转渲染指令', () => {
  const rect = layerToRenderCommands({ type: 'rect', params: { x: 10, y: 20, w: 100, h: 50, stroke: '#f00', fill: 'rgba(0,0,0,0)' } });
  assert.deepEqual(rect, [{ type: 'rect', x: 10, y: 20, w: 100, h: 50, stroke: '#f00', fill: 'rgba(0,0,0,0)' }]);
  const pen = layerToRenderCommands({ type: 'pen', params: { points: [{ x: 1, y: 2 }], stroke: '#000', width: 2 } });
  assert.equal(pen[0].type, 'polyline');
  assert.ok(pen[0].points.length === 1);
});

test('layerToRenderCommands 未知类型必须抛错', () => {
  assert.throws(() => layerToRenderCommands({ type: 'nope', params: {} }), /不支持的渲染类型/);
  assert.throws(() => layerToRenderCommands(null), /图层对象/);
});

test('flattenRenderCommands 扁平化所有图层且探测裁剪指令', () => {
  const flat = flattenRenderCommands([
    { type: 'rect', params: { x: 0, y: 0, w: 10, h: 10 } },
    { type: 'crop', params: { x: 0, y: 0, w: 5, h: 5 } },
  ]);
  assert.equal(flat.commands.length, 2);
  assert.equal(flat.hasCrop, true, '有 crop 指令必须标记');
  const noCrop = flattenRenderCommands([{ type: 'line', params: { x1: 0, y1: 0, x2: 9, y2: 9 } }]);
  assert.equal(noCrop.hasCrop, false);
});

test('renderModel 源码护栏：指令表全类型 + 扁平化存在', () => {
  const source = readFileSync(new URL('./renderModel.js', import.meta.url), 'utf8');
  assert.ok(source.includes('export const RENDER_COMMANDS'), '必须声明渲染指令表');
  assert.ok(source.includes('arrow:'), '指令表必须含箭头');
  assert.ok(source.includes('mosaic:'), '指令表必须含马赛克');
  assert.ok(source.includes('crop:'), '指令表必须含裁剪');
  assert.ok(source.includes('flattenRenderCommands'), '必须提供扁平化');
});