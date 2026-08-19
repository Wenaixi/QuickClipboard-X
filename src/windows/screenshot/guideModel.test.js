import { test } from 'node:test';
import assert from 'node:assert/strict';
import { guideLines } from './guideModel.js';

const bounds = { width: 1920, height: 1080 };

test('guideLines 光标居中时垂直线贯穿高度且水平线贯穿宽度', () => {
  const lines = guideLines({ x: 960, y: 540 }, bounds);
  assert.deepEqual(lines.vertical, { left: 960, top: 0, width: 1, height: 1080 });
  assert.deepEqual(lines.horizontal, { left: 0, top: 540, width: 1920, height: 1 });
});

test('guideLines 光标越出边界时夹紧到边界内', () => {
  const lines = guideLines({ x: -50, y: 5000 }, bounds);
  assert.equal(lines.vertical.left, 0);
  assert.equal(lines.horizontal.top, 1080);
  assert.equal(lines.vertical.height, 1080);
  assert.equal(lines.horizontal.width, 1920);
});

test('guideLines 拒绝无效坐标或边界', () => {
  assert.throws(() => guideLines({ x: Number.NaN, y: 1 }, bounds), /点 x 必须是有限数字/);
  assert.throws(() => guideLines({ x: 1, y: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
});
