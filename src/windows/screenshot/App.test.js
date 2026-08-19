import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { normalizeBootstrap } from './screenshotModel.js';

test('截图浮窗中英文语言包完整提供动作和状态文案', () => {
  for (const locale of ['zh-CN', 'en-US']) {
    const messages = JSON.parse(readFileSync(new URL(`../../shared/locales/${locale}.json`, import.meta.url)));
    const screenshot = messages.screenshot;
    assert.deepEqual(Object.keys(screenshot.actions).sort(), ['ai', 'configureAi', 'copy', 'pin', 'save']);
    for (const key of ['processing', 'selectionLabel', 'cancelLabel', 'cancelFailed', 'sessionNotReady', 'actionFailed', 'openAiSettingsFailed', 'shortcutHint']) {
      assert.equal(typeof screenshot[key], 'string', `${locale} 缺少 screenshot.${key}`);
    }
  }
});

test('normalizeBootstrap 使用显式显示器物理尺寸与逻辑尺寸', () => {
  const result = normalizeBootstrap({
    sessionId: 'session-1',
    devicePixelRatio: 1.25,
    monitor: {
      logicalWidth: 1536,
      logicalHeight: 864,
      physicalWidth: 1920,
      physicalHeight: 1080,
      left: -1920,
      top: 240,
    },
  });

  assert.deepEqual(result, {
    sessionId: 'session-1',
    bounds: { width: 1536, height: 864 },
    physicalBounds: { width: 1920, height: 1080 },
    monitorLeft: -1536,
    monitorTop: 192,
    dpr: 1.25,
    initialAction: '',
    screenshotAiEnabled: true,
    screenshotAiConfigured: false,
    screenshotMagnifierEnabled: true,
    magnifierBackground: null,
  });
});

test('放大镜渲染条件包含开关、背景快照与指针点', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 以 JSX 表达式开头锚定，避免前置 false 短路绕过该条件。
  assert.ok(source.includes('{bootstrap.screenshotMagnifierEnabled && bootstrap.magnifierBackground && magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && ('));
  assert.ok(source.includes('<canvas className="screenshot-magnifier"'));
});

test('放大镜拖拽时更新指针点并在结束/取消时清空', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('setMagnifierPoint(current);'));
  assert.ok(source.includes('setMagnifierPoint(draft.end);'));
  assert.ok(source.includes('setMagnifierPoint(null);'));
});

test('放大镜 canvas 用几何绘制背景快照且关闭平滑', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('context.imageSmoothingEnabled = false;'));
  assert.ok(source.includes('magnifierGeometry(magnifierPoint, bootstrap.bounds)'));
  assert.ok(source.includes('context.drawImage('));
});

test('normalizeBootstrap 透传放大镜开关与背景快照', () => {
  const result = normalizeBootstrap({
    screenshotMagnifierEnabled: false,
    magnifierBackground: 'data:image/png;base64,AAAA',
  });
  assert.equal(result.screenshotMagnifierEnabled, false);
  assert.equal(result.magnifierBackground, 'data:image/png;base64,AAAA');
});

test('选区内部按下进入整体平移模式', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('if (hitSelectionInterior(start, selectionRef.current, MOVE_INSET)) {'));
  assert.ok(source.includes('moveRef.current = { pointerId: event.pointerId, start, selectionStart: selectionRef.current };'));
});

test('选区边缘按下进入调整大小模式', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const edge = hitSelectionEdge(start, selectionRef.current, RESIZE_TOLERANCE);'));
  assert.ok(source.includes('resizeRef.current = { pointerId: event.pointerId, edge };'));
});

test('调整大小拖拽调用 resizeSelection 并实时同步选区', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds);'));
  assert.ok(source.includes('selectionRef.current = next;'));
});

test('调整大小结束后保留选区且清理调整状态', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const finalSelection = resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds);'));
  assert.ok(source.includes('resizeRef.current = null;'));
  assert.ok(source.includes('selectionRef.current = finalSelection;'));
});

test('平移拖拽调用 nudgeSelection 并实时同步选区', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = nudgeSelection(moving.selectionStart, current.x - moving.start.x, current.y - moving.start.y, bootstrap.bounds);'));
  assert.ok(source.includes('selectionRef.current = next;'));
});

test('平移结束后保留选区且不触发选窗或清空', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('moveRef.current = null;'));
  assert.ok(source.includes('selectionRef.current = finalSelection;'));
  assert.ok(source.includes('setSelection(finalSelection);'));
});

test('截图键盘微调接线调用 nudgeSelection 并同步选区状态', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = nudgeSelection(selectionRef.current, nudgeX * step, nudgeY * step, bootstrap.bounds);'));
  assert.ok(source.includes('setSelection(next);'));
  assert.ok(source.includes('applySelectionStyle(rootRef.current, next);'));
});

test('截图键盘微调默认 1px 且 Ctrl 加速 10px', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const NUDGE_STEP = 1;'));
  assert.ok(source.includes('const NUDGE_FAST_STEP = 10;'));
  assert.ok(source.includes('const step = event.ctrlKey ? NUDGE_FAST_STEP : NUDGE_STEP;'));
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
      monitorLeft: 0,
      monitorTop: 0,
      dpr: 1.5,
      initialAction: '',
      screenshotAiEnabled: true,
      screenshotAiConfigured: false,
      screenshotMagnifierEnabled: true,
      magnifierBackground: null,
    });
  } finally {
    Object.defineProperty(globalThis, 'innerWidth', { configurable: true, value: oldWidth });
    Object.defineProperty(globalThis, 'innerHeight', { configurable: true, value: oldHeight });
    Object.defineProperty(globalThis, 'devicePixelRatio', { configurable: true, value: oldDpr });
  }
});
