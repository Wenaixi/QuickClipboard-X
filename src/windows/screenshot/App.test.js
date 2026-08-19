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

test('工具栏放置与定位统一走自适应模块', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const placement = toolbarPlacement(selection, bootstrap.bounds);'));
  assert.ok(source.includes('return toolbarStyleModel(selection, bootstrap.bounds, placement);'));
});

test('选区建立后渲染像素标尺并输出自适应刻度', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const horizontalTicks = rulerTicks(bounds.width);'));
  assert.ok(source.includes('const verticalTicks = rulerTicks(bounds.height);'));
  assert.ok(source.includes('{selection && <Ruler bounds={bootstrap.bounds} />}'));
  assert.ok(source.includes('screenshot-ruler-tick-major'));
});

test('选区建立后渲染三分法构图辅助网格', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('{selection && <ThirdsGrid bounds={bootstrap.bounds} />}'));
  assert.ok(source.includes('const grid = thirdsGrid(bounds);'));
  assert.ok(source.includes('data-screenshot-thirds="true"'));
});

test('十字参考线在拖拽时渲染并贯穿画布', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('<CrosshairGuides point={magnifierPoint} bounds={bootstrap.bounds} />'));
  assert.ok(source.includes('const lines = guideLines(point, bounds);'));
  assert.ok(source.includes('data-screenshot-guide="true"'));
});

test('坐标指示面板在拖拽时渲染并显示光标坐标', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('<div className="screenshot-coordinates" data-screenshot-coordinates="true" style={coordinatePanelStyle(magnifierPoint, bootstrap.bounds)}>{formatCursorCoordinate(magnifierPoint)}</div>'));
  assert.ok(source.includes('coordinatePanelPosition(point, bounds)'));
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

test('滚轮调整放大镜缩放倍率并随缩放重绘', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('onWheel={handleWheel}'));
  assert.ok(source.includes('setMagnifierScale((current) => magnifierScaleForWheel(current, event.deltaY));'));
  assert.ok(source.includes('magnifierCanvasStyle(magnifierPoint, bootstrap.bounds, magnifierScale)'));
  assert.ok(source.includes('{ scale: magnifierScale }'));
});

test('放大镜绘制后读取中心像素颜色并渲染读数标签', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('readCenterPixel(context.getImageData(0, 0, canvas.width, canvas.height).data, canvas.width, canvas.height)'));
  assert.ok(source.includes('setMagnifierColor(pixel);'));
  assert.ok(source.includes('setMagnifierColor(null);'));
  assert.ok(source.includes('data-screenshot-color="true"'));
  assert.ok(source.includes('formatRgb(magnifierColor)'));
  assert.ok(source.includes('hexFromRgb(magnifierColor)'));
});

test('放大镜 canvas 用几何绘制背景快照且关闭平滑', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('context.imageSmoothingEnabled = false;'));
  assert.ok(source.includes('magnifierGeometry(magnifierPoint, bootstrap.bounds, { scale: magnifierScale })'));
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

test('尺寸标签统一走格式化模块并含百万像素', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('selectionSizeLabelText(selection)'));
  assert.ok(source.includes('const pixels = formatPixelSize(selection);'));
  assert.ok(source.includes('const ratio = formatAspectRatio(selection);'));
  assert.ok(source.includes('const megapixels = formatMegapixels(selection);'));
});

test('尺寸标签随选区贴近边缘翻转防溢出', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 类名与内联样式两个辅助函数都必须调用放置函数，任何一处绕过都应被捕获。
  const placements = (source.match(/selectionLabelPlacement\(selection, bounds\)/g) || []).length;
  assert.ok(placements >= 2, `期望至少 2 处放置调用，实际 ${placements}`);
  assert.ok(source.includes('screenshot-selection-size-below'));
  assert.ok(source.includes('screenshot-selection-size-left'));
});

test('悬停选区时切换调整/移动光标并在取消时重置', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const hover = cursorForSelectionHover(pointFromPointerEvent(event, root), selectionRef.current, bootstrap.bounds);'));
  assert.ok(source.includes("root.style.cursor = hover || 'crosshair';"));
  assert.ok(source.includes("rootRef.current.style.cursor = 'crosshair';"));
});

test('选区边框样式接线调用 lineStyle 生成描边', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const { borderWidth, borderColor } = lineStyle(1, \'rgba(255, 255, 255, 0.96)\', 1);'));
  assert.ok(source.includes('screenshot-selection-line'));
});

test('调整大小按住 Ctrl 从中心缩放并保持比例可叠加', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const calls = (source.match(/resizeSelection\(selectionRef\.current, resizing\.edge, current, bootstrap\.bounds, \{ keepAspectRatio: event\.shiftKey, fromCenter: event\.ctrlKey \}\)/g) || []).length;
  assert.ok(calls >= 2, `期望移动与抬起两处都传 fromCenter，实际 ${calls}`);
});

test('选区内部按下进入整体平移模式', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('if (hitSelectionInterior(start, selectionRef.current, MOVE_INSET)) {'));
  assert.ok(source.includes('moveRef.current = { pointerId: event.pointerId, start, selectionStart: selectionRef.current };'));
});

test('双击选区内部时完成截图且忽略控件区', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('onDoubleClick={handleDoubleClick}'));
  assert.ok(source.includes('if (!hitSelectionInterior(point, selectionRef.current, 0)) return;'));
  // 锚定 handleDoubleClick 函数体结尾的多行模式，避免被 Enter 快捷键分支误命中。
  assert.ok(source.includes("    event.preventDefault();\n    void completeScreenshot('copy');\n  };"));
  assert.ok(source.includes('target.closest(\'[data-screenshot-control]\')'));
});

test('初次拖拽按住 Shift 时实时走正方形框选', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('? squareSelection(draft.start, draft.end, bootstrap.bounds)'));
});

test('拖动收尾时非单击选区吸附到屏幕引导线且单击选窗不吸附', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('if (!clicked) finalSelection = magnetSelection(finalSelection, bootstrap.bounds);'));
  assert.ok(source.includes('magnetSelection(resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey, fromCenter: event.ctrlKey }), bootstrap.bounds, { edge: resizing.edge })'));
  assert.ok(source.includes('magnetSelection(nudgeSelection(moving.selectionStart, current.x - moving.start.x, current.y - moving.start.y, bootstrap.bounds), bootstrap.bounds)'));
});

test('松开时按住 Shift 生成正方形且跳过选窗逻辑', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const square = event.shiftKey && !clicked ? squareSelection(draft.start, end, bootstrap.bounds) : null;'));
  assert.ok(source.includes('if (!square && clicked && bootstrap.sessionId) {'));
});

test('选区边缘按下进入调整大小模式', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const edge = hitSelectionEdge(start, selectionRef.current, RESIZE_TOLERANCE);'));
  assert.ok(source.includes('resizeRef.current = { pointerId: event.pointerId, edge };'));
});

test('调整大小拖拽按住 Shift 保持宽高比', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 分别锚定实时移动与收尾两条调用，防止任一路径丢失 Shift 传参或磁吸接线。
  assert.ok(source.includes('const next = magnetSelection(resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey, fromCenter: event.ctrlKey }), bootstrap.bounds, { edge: resizing.edge });'));
  assert.ok(source.includes('const finalSelection = magnetSelection(resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey, fromCenter: event.ctrlKey }), bootstrap.bounds, { edge: resizing.edge });'));
});

test('调整大小拖拽调用 resizeSelection 并实时同步选区', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = magnetSelection(resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey, fromCenter: event.ctrlKey }), bootstrap.bounds, { edge: resizing.edge });'));
  assert.ok(source.includes('selectionRef.current = next;'));
});

test('调整大小结束后保留选区且清理调整状态', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const finalSelection = magnetSelection(resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey, fromCenter: event.ctrlKey }), bootstrap.bounds, { edge: resizing.edge });'));
  assert.ok(source.includes('resizeRef.current = null;'));
  assert.ok(source.includes('selectionRef.current = finalSelection;'));
});

test('平移拖拽调用 nudgeSelection 并实时同步选区', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = magnetSelection(nudgeSelection(moving.selectionStart, current.x - moving.start.x, current.y - moving.start.y, bootstrap.bounds), bootstrap.bounds);'));
  assert.ok(source.includes('selectionRef.current = next;'));
});

test('平移结束后保留选区且不触发选窗或清空', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('moveRef.current = null;'));
  assert.ok(source.includes('selectionRef.current = finalSelection;'));
  assert.ok(source.includes('setSelection(finalSelection);'));
});

test('选区调整前记录历史且 Ctrl+Z 撤销恢复', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const keydownIndex = source.indexOf('const handleKeyDown = (event) => {');
  const body = source.slice(keydownIndex);
  const zIndex = body.indexOf("if (event.ctrlKey && event.key.toLowerCase() === 'z') {");
  assert.ok(zIndex !== -1, 'keydown 必须包含 Ctrl+Z 撤销分支');
  const nudgeIndex = body.indexOf('const next = nudgeSelection(selectionRef.current, nudgeX * step, nudgeY * step, bootstrap.bounds);');
  assert.ok(zIndex > nudgeIndex, '撤销分支必须位于微调分支之后');
  assert.ok(source.includes('selectionHistoryRef.current = pushSelectionHistory(selectionHistoryRef.current, selectionRef.current);'));
  assert.ok(source.includes('const undone = undoSelectionHistory(selectionHistoryRef.current);'));
  assert.ok(source.includes('selectionHistoryRef.current = undone.history;'));
  assert.ok(source.includes('selectionRef.current = undone.selection;'));
  assert.ok(source.includes('selectionHistoryRef.current = [];'));
});

test('焦点落在工具栏按钮时全局快捷键不抢原生激活', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const keydownIndex = source.indexOf('const handleKeyDown = (event) => {');
  const body = source.slice(keydownIndex);
  const escapeIndex = body.indexOf("event.key === 'Escape'");
  // 锚定完整守卫行，避免 `false &&` 前缀短路仍残留 closest 子串绕过护栏。
  const controlGuardLine = "if (event.target instanceof Element && event.target.closest('[data-screenshot-control]')) return;";
  const controlReturnIndex = body.indexOf(controlGuardLine);
  assert.ok(controlReturnIndex !== -1, 'keydown 必须排除控件区焦点');
  assert.ok(controlReturnIndex > escapeIndex, '控件排除必须位于 Esc 之后');
});

test('选区建立后数字键快捷执行动作且快捷键提示展示', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const hotkeyAction = actionForHotkey(event.key);'));
  assert.ok(source.includes('if (hotkeyAction) { event.preventDefault(); void completeScreenshot(hotkeyAction); return; }'));
  assert.ok(source.includes('hotkeyForAction(action.id)'));
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
