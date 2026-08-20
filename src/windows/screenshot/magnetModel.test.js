import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { magnetSelection } from './magnetModel.js';

const bounds = { width: 1920, height: 1080 };

test('magnetSelection 平移靠近屏幕左缘时吸附到 0 且保持尺寸', () => {
  const result = magnetSelection({ left: 3, top: 100, right: 403, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 0, top: 100, right: 400, bottom: 400, width: 400, height: 300 });
});

test('magnetSelection 平移靠近屏幕右缘时吸附到宽度', () => {
  const result = magnetSelection({ left: 1517, top: 100, right: 1917, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 1520, top: 100, right: 1920, bottom: 400, width: 400, height: 300 });
});

test('magnetSelection 平移靠近垂直中心线时吸附到中心', () => {
  const result = magnetSelection({ left: 953, top: 100, right: 973, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 950, top: 100, right: 970, bottom: 400, width: 20, height: 300 });
});

test('magnetSelection 超出容差时保持原选区', () => {
  const result = magnetSelection({ left: 10, top: 100, right: 410, bottom: 400 }, bounds);
  assert.deepEqual(result, { left: 10, top: 100, right: 410, bottom: 400, width: 400, height: 300 });
});

test('magnetSelection 两轴同时靠近边缘时同时吸附', () => {
  const result = magnetSelection({ left: 2, top: 3, right: 402, bottom: 203 }, bounds);
  assert.deepEqual(result, { left: 0, top: 0, right: 400, bottom: 200, width: 400, height: 200 });
});

test('magnetSelection 调整大小拖 e 边靠近右缘时仅右边缘吸附', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 1915, bottom: 400 }, bounds, { edge: 'e' });
  assert.deepEqual(result, { left: 100, top: 100, right: 1920, bottom: 400, width: 1820, height: 300 });
});

test('magnetSelection 调整大小拖 e 边靠近垂直中心线时右边缘吸附到中心', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 958, bottom: 400 }, bounds, { edge: 'e' });
  assert.deepEqual(result, { left: 100, top: 100, right: 960, bottom: 400, width: 860, height: 300 });
});

test('magnetSelection 调整大小拖 w 边靠近左缘时仅左边缘吸附', () => {
  const result = magnetSelection({ left: 3, top: 100, right: 403, bottom: 400 }, bounds, { edge: 'w' });
  assert.deepEqual(result, { left: 0, top: 100, right: 403, bottom: 400, width: 403, height: 300 });
});

test('magnetSelection 调整大小拖角点 se 时双轴同时吸附', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 1915, bottom: 1075 }, bounds, { edge: 'se' });
  assert.deepEqual(result, { left: 100, top: 100, right: 1920, bottom: 1080, width: 1820, height: 980 });
});

test('magnetSelection 调整大小拖 s 边靠近水平中心线时下边缘吸附到中心', () => {
  const result = magnetSelection({ left: 100, top: 100, right: 400, bottom: 542 }, bounds, { edge: 's' });
  assert.deepEqual(result, { left: 100, top: 100, right: 400, bottom: 540, width: 300, height: 440 });
});

test('magnetSelection 越界选区不产生负宽或零宽', () => {
  const bounds = { width: 800, height: 600 };
  const w = magnetSelection({ left: -100, top: 100, right: -50, bottom: 200 }, bounds, { edge: 'w' });
  assert.ok(w.width >= 1, 'edge=w 越界选区必须保持最小 1px 宽');
  assert.equal(w.left, 0);
  const n = magnetSelection({ left: 100, top: -100, right: 200, bottom: -50 }, bounds, { edge: 'n' });
  assert.ok(n.height >= 1, 'edge=n 越界选区必须保持最小 1px 高');
  assert.equal(n.top, 0);
  const pan = magnetSelection({ left: -5, top: -5, right: 805, bottom: 605 }, bounds);
  assert.ok(pan.right <= 800, '平移后右边界不得越界');
  assert.ok(pan.bottom <= 600, '平移后下边界不得越界');
});

test('magnetSelection 任何输入都返回边界内最小 1px 合法选区', () => {
  const source = readFileSync(new URL('./magnetModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function magnetSelection');
  const body = source.slice(start);
  // 平移与调整两条路径都必须以夹紧收尾，返回的选区不得越界或产生负尺寸。
  assert.ok(body.includes('left = clamp(left, 0, Math.max(0, bounds.width - snapWidth))'), '平移路径必须夹紧 left');
  assert.ok(body.includes('right = Math.min(left + snapWidth, bounds.width)'), '平移路径必须夹紧 right');
  assert.ok(body.includes('right = Math.max(right, left + 1)'), 'w 边调整必须保证最小 1px 宽度');
  assert.ok(body.includes('bottom = Math.max(bottom, top + 1)'), 'n 边调整必须保证最小 1px 高度');
  // 越界输入也必须产生合法选区。
  const pan = magnetSelection({ left: -5, top: -5, right: 805, bottom: 605 }, bounds);
  assert.ok(pan.width >= 1 && pan.height >= 1, '平移越界输入必须保持最小 1px');
  const w = magnetSelection({ left: -100, top: 100, right: -50, bottom: 200 }, bounds, { edge: 'w' });
  assert.ok(w.width >= 1 && w.height >= 1 && w.left >= 0, 'w 边越界输入必须夹紧且最小 1px');
});

test('magnetSelection 平移只改位置不改尺寸且吸附点精确落线', () => {
  // 属性测试：平移路径（edge=''）必须保持选区尺寸不变，仅允许位移；
  // 若发生吸附，吸附后的左缘/右缘/垂直中心必须精确落在 0 / 宽度 / 半宽之一。
  const source = readFileSync(new URL('./magnetModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function magnetSelection');
  const body = source.slice(start);
  // 源码护栏：平移候选线必须完整——左缘 0 / 右缘 bounds.width / 垂直中心 bounds.width/2 三条缺一不可。
  // 用完整多行块锚定平移路径（调整分支 edge='e' 也有单行右缘候选，contains 会被稀释，§10.4）。
  const dxBlock = 'const dx = bestSnapDelta([\n      { delta: 0 - left },\n      { delta: bounds.width - right },\n      { delta: bounds.width / 2 - (left + right) / 2 },\n    ], tolerance);';
  const dyBlock = 'const dy = bestSnapDelta([\n      { delta: 0 - top },\n      { delta: bounds.height - bottom },\n      { delta: bounds.height / 2 - (top + bottom) / 2 },\n    ], tolerance);';
  assert.ok(body.includes(dxBlock), '平移 dx 候选必须完整（左缘/右缘/垂直中心）');
  assert.ok(body.includes(dyBlock), '平移 dy 候选必须完整（顶缘/底缘/水平中心）');
  const cases = [
    { left: 3, top: 100, right: 403, bottom: 400 },
    { left: 1517, top: 100, right: 1917, bottom: 400 },
    { left: 953, top: 100, right: 973, bottom: 400 },
    { left: 10, top: 100, right: 410, bottom: 400 },
    { left: 2, top: 3, right: 402, bottom: 203 },
    { left: 600, top: 500, right: 900, bottom: 700 },
    { left: 0, top: 0, right: 1920, bottom: 1080 },
    { left: -5, top: -5, right: 805, bottom: 605 },
  ];
  for (const input of cases) {
    const result = magnetSelection(input, bounds);
    const inWidth = input.right - input.left;
    const inHeight = input.bottom - input.top;
    assert.equal(result.width, inWidth, '平移不得改变宽度');
    assert.equal(result.height, inHeight, '平移不得改变高度');
    assert.ok(result.left >= 0 && result.top >= 0 && result.right <= bounds.width && result.bottom <= bounds.height, '平移结果必须完全在边界内');
    // 吸附生效时，左缘/右缘/中心必须命中候选线；否则输出与输入一致。
    const snappedLines = [0, bounds.width, bounds.width / 2];
    const onLine = snappedLines.some((line) => result.left === line || result.right === line || (result.left + result.right) / 2 === line);
    if (result.left !== input.left || result.right !== input.right) {
      assert.ok(onLine, `发生吸附时必须精确落线: ${JSON.stringify(result)}`);
    }
  }
});

test('magnetSelection 默认容差 6px 且边界行为一致', () => {
  const source = readFileSync(new URL('./magnetModel.js', import.meta.url), 'utf8');
  // 源码护栏：默认吸附容差必须明确为 6（ShareX 公开行为的合理贴近阈值）。
  assert.ok(source.includes('const tolerance = options.tolerance ?? 6;'), '默认容差必须为 6');
  // 行为边界：距左缘 6px 内吸附到 0，7px 外不吸附（同一侧比较保证语义一致）。
  const make = (left) => ({ left, top: 100, right: left + 300, bottom: 300, width: 300, height: 200 });
  const at6 = magnetSelection(make(6), bounds);
  assert.equal(at6.left, 0, '距左缘 6px 必须吸附');
  assert.equal(at6.width, 300, '吸附不得改变尺寸');
  const at7 = magnetSelection(make(7), bounds);
  assert.equal(at7.left, 7, '距左缘 7px 不得吸附');
  // 自定义容差同样生效：容差 10 时 10px 吸附、11px 不吸附。
  const at10 = magnetSelection(make(10), bounds, { tolerance: 10 });
  assert.equal(at10.left, 0, '自定义容差 10 内必须吸附');
  const at11 = magnetSelection(make(11), bounds, { tolerance: 10 });
  assert.equal(at11.left, 11, '自定义容差 10 外不得吸附');
});

test('magnetSelection 最佳吸附候选单一来源且 edge 值域校验完整', () => {
  const source = readFileSync(new URL('./magnetModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function magnetSelection');
  const body = source.slice(start);
  // 源码护栏一：平移与调整两条路径都必须经 bestSnapDelta 取最小位移（不允许内联 if 拼差异）。
  const snapCalls = (body.match(/bestSnapDelta\(/g) || []).length;
  assert.ok(snapCalls >= 4, '必须存在至少 4 处 bestSnapDelta 调用（平移 dx/dy + 调整 e/w/n/s）');
  // 源码护栏二：调整分支必须按 edge.includes 分轴处理（拖 e 只吸附右缘/中心，不得动 left）。
  assert.ok(body.includes("if (edge.includes('e')) {"), 'e 边调整必须独立分支');
  assert.ok(body.includes("if (edge.includes('w')) {"), 'w 边调整必须独立分支');
  // 源码护栏三：edge 值域必须校验为 n/s/e/w 组合（非法值直接拒绝）。
  assert.ok(body.includes("!/^[nsew]+$/.test(edge)"), 'edge 必须限制为 n/s/e/w 组合');
  // 行为属性：bestSnapDelta 取最小位移（左缘 1px 与右缘 5px 并存时吸附左缘）；
  // edge 限制合法组合，拒绝非法组合。
  const nearBoth = magnetSelection({ left: 1, top: 100, right: 1915, bottom: 400 }, bounds);
  assert.equal(nearBoth.left, 0, '双候选并存时必须取最小位移（左缘 1px）');
  assert.equal(nearBoth.right, 1914, '平移时右缘必须随左缘同步移动保持尺寸');
  assert.equal(nearBoth.width, 1914, '吸附不得改变尺寸');
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { edge: 'x' }), /edge 必须是空串或 n\/s\/e\/w 组合/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { edge: 'nsewx' }), /edge 必须是空串或 n\/s\/e\/w 组合/);
  // 合法组合可用：se 双轴调整同时吸附。
  const se = magnetSelection({ left: 100, top: 100, right: 1915, bottom: 1075 }, bounds, { edge: 'se' });
  assert.equal(se.right, 1920, 'se 调整右缘必须吸附');
  assert.equal(se.bottom, 1080, 'se 调整下缘必须吸附');
});

test('magnetSelection 等距离候选平局时取先出现候选且源码严格小于', () => {
  const source = readFileSync(new URL('./magnetModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('function bestSnapDelta');
  const body = source.slice(start, start + 400);
  // 源码护栏：候选替换必须用严格小于（distance < bestDistance）而非 <=，
  // 保证等距离候选平局时保留先出现的候选（右缘候选先于中心候选），确定性无歧义。
  assert.ok(body.includes('if (distance <= tolerance && distance < bestDistance)'), '候选必须同时满足容差与严格小于');
  assert.ok(!body.includes('distance <= bestDistance'), '禁止宽松小于等于替换（平局歧义）');
  // 行为边界：构造 W=24 容差 6，e 边 right=18=3W/4 时右缘候选（W-right=6）与中心候选
  // （W/2-right=-6）距离相等且同落容差内，必须吸附到右缘 24（先出现候选）；
  // 若实现改为 <= 会替换为后候选错误吸附到中心 12。
  const small = { width: 24, height: 24 };
  const atThreeQuarter = magnetSelection({ left: 1, top: 1, right: 18, bottom: 4 }, small, { edge: 'e', tolerance: 6 });
  assert.equal(atThreeQuarter.right, 24, '等距离平局必须吸附到右缘（先出现候选）');
  assert.notEqual(atThreeQuarter.right, 12, '不得错误吸附到中心');
  // s 边同理：bottom=18=3H/4 时吸附到底缘 24。
  const sQuarter = magnetSelection({ left: 1, top: 1, right: 4, bottom: 18 }, small, { edge: 's', tolerance: 6 });
  assert.equal(sQuarter.bottom, 24, 's 边等距离平局必须吸附到底缘');
  assert.notEqual(sQuarter.bottom, 12, 's 边不得错误吸附到中心');
});

test('magnetSelection 调整路径四条边候选线完整（边缘加中心线）', () => {
  const source = readFileSync(new URL('./magnetModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function magnetSelection');
  const body = source.slice(start);
  // 源码护栏：四条被拖边的候选块必须各自包含屏幕边缘 + 屏幕中心线两个候选
  //（与平移路径 dxBlock/dyBlock 的完整块锚定一致，§10.4：单行 contains 会被其他分支稀释）。
  // 只留边缘候选会让靠近中心线时失去磁吸，只留中心线候选会让靠近屏幕边缘时失去磁吸。
  const eBlock = 'right += bestSnapDelta([\n        { delta: bounds.width - right },\n        { delta: bounds.width / 2 - right },\n      ], tolerance);';
  const wBlock = 'left += bestSnapDelta([\n        { delta: 0 - left },\n        { delta: bounds.width / 2 - left },\n      ], tolerance);';
  const sBlock = 'bottom += bestSnapDelta([\n        { delta: bounds.height - bottom },\n        { delta: bounds.height / 2 - bottom },\n      ], tolerance);';
  const nBlock = 'top += bestSnapDelta([\n        { delta: 0 - top },\n        { delta: bounds.height / 2 - top },\n      ], tolerance);';
  assert.ok(body.includes(eBlock), 'e 边候选必须完整（右缘加中心线）');
  assert.ok(body.includes(wBlock), 'w 边候选必须完整（左缘加中心线）');
  assert.ok(body.includes(sBlock), 's 边候选必须完整（下缘加中心线）');
  assert.ok(body.includes(nBlock), 'n 边候选必须完整（上缘加中心线）');
  // 行为验证：w 边左缘 963 距垂直中心 960 仅 3px，必须吸附到 960（中心线候选生效）；
  // n 边上缘 537 距水平中心 540 仅 3px，必须吸附到 540（中心线候选生效）。
  // 调整语义：被拖边吸附后精确落线，对边保持原值（w 边调整会改变宽度是正确行为）。
  const wCenter = magnetSelection({ left: 963, top: 100, right: 1263, bottom: 400 }, bounds, { edge: 'w' });
  assert.equal(wCenter.left, 960, 'w 边靠近垂直中心线必须吸附到 960');
  assert.equal(wCenter.right, 1263, 'w 边吸附不得移动对边');
  const nCenter = magnetSelection({ left: 100, top: 537, right: 400, bottom: 837 }, bounds, { edge: 'n' });
  assert.equal(nCenter.top, 540, 'n 边靠近水平中心线必须吸附到 540');
  assert.equal(nCenter.bottom, 837, 'n 边吸附不得移动对边');
  // 对称行为：e/s 边中心线吸附（既有测试 958→960 / 542→540）与边缘吸附（1915→1920 / 1075→1080）双路共存。
  assert.equal(magnetSelection({ left: 100, top: 100, right: 958, bottom: 400 }, bounds, { edge: 'e' }).right, 960, 'e 边中心线吸附必须保持');
  assert.equal(magnetSelection({ left: 100, top: 100, right: 1915, bottom: 400 }, bounds, { edge: 'e' }).right, 1920, 'e 边右缘吸附必须保持');
});

test('magnetSelection 拒绝无效输入', () => {
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, { width: 0, height: 10 }), /边界尺寸必须为正数/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { tolerance: -1 }), /吸附容差不能为负数/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: 1 }, bounds, { edge: 'x' }), /edge 必须是空串或 n\/s\/e\/w 组合/);
  assert.throws(() => magnetSelection({ left: 0, top: 0, right: 1, bottom: Number.NaN }, bounds), /bottom 必须是有限数字/);
});
