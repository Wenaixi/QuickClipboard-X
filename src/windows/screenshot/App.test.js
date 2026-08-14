import { test } from 'node:test';
import assert from 'node:assert/strict';
import { normalizeBootstrap } from './screenshotModel.js';

test('normalizeBootstrap 使用显式显示器物理尺寸与逻辑尺寸', () => {
  const result = normalizeBootstrap({
    sessionId: 'session-1',
    devicePixelRatio: 1.25,
    monitor: {
      logicalWidth: 1536,
      logicalHeight: 864,
      physicalWidth: 1920,
      physicalHeight: 1080,
    },
  });

  assert.deepEqual(result, {
    sessionId: 'session-1',
    bounds: { width: 1536, height: 864 },
    physicalBounds: { width: 1920, height: 1080 },
    dpr: 1.25,
  });
});

test('normalizeBootstrap 缺失尺寸时以视口和 DPR 推导安全默认值', () => {
  const oldWidth = globalThis.innerWidth;
  const oldHeight = globalThis.innerHeight;
  const oldDpr = globalThis.devicePixelRatio;
  Object.defineProperty(globalThis, 'innerWidth', { configurable: true, value: 1200 });
  Object.defineProperty(globalThis, 'innerHeight', { configurable: true, value: 700 });
  Object.defineProperty(globalThis, 'devicePixelRatio', { configurable: true, value: 1.5 });

  try {
    assert.deepEqual(normalizeBootstrap({}, { width: globalThis.innerWidth, height: globalThis.innerHeight, devicePixelRatio: globalThis.devicePixelRatio }), {
      sessionId: '',
      bounds: { width: 1200, height: 700 },
      physicalBounds: { width: 1800, height: 1050 },
      dpr: 1.5,
    });
  } finally {
    Object.defineProperty(globalThis, 'innerWidth', { configurable: true, value: oldWidth });
    Object.defineProperty(globalThis, 'innerHeight', { configurable: true, value: oldHeight });
    Object.defineProperty(globalThis, 'devicePixelRatio', { configurable: true, value: oldDpr });
  }
});
