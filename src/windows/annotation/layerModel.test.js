import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  createLayer,
  addLayer,
  updateLayer,
  removeLayer,
  clearLayers,
  reorderLayer,
  mergeLayers,
} from './layerModel.js';

test('createLayer 校验类型与 id 且深拷贝参数与坐标', () => {
  const layer = createLayer('l1', 'arrow', { color: '#f00' }, [{ x: 1, y: 2 }]);
  assert.equal(layer.type, 'arrow');
  assert.throws(() => createLayer('', 'rect'), /非空字符串/);
  assert.throws(() => createLayer('l', 'unknown'), /未知标注类型/);
  layer.params.color = '#000';
  layer.points[0].x = 99;
  const fresh = createLayer('l1', 'arrow', { color: '#f00' }, [{ x: 1, y: 2 }]);
  assert.equal(fresh.params.color, '#f00', '修改返回不得污染新层');
  assert.equal(fresh.points[0].x, 1);
});

test('addLayer 代际校验与追加', () => {
  const ref = { current: 5 };
  assert.throws(() => addLayer([], createLayer('a', 'rect'), 4, ref), /代际已过期/);
  const layers = addLayer([], createLayer('a', 'rect'), 5, ref);
  assert.equal(layers.length, 1);
});

test('updateLayer 只更新匹配 id 且合并参数', () => {
  let layers = [createLayer('a', 'rect', { color: '#f00' }), createLayer('b', 'line')];
  layers = updateLayer(layers, 'a', { params: { strokeWidth: 3 } });
  assert.deepEqual(layers[0].params, { color: '#f00', strokeWidth: 3 });
  assert.equal(layers[1].params.strokeWidth, undefined, '不匹配图层不得被更新');
});

test('removeLayer 删除且 clearLayers 清空', () => {
  let layers = [createLayer('a', 'rect'), createLayer('b', 'line')];
  layers = removeLayer(layers, 'a');
  assert.deepEqual(layers.map((l) => l.id), ['b']);
  assert.deepEqual(clearLayers(), []);
});

test('reorderLayer 调整 z 序且越界夹紧', () => {
  let layers = [createLayer('a', 'rect'), createLayer('b', 'line'), createLayer('c', 'text')];
  layers = reorderLayer(layers, 'c', 1);
  assert.deepEqual(layers.map((l) => l.id), ['a', 'c', 'b']);
  layers = reorderLayer(layers, 'a', 99);
  assert.deepEqual(layers.map((l) => l.id), ['c', 'b', 'a'], '越界夹紧到最上层');
  const unchanged = reorderLayer(layers, 'missing', 1);
  assert.equal(unchanged, layers, '未知 id 不得改变数组');
});

test('mergeLayers 扁平化为单层（对齐 FlattenAnnotations）', () => {
  const merged = mergeLayers();
  assert.deepEqual(merged, [{ id: 'flat', type: 'flat', params: {}, points: [] }]);
});

test('layerModel 源码护栏：图层类型枚举/代际校验/扁平化存在', () => {
  const source = readFileSync(new URL('./layerModel.js', import.meta.url), 'utf8');
  assert.ok(source.includes('export const LAYER_TYPES'), '必须声明图层类型枚举');
  assert.ok(source.includes('expectedGeneration !== generationRef.current'), '必须做代际校验');
  assert.ok(source.includes('mergeLayers'), '必须提供扁平化');
  assert.ok(source.includes('reorderLayer'), '必须提供 z 序调整');
});