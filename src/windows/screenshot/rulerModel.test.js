import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { rulerMajorStep, rulerTicks } from './rulerModel.js';

test('rulerMajorStep 按屏幕尺寸自适应主刻度间隔', () => {
  assert.equal(rulerMajorStep(800), 50);
  assert.equal(rulerMajorStep(900), 100);
  assert.equal(rulerMajorStep(1600), 100);
  assert.equal(rulerMajorStep(1700), 200);
  assert.equal(rulerMajorStep(3840), 200);
});

test('rulerMajorStep 拒绝非正或非法长度', () => {
  assert.throws(() => rulerMajorStep(0), /标尺长度必须为正数/);
  assert.throws(() => rulerMajorStep(-10), /标尺长度必须为正数/);
  assert.throws(() => rulerMajorStep(Number.NaN), /标尺长度 必须是有限数字/);
});

test('rulerTicks 输出从 0 到长度的全部刻度并标记主刻度标签', () => {
  const ticks = rulerTicks(300);
  assert.equal(ticks.length, 31);
  assert.deepEqual(ticks[0], { position: 0, label: '0' });
  assert.deepEqual(ticks[5], { position: 50, label: '50' });
  assert.deepEqual(ticks[10], { position: 100, label: '100' });
  assert.equal(ticks[3].label, null);
  assert.equal(ticks[30].label, '300');
});

test('rulerTicks 主刻度间隔为 100 时标签只出现在整百位置', () => {
  const ticks = rulerTicks(1080);
  assert.equal(ticks.length, 55);
  const labeled = ticks.filter((tick) => tick.label !== null);
  assert.deepEqual(labeled.map((tick) => tick.position), [0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000]);
});

test('rulerTicks 小数长度下主刻度标签完整且不越界', () => {
  // 小数长度若用浮点累加会产生漂移，主刻度标签可能漏判；计数循环必须稳定。
  const ticks = rulerTicks(1365.33);
  assert.ok(ticks.length > 0);
  assert.ok(ticks.every((tick) => tick.position <= 1365.33), '刻度不得越界');
  const majors = ticks.filter((tick) => tick.label !== null);
  assert.ok(majors.length >= 6, '主刻度标签不得因浮点漂移缺失');
  assert.equal(majors[0].label, '0');
  assert.equal(majors[1].label, '100');
  assert.equal(majors[2].label, '200');
});

test('rulerTicks 标签为主刻度整数倍且文本整数并单调不越界', () => {
  // 属性测试：任意合法长度下，
  // ①带标签的刻度位置必须是主刻度间隔的整数倍（0 / majorStep / 2*majorStep ...）；
  // ②标签文本必须是整数（不含小数），否则渲染会出现 99.99999 类脏值；
  // ③刻度位置必须严格递增且不超过长度。
  const source = readFileSync(new URL('./rulerModel.js', import.meta.url), 'utf8');
  // 源码护栏：必须用整数计数循环（index 从 0 计数到 total，position = index * minorStep），
  // 禁止浮点累加（position += minorStep），否则小数长度下漂移。
  assert.ok(source.includes('for (let index = 0; index <= total; index += 1)'), '必须使用整数计数循环');
  assert.ok(source.includes('const position = index * minorStep;'), '位置必须由计数乘步进得到');
  assert.ok(!source.includes('position += minorStep'), '禁止浮点累加产生漂移');
  for (const length of [300, 1080, 1365.33, 1920, 3840, 800, 1600]) {
    const ticks = rulerTicks(length);
    const majorStep = rulerMajorStep(length);
    for (let i = 0; i < ticks.length; i += 1) {
      const tick = ticks[i];
      assert.ok(tick.position >= 0 && tick.position <= length, '刻度不得越界');
      if (i > 0) assert.ok(ticks[i].position > ticks[i - 1].position, '刻度必须严格递增');
      if (tick.label !== null) {
        assert.equal(tick.position % majorStep, 0, '带标签刻度必须是主刻度整数倍');
        assert.ok(Number.isInteger(tick.position), '标签文本对应的位置必须是整数');
        assert.equal(tick.label, String(tick.position), '标签文本必须与位置一致');
      }
    }
  }
});

test('rulerMajorStep 三档常量且次刻度为主刻度 1/5 标签索引 5 的倍数', () => {
  const source = readFileSync(new URL('./rulerModel.js', import.meta.url), 'utf8');
  const stepStart = source.indexOf('export function rulerMajorStep');
  const stepBody = source.slice(stepStart, stepStart + 300);
  // 源码护栏：三档主刻度必须为 50/100/200（800 及以下 50、1600 及以下 100、以上 200）。
  assert.ok(stepBody.includes('if (length <= 800) return 50;'), '800 及以下必须取 50');
  assert.ok(stepBody.includes('if (length <= 1600) return 100;'), '1600 及以下必须取 100');
  assert.ok(stepBody.includes('return 200;'), '1600 以上必须取 200');
  const ticksStart = source.indexOf('export function rulerTicks');
  const ticksBody = source.slice(ticksStart, ticksStart + 500);
  // 次刻度间隔必须为主刻度的 1/5（每主刻度放 4 个次刻度）。
  assert.ok(ticksBody.includes('const minorStep = majorStep / 5;'), '次刻度必须为主刻度 1/5');
  // 主刻度标签只在索引为 5 的倍数处（0/5/10...），其余次刻度标签为 null。
  assert.ok(ticksBody.includes('index % 5 === 0 ? String(position) : null'), '标签必须只在 5 的倍数索引');
  // 行为属性：300 长度下主刻度 0/50/100/150/200/250/300 全部带整数标签，次刻度无标签。
  const ticks = rulerTicks(300);
  for (const position of [0, 50, 100, 150, 200, 250, 300]) {
    const tick = ticks.find((t) => t.position === position);
    assert.ok(tick, `缺少刻度 ${position}`);
    assert.equal(tick.label, String(position), `主刻度 ${position} 必须带整数标签`);
  }
  assert.equal(ticks.find((t) => t.position === 10).label, null, '次刻度必须无标签');
  assert.equal(ticks.find((t) => t.position === 290).label, null, '次刻度必须无标签');
});

test('rulerTicks 源码 total 向下取整保证端点含而整倍数越界且最后刻度不越界', () => {
  const source = readFileSync(new URL('./rulerModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function rulerTicks');
  const body = source.slice(start, start + 500);
  // 源码护栏：刻度总数必须向下取整（Math.floor(length / minorStep)）——向上取整会让
  // 最后刻度越过长度（如 305 长度下最后一个刻度变成 310）。循环必须含端点（index <= total）。
  assert.ok(body.includes('const total = Math.floor(length / minorStep);'), 'total 必须向下取整');
  assert.ok(body.includes('for (let index = 0; index <= total; index += 1)'), '循环必须含端点');
  // 行为属性一：长度恰为次刻度整数倍时最后刻度必须恰好等于长度（端点含）。
  const exact300 = rulerTicks(300);
  assert.equal(exact300[exact300.length - 1].position, 300, '300 长度最后刻度必须恰好 300');
  const exact1080 = rulerTicks(1080);
  assert.equal(exact1080[exact1080.length - 1].position, 1080, '1080 长度最后刻度必须恰好 1080');
  // 行为属性二：长度非次刻度整数倍时最后刻度停在最后一个完整刻度（小于长度且是次刻度整数倍）。
  const partial = rulerTicks(305);
  assert.equal(partial[partial.length - 1].position, 300, '305 长度最后刻度必须停在 300');
  const partialFloat = rulerTicks(1365.33);
  assert.equal(partialFloat[partialFloat.length - 1].position, 1360, '1365.33 长度最后刻度必须停在 1360');
  // 行为属性三：任意长度下首刻度恒为 0，末刻度不越界且是次刻度整数倍。
  for (const length of [5, 50, 300, 305, 800, 1080, 1365.33, 1920, 3840]) {
    const ticks = rulerTicks(length);
    assert.equal(ticks[0].position, 0, '首刻度必须为 0');
    const last = ticks[ticks.length - 1];
    assert.ok(last.position <= length, '末刻度不得越界');
    const minorStep = rulerMajorStep(length) / 5;
    assert.equal(last.position % minorStep, 0, '末刻度必须是次刻度整数倍');
  }
});

test('rulerTicks 拒绝非正或非法长度', () => {
  assert.throws(() => rulerTicks(0), /标尺长度必须为正数/);
  assert.throws(() => rulerTicks(Number.NaN), /标尺长度 必须是有限数字/);
});
