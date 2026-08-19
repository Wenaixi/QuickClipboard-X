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
    lifecycleMode: 'quick',
  });
});

test('工具栏放置与定位统一走自适应模块', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const placement = toolbarPlacement(selection, bootstrap.bounds);'));
  assert.ok(source.includes('return toolbarStyleModel(selection, bootstrap.bounds, placement);'));
});

test('Esc 与右键仅在无选区或小选区时取消截图', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { canResetSelection } from \'./resetModel.js\';'));
  assert.ok(source.includes("if (event.key === 'Escape' && (!selectionRef.current || canResetSelection(selectionRef.current))) { event.preventDefault(); void cancelScreenshot(); return; }"));
  assert.ok(source.includes("if (!selectionRef.current || canResetSelection(selectionRef.current)) void cancelScreenshot();"));
  const keydownIndex = source.indexOf('const handleKeyDown = (event) => {');
  const body = source.slice(keydownIndex);
  assert.ok(body.includes('canResetSelection'), 'keydown 必须使用小选区重置守卫');
});

test('放大镜绘制网格线与中心十字辅助像素对齐', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { magnifierGridLines, magnifierCrosshair } from \'./magnifierGridModel.js\';'));
  assert.ok(source.includes('const grid = magnifierGridLines(geometry);'));
  assert.ok(source.includes('const cross = magnifierCrosshair(geometry);'));
  assert.ok(source.includes('for (const x of grid.vertical) { context.moveTo(x + 0.5, 0); context.lineTo(x + 0.5, canvas.height); }'));
  assert.ok(source.includes('for (const y of grid.horizontal) { context.moveTo(0, y + 0.5); context.lineTo(canvas.width, y + 0.5); }'));
  assert.ok(source.includes('context.moveTo(cross.x + 0.5, 0);'));
});

test('Ctrl+A 一键选中整个屏幕并保留撤销历史', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { fullScreenSelection } from \'./fullScreenModel.js\';'));
  assert.ok(source.includes("if (event.ctrlKey && event.key.toLowerCase() === 'a') {"));
  assert.ok(source.includes('const next = fullScreenSelection(bootstrap.bounds);'));
  const keydownIndex = source.indexOf('const handleKeyDown = (event) => {');
  const body = source.slice(keydownIndex);
  assert.ok(body.includes('fullScreenSelection'), 'keydown 必须使用全屏选区生成');
  assert.ok(body.includes('pushSelectionHistory'), '全屏选区前必须保留撤销历史');
});

test('完成快捷键统一走模型且支持 Ctrl+C 复制', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { completeShortcutForEvent } from \'./completeShortcutModel.js\';'));
  assert.ok(source.includes('const completeAction = completeShortcutForEvent(event);'));
  assert.ok(source.includes('if (completeAction) { event.preventDefault(); void completeScreenshot(completeAction); }'));
});

test('选区建立前显示初始引导提示且不遮挡拖拽', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { idleHint } from \'./idleModel.js\';'));
  assert.ok(source.includes('!selecting && !selection && !showHelp && <div className="screenshot-idle-hint" aria-live="polite" data-screenshot-idle="true">{idleHint(t)}</div>'));
  for (const locale of ['zh-CN', 'en-US']) {
    const messages = JSON.parse(readFileSync(new URL(`../../shared/locales/${locale}.json`, import.meta.url)));
    assert.equal(typeof messages.screenshot.idleHint, 'string', `${locale} 缺少 screenshot.idleHint`);
  }
});

test('选区建立后显示完成动作邀请提示且不遮挡操作', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { includeInvite } from \'./inviteModel.js\';'));
  assert.ok(source.includes('selection && !selecting && !moving && !resizing && !busyAction && !showHelp && <div className="screenshot-invite" data-screenshot-invite="true">{includeInvite(t)}</div>'));
  for (const locale of ['zh-CN', 'en-US']) {
    const messages = JSON.parse(readFileSync(new URL(`../../shared/locales/${locale}.json`, import.meta.url)));
    assert.equal(typeof messages.screenshot.invite, 'string', `${locale} 缺少 screenshot.invite`);
  }
});

test('放大镜背景采样按 DPR 换算物理像素且色块不遮挡面板', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('geometry.source.cols * dpr'));
  assert.ok(source.includes('const magnifierLayout = useMemo(() => ('));
  // 颜色板必须跟随放大镜翻转，贴底时放到放大镜上方避免溢出被裁。
  assert.ok(source.includes('function colorPanelStyle(magnifierLayout, bounds)'));
  assert.ok(source.includes('below + 20 <= bounds.height'));
});

test('截图成功后主动关闭窗口避免残留选区状态闪现', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const start = source.indexOf('const completeScreenshot = async (action) => {');
  const body = source.slice(start);
  // 成功路径必须销毁窗口：后端 cleanup_plan 只 hide 不销毁，React 实例会保留
  // 上一会话的选区/工具栏状态并在下次复用窗口时闪现旧选区。
  assert.ok(body.includes("await getCurrentWindow().close().catch(() => {});"), '成功路径必须主动关闭截图窗口');
  const catchIndex = body.indexOf('} catch (error) {');
  const closeIndex = body.indexOf("await getCurrentWindow().close()");
  assert.ok(catchIndex !== -1 && closeIndex !== -1 && catchIndex < closeIndex, '失败路径必须 return 显示错误，成功路径才 close');
  // 顺序不变量：close 前必须先清空交互状态，否则 close 触发的 blur 会被
  // handleBlur 误判为取消而发出多余的 cancel_screenshot 请求（会话已完成）。
  const resetIndex = body.indexOf('resetInteractionState({ draftRef, selectionRef, moveRef, resizeRef, setSelection, setSelecting, setMoving, setResizing });');
  assert.ok(resetIndex !== -1 && resetIndex < closeIndex, '成功路径必须先清空交互状态再关闭窗口');
});

test('完成动作成功后清理占用且 AI 未配置回落配置入口', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('} finally {'));
  assert.ok(source.includes('setBusyAction(\'\');'));
  assert.ok(source.includes("if (action === 'ai') void openAiSettings();"));
});

test('帮助面板打开时按 Esc 关闭且失焦不打断处理', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes("if (event.key === 'Escape' && showHelp) { event.preventDefault(); setShowHelp(false); return; }"));
  assert.ok(source.includes('const handleBlur = () => { if (busyAction) return;'));
});

test('选区调整进行中键盘不触发完成与快速微调', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('busyAction || draftRef.current || moveRef.current || resizeRef.current) return;'));
  assert.ok(source.includes('if (nudgeX !== undefined && !event.altKey && !event.metaKey) {'));
});

test('帮助面板可交互且点击不穿透底层拖拽', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const css = readFileSync(new URL('./screenshot.css', import.meta.url), 'utf8');
  // 锚定 .screenshot-help 完整规则，避免被 toolbar/cancel 的 pointer-events: auto 误命中。
  const helpRule = css.split('\n').find((line) => line.includes('.screenshot-help {'));
  assert.ok(helpRule && helpRule.includes('pointer-events: auto;'), '帮助面板必须可交互');
  assert.ok(helpRule.includes('max-width: calc(100% - 24px)'), '帮助面板必须夹紧不超过屏幕宽度');
  assert.ok(helpRule.includes('min-width: min(300px'), '窄屏时帮助面板最小宽度必须可收缩');
  assert.ok(helpRule.includes('overflow: auto'), '帮助面板必须支持双轴滚动防内容溢出');
  assert.ok(source.includes('data-screenshot-control role="dialog"'), '帮助面板必须阻止底层拖拽');
  assert.ok(source.includes('onPointerDown={(event) => event.stopPropagation()}'), '帮助面板必须停止事件冒泡');
});

test('截图界面无障碍标注完整且高频面板不产生朗读噪音', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('role="toolbar" aria-label={t(\'screenshot.toolbarLabel\')}'), '工具栏必须有 role 与可访问名称');
  assert.ok(source.includes('className="screenshot-coordinates" aria-hidden="true"'), '坐标面板必须对屏幕阅读器隐藏');
  assert.ok(source.includes('className="screenshot-color" aria-hidden="true"'), '颜色面板必须对屏幕阅读器隐藏');
  assert.ok(source.includes('className="screenshot-mode-hint" aria-hidden="true"'), '模式提示必须对屏幕阅读器隐藏');
  assert.ok(source.includes('className="screenshot-idle-hint" aria-live="polite"'), '初始引导必须声明实时区域');
  assert.ok(source.includes('aria-hidden="true" className="screenshot-guide screenshot-guide-v"'), '十字参考线必须声明装饰性');
  assert.ok(source.includes('aria-hidden="true" className={`screenshot-handle screenshot-handle-'), '调整手柄必须声明装饰性');
  for (const locale of ['zh-CN', 'en-US']) {
    const messages = JSON.parse(readFileSync(new URL(`../../shared/locales/${locale}.json`, import.meta.url)));
    assert.equal(typeof messages.screenshot.toolbarLabel, 'string', `${locale} 缺少 screenshot.toolbarLabel`);
    assert.ok(messages.screenshot.idleHint.includes('Esc'), `${locale} idleHint 必须提示 Esc 取消`);
  }
});

test('快捷键帮助面板随 F1 切换并列出全部快捷键', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { helpEntries, isHelpShortcut } from \'./helpModel.js\';'));
  assert.ok(source.includes('if (isHelpShortcut(event)) { event.preventDefault(); setShowHelp((current) => !current); return; }'));
  assert.ok(source.includes('data-screenshot-help="true"'));
  assert.ok(source.includes('helpEntries(t).map((entry) =>'));
  for (const locale of ['zh-CN', 'en-US']) {
    const messages = JSON.parse(readFileSync(new URL(`../../shared/locales/${locale}.json`, import.meta.url)));
    for (const key of ['title', 'complete', 'save', 'pin', 'fullscreen', 'cancel', 'nudge', 'square', 'center', 'undo', 'quickAction']) {
      assert.equal(typeof messages.screenshot.help[key], 'string', `${locale} 缺少 screenshot.help.${key}`);
    }
  }
});

test('拖拽草稿走统一选区生成且取消/配置统一重置交互状态', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { selectionFromDraft, resetInteractionState } from \'./draftModel.js\';'));
  assert.ok(source.includes('const next = selectionFromDraft(draft, event, bootstrap.bounds);'));
  const resets = source.match(/resetInteractionState\(\{ draftRef, selectionRef, moveRef, resizeRef, setSelection, setSelecting, setMoving, setResizing \}\);/g);
  assert.ok(resets && resets.length >= 2, '取消与配置重置必须都走统一交互重置');
});

test('选区建立后渲染八个调整手柄', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { selectionHandles } from \'./handleModel.js\';'));
  assert.ok(source.includes('selectionHandles(selection).map((handle) =>'));
  assert.ok(source.includes('{selection && <SelectionHandles selection={selection} />}'));
  assert.ok(source.includes('data-screenshot-handle="true"'));
  assert.ok(source.includes('screenshot-handle-${handle.edge}'));
});

test('交互模式提示随拖拽/移动/调整状态渲染', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('modeHint(modeForState({ selecting, moving, resizing }), t)'));
  assert.ok(source.includes('data-screenshot-mode="true"'));
  assert.ok(source.includes('setResizing(true);'));
  assert.ok(source.includes('setMoving(true);'));
  assert.ok(source.includes('setResizing(false);'));
  assert.ok(source.includes('setMoving(false);'));
  for (const locale of ['zh-CN', 'en-US']) {
    const messages = JSON.parse(readFileSync(new URL(`../../shared/locales/${locale}.json`, import.meta.url)));
    for (const key of ['select', 'move', 'resize']) {
      assert.equal(typeof messages.screenshot.mode[key], 'string', `${locale} 缺少 screenshot.mode.${key}`);
    }
  }
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
  assert.ok(source.includes('<div className="screenshot-coordinates" aria-hidden="true" data-screenshot-coordinates="true" style={coordinatePanelStyle(magnifierPoint, bootstrap.bounds)}>{formatCursorCoordinate(magnifierPoint)}</div>'));
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
  // 中心像素必须在十字线叠加之前读取，否则颜色板显示红色污染后的伪色。
  // 顺序类不变量必须比较下标（§10.3 铁律 3）：只 contains 区分不了读取在绘制前还是后。
  const readIndex = source.indexOf('const centerPixel = readCenterPixel(context.getImageData(0, 0, canvas.width, canvas.height).data, canvas.width, canvas.height);');
  assert.ok(readIndex !== -1, '放大镜必须读取中心像素');
  const crossIndex = source.indexOf("context.strokeStyle = 'rgba(255, 80, 80, 0.85)'");
  assert.ok(crossIndex !== -1, '放大镜必须绘制红色十字线');
  assert.ok(readIndex < crossIndex, '中心像素读取必须早于十字线绘制');
  assert.ok(source.includes('setMagnifierColor(centerPixel);'));
  // 快速移动时旧 Image onload 可能晚于新帧触发，必须按代丢弃避免覆盖新渲染。
  assert.ok(source.includes('let generation = 0;'));
  assert.ok(source.includes('const frame = generation;'));
  assert.ok(source.includes('if (frame !== generation) return;'));
  assert.ok(source.includes('image.onload = null;'), 'effect cleanup 必须解除旧 onload 回调');
  assert.ok(source.includes('configurePromise\n      .then('), 'configure 监听链尾必须捕获拒绝');
  assert.ok(source.includes('setMagnifierColor(null);'));
  assert.ok(source.includes('data-screenshot-color="true"'));
  assert.ok(source.includes('formatRgb(magnifierColor)'));
  assert.ok(source.includes('colorPanelStyle(magnifierLayout, bootstrap.bounds)'));
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

test('尺寸标签统一走格式化模块并含位置与百万像素', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('selectionSizeLabelText(selection, bootstrap)'));
  assert.ok(source.includes('const physical = physicalSize(selection, bootstrap.dpr);'));
  assert.ok(source.includes('const pixels = formatPixelSize(physical);'));
  assert.ok(source.includes('const ratio = formatAspectRatio(physical);'));
  assert.ok(source.includes('const megapixels = formatMegapixels(physical);'));
  assert.ok(source.includes('const position = formatSelectionPosition(selection, { dpr: bootstrap.dpr, monitorLeft: bootstrap.monitorLeft, monitorTop: bootstrap.monitorTop });'));
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
  // 实时框选统一走 selectionFromDraft（内部按 Shift 分流 squareSelection/normalizeSelection），
  // 此处锚定统一调用，避免单点绕过。
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = selectionFromDraft(draft, event, bootstrap.bounds);'));
});

test('生命周期模式决定成功路径是否销毁截图窗口', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 成功路径必须按 dispose 条件关闭窗口；quick/auto 隐藏复用。
  assert.ok(source.includes("if (bootstrap.lifecycleMode === 'dispose') {"), 'dispose 模式才销毁窗口');
  const model = readFileSync(new URL('./screenshotModel.js', import.meta.url), 'utf8');
  assert.ok(model.includes("lifecycleMode: payload.lifecycleMode === 'dispose' || payload.lifecycleMode === 'auto' ? payload.lifecycleMode : 'quick'"), 'bootstrap 解析必须携带生命周期模式');
});

test('拖拽框选/移动/调整时实时显示尺寸标签且收尾清空', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 尺寸标签渲染必须同时接受 liveSelection 与 selection（拖拽中无正式选区）。
  assert.ok(source.includes('{(liveSelection || selection) &&'), '尺寸标签必须支持拖拽中实时选区');
  assert.ok(source.includes('selectionSizeLabelText(liveSelection || selection, bootstrap)'), '尺寸文本必须取实时选区');
  // 三条移动路径（resize/move/draft）必须每帧同步实时选区。
  const resizeSync = source.match(/magnetSelection\(resizeSelection/g);
  const moveSync = source.match(/nudgeSelection\(moving\.selectionStart/g);
  assert.ok(resizeSync && resizeSync.length >= 2, '调整大小实时路径必须存在');
  assert.ok(source.includes('const next = selectionFromDraft(draft, event, bootstrap.bounds);\n    setLiveSelection(next);'), '拖拽框选必须实时同步');
  assert.ok(source.includes('setLiveSelection(null);'), '交互收尾必须清空实时选区');
  // 清空次数：resize + move + draft 三条 pointerUp 路径 + configure 重置。
  assert.ok(source.split('setLiveSelection(null);').length >= 5, '收尾与重置路径都必须清空实时选区');
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

test('AI 未配置时工具栏禁用 AI 动作且数字键 4 引导进入设置', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 工具栏按钮：disabled 判定必须与可用性函数一致。
  assert.ok(source.includes('disabled={Boolean(busyAction) || !actionIsEnabled(action.id, bootstrap)}'), '按钮禁用必须跟随可用性');
  // completeScreenshot 内二次守卫：AI 未配置时按数字键 4 引导进入设置（与按钮点击一致）。
  assert.ok(source.includes("if (action === 'ai') void openAiSettings();"), 'AI 未配置时数字键 4 必须引导进入设置');
  assert.ok(source.includes('return action !== \'ai\' || (bootstrap.screenshotAiEnabled !== false && bootstrap.screenshotAiConfigured === true);'), 'AI 可用性判断必须要求已配置');
});

test('截图键盘微调接线调用 nudgeSelection 并同步选区状态', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = nudgeSelection(selectionRef.current, nudgeX * step, nudgeY * step, bootstrap.bounds);'));
  assert.ok(source.includes('setSelection(next);'));
  assert.ok(source.includes('applySelectionStyle(rootRef.current, next);'));
});

test('交互状态机修复护栏：ref 同步与 busyAction 守卫与放大镜复位', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // F1：新草稿起点同步清 selectionRef，避免与 React state 撕裂。
  assert.ok(source.includes('setSelecting(true);\n    selectionRef.current = null;\n    setSelection(null);'));
  // F4：处理中禁止 pointermove 改写选区。
  assert.ok(source.includes('const handlePointerMove = (event) => {\n    if (busyAction) return;'));
  // F5：调整/移动开始与完成分支统一清理 selecting 状态。
  assert.ok(source.includes('setSelecting(false);\n        setResizing(true);'));
  assert.ok(source.includes('setSelecting(false);\n        setMoving(true);'));
  // F3：configure 与取消时复位放大镜缩放倍率。
  assert.ok(source.includes('setMagnifierScale(DEFAULT_MAGNIFIER_SCALE);'));
  assert.ok(source.includes('const DEFAULT_MAGNIFIER_SCALE = 6;'));
  // F6：bounds 变化时重建键盘闭包。
  assert.ok(source.includes("}, [bootstrap.sessionId, bootstrap.bounds, busyAction, showHelp]);"));
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
      lifecycleMode: 'quick',
    });
  } finally {
    Object.defineProperty(globalThis, 'innerWidth', { configurable: true, value: oldWidth });
    Object.defineProperty(globalThis, 'innerHeight', { configurable: true, value: oldHeight });
    Object.defineProperty(globalThis, 'devicePixelRatio', { configurable: true, value: oldDpr });
  }
});

test('快速动作 initialAction 从配置事件接收并在指针释放后一次性消费', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 配置事件必须把后端初始动作写入 ref。
  assert.ok(source.includes('initialActionRef.current = nextBootstrap.initialAction;'), 'configure 必须写入快速动作');
  // 指针释放后读取并立即清空，确保同一会话内只触发一次。
  assert.ok(source.includes('const action = initialActionRef.current;'), '指针释放必须读取快速动作');
  assert.ok(source.includes("initialActionRef.current = '';"), '消费后必须清空快速动作');
  assert.ok(source.includes('void completeScreenshot(action);'), '快速动作必须直接走完成流程');
  // 顺序：读取 ref 必须在清空之前。
  const readIndex = source.indexOf('const action = initialActionRef.current;');
  const clearIndex = source.indexOf("initialActionRef.current = '';");
  assert.ok(readIndex !== -1 && clearIndex !== -1 && readIndex < clearIndex, '必须先读取再清空');
});
