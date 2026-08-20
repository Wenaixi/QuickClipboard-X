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
