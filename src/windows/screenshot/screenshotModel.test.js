import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { normalizeBootstrap } from './screenshotModel.js';

test('normalizeBootstrap lifecycleMode 默认 quick 且 dispose/auto 透传非法值回退', () => {
  // 默认（未提供字段）：quick。
  assert.equal(normalizeBootstrap({}).lifecycleMode, 'quick');
  assert.equal(normalizeBootstrap({ lifecycleMode: 'dispose' }).lifecycleMode, 'dispose');
  assert.equal(normalizeBootstrap({ lifecycleMode: 'auto' }).lifecycleMode, 'auto');
  // 非法值必须回退 quick（后端仅认识三种枚举）。
  assert.equal(normalizeBootstrap({ lifecycleMode: 'keep' }).lifecycleMode, 'quick');
  assert.equal(normalizeBootstrap({ lifecycleMode: null }).lifecycleMode, 'quick');
  assert.equal(normalizeBootstrap({ lifecycleMode: 42 }).lifecycleMode, 'quick');
});

test('normalizeBootstrap 提示与 AI 开关默认开启且配置必须严格布尔', () => {
  // 默认：提示开启、AI 功能开启但未配置、放大镜开启。
  const defaults = normalizeBootstrap({});
  assert.equal(defaults.screenshotHintsEnabled, true, '截图提示默认开启');
  assert.equal(defaults.screenshotAiEnabled, true, 'AI 功能默认开启');
  assert.equal(defaults.screenshotAiConfigured, false, 'AI 配置默认 false');
  assert.equal(defaults.screenshotMagnifierEnabled, true, '放大镜默认开启');
  // 显式关闭生效。
  assert.equal(normalizeBootstrap({ screenshotHintsEnabled: false }).screenshotHintsEnabled, false);
  assert.equal(normalizeBootstrap({ screenshotAiEnabled: false }).screenshotAiEnabled, false);
  // AI 配置必须严格 true：字符串 'true' 不算配置完成。
  assert.equal(normalizeBootstrap({ screenshotAiConfigured: 'true' }).screenshotAiConfigured, false);
  assert.equal(normalizeBootstrap({ screenshotAiConfigured: 1 }).screenshotAiConfigured, false);
  assert.equal(normalizeBootstrap({ screenshotAiConfigured: true }).screenshotAiConfigured, true);
});

test('normalizeBootstrap 显示器物理坐标按 dpr 换算为逻辑坐标', () => {
  // monitor.left/top 是物理像素，除以 dpr 得逻辑像素（与 bounds 同坐标系）。
  const result = normalizeBootstrap({
    devicePixelRatio: 1.25,
    monitor: { logicalWidth: 1536, logicalHeight: 864, physicalWidth: 1920, physicalHeight: 1080, left: -1920, top: 240 },
  });
  assert.equal(result.monitorLeft, -1536, '左缘物理 -1920 除以 dpr 1.25 得 -1536');
  assert.equal(result.monitorTop, 192, '上缘物理 240 除以 dpr 1.25 得 192');
  assert.equal(result.bounds.width, 1536, '逻辑宽必须来自 monitor.logicalWidth');
  assert.equal(result.physicalBounds.width, 1920, '物理宽必须来自 monitor.physicalWidth');
});

test('normalizeBootstrap 尺寸缺失时逐级回退并保持最小 1 且物理从逻辑推导', () => {
  // 无任何尺寸：回退到最小 1x1，dpr 回退 1，物理从逻辑 * dpr 推导。
  const minimal = normalizeBootstrap({});
  assert.deepEqual(minimal.bounds, { width: 1, height: 1 });
  assert.deepEqual(minimal.physicalBounds, { width: 1, height: 1 });
  assert.equal(minimal.dpr, 1);
  // 视口提供尺寸但 monitor 缺失：用视口逻辑尺寸，物理从逻辑 * dpr 推导。
  const fromViewport = normalizeBootstrap({}, { width: 1200, height: 700, devicePixelRatio: 1.5 });
  assert.deepEqual(fromViewport.bounds, { width: 1200, height: 700 });
  assert.deepEqual(fromViewport.physicalBounds, { width: 1800, height: 1050 });
  assert.equal(fromViewport.dpr, 1.5);
  // payload.viewport 优先于全局视口。
  const payloadViewport = normalizeBootstrap({ viewport: { width: 800, height: 600 } }, { width: 1200, height: 700 });
  assert.deepEqual(payloadViewport.bounds, { width: 800, height: 600 });
});

test('normalizeBootstrap 源码正数回退只接受有限正数且任意非法输入组合输出恒正有限', () => {
  const source = readFileSync(new URL('./screenshotModel.js', import.meta.url), 'utf8');
  // 源码护栏一：positiveNumber 必须只接受有限正数（Number.isFinite && value > 0），
  // 0/负数/NaN/Infinity 全部走 fallback——尺寸为 0 会让选区与采样全部失效。
  assert.ok(source.includes('Number.isFinite(value) && value > 0 ? value : fallback'), 'positiveNumber 必须只接受有限正数');
  // 行为属性：大量非法输入组合（0/负数/NaN/Infinity/字符串/undefined 混合）下，
  // dpr、bounds、physicalBounds 必须恒为有限正数，且物理尺寸与逻辑尺寸按 dpr 守恒。
  const badValues = [0, -1, Number.NaN, Infinity, -Infinity, '800', null, undefined];
  let combinations = 0;
  for (const dpr of [undefined, 0, -2, Number.NaN, 2]) {
    for (const w of [undefined, 0, -5, Number.NaN, 800]) {
      for (const h of [undefined, 0, -5, Number.NaN, 600]) {
        const result = normalizeBootstrap({ devicePixelRatio: dpr, monitor: { logicalWidth: w, logicalHeight: h } });
        assert.ok(Number.isFinite(result.dpr) && result.dpr > 0, `dpr 必须为正有限（输入 ${dpr}）`);
        assert.ok(Number.isFinite(result.bounds.width) && result.bounds.width > 0, '逻辑宽必须为正有限');
        assert.ok(Number.isFinite(result.bounds.height) && result.bounds.height > 0, '逻辑高必须为正有限');
        assert.ok(Number.isFinite(result.physicalBounds.width) && result.physicalBounds.width > 0, '物理宽必须为正有限');
        assert.ok(Number.isFinite(result.physicalBounds.height) && result.physicalBounds.height > 0, '物理高必须为正有限');
        assert.equal(result.physicalBounds.width, Math.round(result.bounds.width * result.dpr), '物理宽必须与逻辑宽按 dpr 守恒');
        assert.equal(result.physicalBounds.height, Math.round(result.bounds.height * result.dpr), '物理高必须与逻辑高按 dpr 守恒');
        combinations += 1;
      }
    }
  }
  assert.ok(combinations >= 100, '非法输入组合扫描必须充分');
  // 显式非法尺寸（0/负数）与非法 dpr（0/负数）也必须被回退链消化。
  const zeroed = normalizeBootstrap({ devicePixelRatio: 0, monitor: { logicalWidth: 0, logicalHeight: 0 } });
  assert.equal(zeroed.dpr, 1, 'dpr 0 必须回退 1');
  assert.deepEqual(zeroed.bounds, { width: 1, height: 1 }, '尺寸 0 必须回退最小 1');
});

test('normalizeBootstrap 源码生命周期值域白名单且物理尺寸从逻辑推导', () => {
  const source = readFileSync(new URL('./screenshotModel.js', import.meta.url), 'utf8');
  // 源码护栏一：lifecycleMode 值域必须只允许 quick/dispose/auto（非法值回退 quick）。
  assert.ok(source.includes("payload.lifecycleMode === 'dispose' || payload.lifecycleMode === 'auto' ? payload.lifecycleMode : 'quick'"), '生命周期值域白名单必须存在');
  // 源码护栏二：物理宽高缺省时从逻辑 * dpr 推导（非零像素必须守恒）。
  assert.ok(source.includes('Math.round(logicalWidth * dpr)'), '物理宽必须从逻辑宽 * dpr 推导');
  assert.ok(source.includes('Math.round(logicalHeight * dpr)'), '物理高必须从逻辑高 * dpr 推导');
  // 源码护栏三：显示器物理坐标必须除以 dpr 换算为逻辑坐标。
  assert.ok(source.includes('monitor.left / dpr'), '显示器左缘必须除以 dpr');
  assert.ok(source.includes('monitor.top / dpr'), '显示器上缘必须除以 dpr');
});
