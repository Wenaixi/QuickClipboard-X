import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { readCenterPixel, formatRgb, hexFromRgb } from './colorModel.js';

function rgbaPixel(data, width, x, y, r, g, b, a = 255) {
  const index = (y * width + x) * 4;
  data[index] = r;
  data[index + 1] = g;
  data[index + 2] = b;
  data[index + 3] = a;
}

test('readCenterPixel 从 RGBA 平铺数组读取中心像素', () => {
  const width = 3;
  const height = 3;
  const data = new Uint8ClampedArray(width * height * 4);
  rgbaPixel(data, width, 0, 0, 255, 0, 0);
  rgbaPixel(data, width, 1, 1, 10, 20, 30);
  rgbaPixel(data, width, 2, 2, 0, 0, 255);
  assert.deepEqual(readCenterPixel(data, width, height), { r: 10, g: 20, b: 30 });
});

test('readCenterPixel 偶数尺寸取与十字中心对齐的像素', () => {
  // 4×4 时十字中心在 (2, 2)，颜色读数必须与之对齐，避免偏上 1 像素。
  const width = 4;
  const height = 4;
  const data = new Uint8ClampedArray(width * height * 4);
  rgbaPixel(data, width, 2, 2, 7, 8, 9);
  assert.deepEqual(readCenterPixel(data, width, height), { r: 7, g: 8, b: 9 });
});

test('readCenterPixel 拒绝长度不足或非法尺寸', () => {
  assert.throws(() => readCenterPixel(new Uint8ClampedArray(8), 2, 2), /背景快照数据长度不足/);
  assert.throws(() => readCenterPixel(new Uint8ClampedArray(16), 0, 2), /宽度与高度必须为正数/);
});

test('formatRgb 输出 RGB 格式并夹紧越界通道', () => {
  assert.equal(formatRgb({ r: 255, g: 0, b: 128 }), 'RGB(255, 0, 128)');
  assert.equal(formatRgb({ r: 300, g: -5, b: 42 }), 'RGB(255, 0, 42)');
});

test('formatRgb 拒绝非法输入', () => {
  assert.throws(() => formatRgb(null), /颜色对象/);
  assert.throws(() => formatRgb({ r: 1, g: 2 }), /缺少蓝色通道/);
});

test('readCenterPixel 源码中心取整 floor 与索引公式且与放大镜十字中心对齐', () => {
  const source = readFileSync(new URL('./colorModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function readCenterPixel');
  const body = source.slice(start, start + 500);
  // 源码护栏一：中心像素必须向下取整（与放大镜十字参考线 centerX=floor(w/2) 对齐，
  // 禁止四舍五入导致偶数尺寸偏上/偏左 1 像素）。
  assert.ok(body.includes('const x = Math.floor(width / 2);'), '中心 x 必须向下取整');
  assert.ok(body.includes('const y = Math.floor(height / 2);'), '中心 y 必须向下取整');
  // 源码护栏二：平铺 RGBA 索引必须为 (y * width + x) * 4（行步进按宽度、每像素 4 通道）。
  assert.ok(body.includes('const index = (y * width + x) * 4;'), '索引必须按行步进与 4 通道计算');
  // 行为：5×5 中心 (2,2)；4×4 中心 (2,2)（floor 对齐十字）；3×3 中心 (1,1)。
  const mk = (w, h, x, y, r, g, b) => {
    const data = new Uint8ClampedArray(w * h * 4);
    data[(y * w + x) * 4] = r;
    data[(y * w + x) * 4 + 1] = g;
    data[(y * w + x) * 4 + 2] = b;
    return data;
  };
  assert.deepEqual(readCenterPixel(mk(5, 5, 2, 2, 1, 2, 3), 5, 5), { r: 1, g: 2, b: 3 });
  assert.deepEqual(readCenterPixel(mk(4, 4, 2, 2, 7, 8, 9), 4, 4), { r: 7, g: 8, b: 9 });
  assert.deepEqual(readCenterPixel(mk(3, 3, 1, 1, 10, 20, 30), 3, 3), { r: 10, g: 20, b: 30 });
});

test('hexFromRgb 源码补零大写语义且小通道值必须两位十六进制', () => {
  const source = readFileSync(new URL('./colorModel.js', import.meta.url), 'utf8');
  // 源码护栏：单通道转十六进制必须补零到两位且转大写（#0A0B0C 而非 #ABC）。
  assert.ok(source.includes("clampChannel(value).toString(16).padStart(2, '0').toUpperCase()"), '十六进制必须补零并大写');
  // 行为：小通道值必须输出两位（01/0A），大通道值大写，全零输出 #000000。
  assert.equal(hexFromRgb({ r: 1, g: 10, b: 255 }), '#010AFF');
  assert.equal(hexFromRgb({ r: 0, g: 0, b: 0 }), '#000000');
  assert.equal(hexFromRgb({ r: 255, g: 0, b: 128 }), '#FF0080');
});

test('formatRgb 与 hexFromRgb 共用通道夹紧单一来源且四舍五入边界锁定', () => {
  const source = readFileSync(new URL('./colorModel.js', import.meta.url), 'utf8');
  // 源码护栏一：clampChannel 必须为 0-255 夹紧 + Math.round 四舍五入（单一夹紧函数）。
  assert.ok(source.includes('return Math.min(255, Math.max(0, Math.round(value)));'), '夹紧必须为 0-255 且四舍五入');
  // 源码护栏二：formatRgb 的 RGB 模板必须三通道全部走 clampChannel（禁止内联 Math.min/max 分叉）。
  assert.ok(source.includes('return `RGB(${clampChannel(rgb.r)}, ${clampChannel(rgb.g)}, ${clampChannel(rgb.b)})`;'), 'RGB 模板必须三通道共用 clampChannel');
  // 源码护栏三：hexFromRgb 的 toHex 也必须走 clampChannel（与 RGB 文本同一夹紧语义）。
  assert.ok(source.includes('clampChannel(value).toString(16).padStart(2, \'0\').toUpperCase()'), '十六进制必须共用 clampChannel');
  // 行为验证：负值夹 0、超 255 夹 255、小数四舍五入（255.4→255、255.5→255 半进位后夹紧、-0.4→0）。
  assert.equal(formatRgb({ r: -5, g: 300, b: 128 }), 'RGB(0, 255, 128)', '负值与超值必须夹紧');
  assert.equal(formatRgb({ r: 255.4, g: 10.6, b: 0 }), 'RGB(255, 11, 0)', '小数必须四舍五入');
  assert.equal(formatRgb({ r: 255.5, g: -0.5, b: 1 }), 'RGB(255, 0, 1)', '半进位后夹紧边界');
  assert.equal(hexFromRgb({ r: 255.5, g: 10.6, b: 0 }), '#FF0B00', '十六进制与 RGB 文本共用同一夹紧');
});

test('hexFromRgb 输出大写十六进制', () => {
  assert.equal(hexFromRgb({ r: 255, g: 0, b: 128 }), '#FF0080');
  assert.equal(hexFromRgb({ r: 0, g: 0, b: 0 }), '#000000');
});
