import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  distanceTo,
  shouldClosePolygon,
  isPolygonClosed,
  polygonPhysicalVertices,
  pointInPolygon,
  polygonBounds,
  polygonPathPoints,
} from './polygonModel.js';

test('distanceTo 计算两点欧氏距离', () => {
  assert.equal(distanceTo({ x: 0, y: 0 }, { x: 3, y: 4 }), 5);
  assert.equal(distanceTo({ x: 10, y: 10 }, { x: 10, y: 10 }), 0);
});

test('shouldClosePolygon 靠近首锚点（容差内）闭合', () => {
  const points = [{ x: 100, y: 100 }, { x: 200, y: 120 }, { x: 180, y: 220 }];
  assert.equal(shouldClosePolygon({ x: 102, y: 104 }, points, 8), true);
  assert.equal(shouldClosePolygon({ x: 115, y: 110 }, points, 8), false);
  assert.equal(shouldClosePolygon({ x: 100, y: 100 }, points), true, '默认容差 8 内必闭合');
  // 空顶点列表不得闭合。
  assert.equal(shouldClosePolygon({ x: 0, y: 0 }, [], 8), false);
});

test('isPolygonClosed 顶点不足 3 个不算闭合', () => {
  assert.equal(isPolygonClosed([{ x: 1, y: 1 }, { x: 2, y: 2 }]), false);
  assert.equal(isPolygonClosed([{ x: 1, y: 1 }, { x: 2, y: 2 }, { x: 3, y: 1 }]), true);
});

test('polygonPhysicalVertices 逻辑坐标转物理像素且四舍五入一致', () => {
  const verts = polygonPhysicalVertices([
    { x: 100, y: 80 },
    { x: 200, y: 160 },
    { x: 150, y: 240 },
  ], 1.25);
  assert.deepEqual(verts, [
    { x: 125, y: 100 },
    { x: 250, y: 200 },
    { x: 187, y: 300 },
  ]);
  // 多边形完成路径另存 {left, top} 结构（与 CSS --selection-polygon 同构），
  // 必须兼容读取——若只读 point.x 会拿到 undefined → NaN 顶点。
  const leftTop = polygonPhysicalVertices([
    { left: 100, top: 80 },
    { left: 200, top: 160 },
    { left: 150, top: 240 },
  ], 1.25);
  assert.deepEqual(leftTop, [
    { x: 125, y: 100 },
    { x: 250, y: 200 },
    { x: 187, y: 300 },
  ]);
  assert.throws(() => polygonPhysicalVertices([{ x: 0, y: 0 }, { x: 1, y: 1 }], 1.25), /至少需要 3/);
  assert.throws(() => polygonPhysicalVertices([{ x: 0, y: 0 }, { x: 1, y: 1 }, { x: 2, y: 2 }], 0), /devicePixelRatio/);
});

test('pointInPolygon 射线法判定内部与外部点', () => {
  const square = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }, { x: 0, y: 100 }];
  assert.equal(pointInPolygon({ x: 50, y: 50 }, square), true);
  assert.equal(pointInPolygon({ x: 150, y: 50 }, square), false);
  assert.equal(pointInPolygon({ x: 50, y: 150 }, square), false);
  const concave = [{ x: 0, y: 0 }, { x: 100, y: 0 }, { x: 100, y: 100 }, { x: 60, y: 40 }, { x: 0, y: 100 }];
  assert.equal(pointInPolygon({ x: 20, y: 60 }, concave), true);
  assert.equal(pointInPolygon({ x: 80, y: 60 }, concave), true, '凹多边形内点（主轮廓内但凹入侧以外）也必须判内');
  assert.equal(pointInPolygon({ x: 60, y: 60 }, concave), false, '凹入缺口内的点必须判外');
  assert.equal(pointInPolygon({ x: 0, y: 0 }, [{ x: 0, y: 0 }, { x: 1, y: 1 }]), false, '不足 3 点必须返回 false');
});

test('polygonBounds 返回轴对齐包围盒', () => {
  const rect = polygonBounds([{ x: 120, y: 80 }, { x: 200, y: 300 }, { x: 60, y: 240 }, { x: 160, y: 50 }]);
  assert.deepEqual(rect, { left: 60, top: 50, right: 200, bottom: 300, width: 140, height: 250 });
  assert.equal(polygonBounds([{ x: 0, y: 0 }, { x: 100, y: 100 }, { x: 100, y: 100 }]), null, '退化多边形必须返回 null');
  assert.equal(polygonBounds([{ x: 0, y: 0 }, { x: 1, y: 1 }]), null, '顶点不足必须返回 null');
});

test('polygonPathPoints 深拷贝顶点防共享引用污染', () => {
  const points = [{ x: 10, y: 20 }, { x: 30, y: 40 }];
  const copy = polygonPathPoints(points);
  copy[0].x = 999;
  assert.equal(points[0].x, 10, '修改拷贝不得影响原数组');
  assert.deepEqual(polygonPathPoints(points), [{ x: 10, y: 20 }, { x: 30, y: 40 }]);
});

test('polygonModel 源码护栏：闭合判定用射线法/射线命中与包围盒存在', () => {
  const source = readFileSync(new URL('./polygonModel.js', import.meta.url), 'utf8');
  assert.ok(source.includes('export function pointInPolygon'), '必须提供多边形命中测试');
  assert.ok(source.includes('export function polygonBounds'), '必须提供包围盒');
  assert.ok(source.includes('export function polygonPhysicalVertices'), '必须提供物理顶点转换');
  assert.ok(source.includes('Math.hypot'), '闭合判定必须用欧氏距离');
  // 射线法奇数交叉规则：内部计数翻转。
  assert.ok(source.includes('inside = !inside'), '命中测试必须用射线法交叉规则');
});
