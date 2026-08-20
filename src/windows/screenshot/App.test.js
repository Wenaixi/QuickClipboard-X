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
    screenshotHintsEnabled: true,
    lifecycleMode: 'quick',
  });
});

test('工具栏放置与定位统一走自适应模块', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const placement = toolbarPlacement(selection, bootstrap.bounds);'));
  assert.ok(source.includes('return toolbarStyleModel(selection, bootstrap.bounds, placement);'));
});

test('取消截图必须完整清理交互状态且清理先于后端请求', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const cancelStart = source.indexOf('const cancelScreenshot = async () => {');
  const cancelBody = source.slice(cancelStart, cancelStart + 700);
  // 清理完整性：取消必须清空手势、撤销历史、交互状态、放大镜、RAF、样式、光标。
  assert.ok(cancelBody.includes('gestureIdRef.current += 1;'), '取消必须作废当前手势');
  assert.ok(cancelBody.includes('selectionHistoryRef.current = [];'), '取消必须清空撤销历史');
  assert.ok(cancelBody.includes('resetInteractionState({ draftRef, selectionRef, moveRef, resizeRef, setSelection, setSelecting, setMoving, setResizing });'), '取消必须统一重置交互状态');
  assert.ok(cancelBody.includes('setMagnifierPoint(null);'), '取消必须清空放大镜点');
  assert.ok(cancelBody.includes('rafWriterRef.current?.cancel();'), '取消必须取消在飞绘制');
  assert.ok(cancelBody.includes("applySelectionStyle(rootRef.current, null);"), '取消必须清除选区样式');
  assert.ok(cancelBody.includes("rootRef.current.style.cursor = 'crosshair';"), '取消必须复位光标');
  // 顺序不变量：UI 清理必须先于向后端发 cancel 请求。若后端请求失败，界面已干净不卡。
  const resetIdx = cancelBody.indexOf('resetInteractionState(');
  const invokeIdx = cancelBody.indexOf("invoke(CANCEL_COMMAND, { sessionId })");
  assert.ok(resetIdx >= 0 && invokeIdx >= 0 && resetIdx < invokeIdx, 'UI 清理必须先于后端请求');
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

test('Ctrl+A 与调整中守卫必须先于动作与完成快捷键', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 顺序不变量：
  // 1) 选区调整进行中（draft/move/resize）必须提前 return，使 Enter/数字键/Ctrl+S 等
  //    完成快捷键不会在拖拽中途误触发完成（防误触核心）。
  // 2) Ctrl+A 全选必须在调整守卫之前处理，保证任何状态下按 Ctrl+A 都能全选。
  const ctrlAIndex = source.indexOf("if (event.ctrlKey && event.key.toLowerCase() === 'a') {", 610);
  const guardIndex = source.indexOf('busyAction || draftRef.current || moveRef.current || resizeRef.current) return;', 610);
  const hotkeyIndex = source.indexOf('const hotkeyAction = actionForHotkey(event.key);', 610);
  const completeIndex = source.indexOf('const completeAction = completeShortcutForEvent(event);', 610);
  assert.ok(ctrlAIndex >= 0, 'Ctrl+A 全选分支必须存在');
  assert.ok(guardIndex >= 0, '调整中守卫必须存在');
  assert.ok(hotkeyIndex >= 0 && completeIndex >= 0, '动作与完成快捷键必须存在');
  assert.ok(ctrlAIndex < guardIndex, 'Ctrl+A 必须先于调整守卫处理');
  assert.ok(guardIndex < hotkeyIndex && hotkeyIndex < completeIndex, '调整守卫必须先于动作与完成快捷键');
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

test('configure 复用窗口时复位帮助面板与处理中占用', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const start = source.indexOf('const configurePromise = listen(CONFIGURE_EVENT');
  const body = source.slice(start, source.indexOf('configurePromise\n      .then'));
  assert.ok(body.includes('setShowHelp(false);'), '复用窗口必须关闭上次的帮助面板');
  assert.ok(body.includes("setBusyAction('');"), '复用窗口必须清除上次的处理中占用');
});

test('拖拽草稿走统一选区生成且取消/配置统一重置交互状态', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('import { selectionFromDraft, resetInteractionState } from \'./draftModel.js\';'));
  assert.ok(source.includes('const next = selectionFromDraft(draft, event, bootstrap.bounds);'));
  const resets = source.match(/resetInteractionState\(\{ draftRef, selectionRef, moveRef, resizeRef, setSelection, setSelecting, setMoving, setResizing \}\);/g);
  assert.ok(resets && resets.length >= 2, '取消与配置重置必须都走统一交互重置');
});

test('遮罩与描边先于三分线标尺手柄渲染且手柄来自模型', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const renderStart = source.indexOf('<main ref={rootRef}');
  const render = source.slice(renderStart);
  // 叠层顺序不变量：四方向遮罩（顶部/左/右/底部）必须先于选区描边，
  // 选区描边必须先于三分线/标尺/手柄，保证视觉层级正确（遮罩在底层、手柄在最上层）。
  const maskTop = render.indexOf('screenshot-mask-top');
  const maskLeft = render.indexOf('screenshot-mask-left');
  const maskRight = render.indexOf('screenshot-mask-right');
  const maskBottom = render.indexOf('screenshot-mask-bottom');
  const selectionLine = render.indexOf('screenshot-selection-line');
  const thirds = render.indexOf('<ThirdsGrid bounds={bootstrap.bounds} />');
  const ruler = render.indexOf('<Ruler bounds={bootstrap.bounds} />');
  const handles = render.indexOf('{selection && <SelectionHandles selection={selection} />}');
  assert.ok([maskTop, maskLeft, maskRight, maskBottom, selectionLine, thirds, ruler, handles].every((i) => i >= 0), '遮罩/描边/三分线/标尺/手柄渲染必须全部存在');
  assert.ok(Math.max(maskTop, maskLeft, maskRight, maskBottom) < selectionLine, '四方向遮罩必须先于选区描边');
  assert.ok(selectionLine < thirds && thirds < ruler && ruler < handles, '描边必须先于三分线/标尺/手柄');
  // 手柄必须来自 selectionHandles 模型（8 个手柄小圆点由数据驱动渲染）。
  const selectionHandlesIdx = source.indexOf('selectionHandles(selection).map((handle) =>');
  assert.ok(selectionHandlesIdx >= 0, '手柄必须由 selectionHandles 数据驱动');
  assert.ok(selectionHandlesIdx < renderStart, 'SelectionHandles 组件必须定义在渲染结构之前');
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

test('十字参考线仅在拖拽中渲染且结束时清理指针点', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 源码护栏一：参考线渲染条件必须同时要求 magnifierPoint 且处于拖拽/移动/调整中，
  // 无选区未拖拽时参考线必须隐藏（禁止无条件渲染贯穿线干扰画面）。
  const guidesRender = '<CrosshairGuides point={magnifierPoint} bounds={bootstrap.bounds} />';
  const guidesCond = source.indexOf(guidesRender);
  assert.ok(guidesCond >= 0, '参考线渲染必须存在');
  const condStart = source.lastIndexOf('{magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && (', guidesCond);
  assert.ok(condStart >= 0, '参考线渲染必须受 magnifierPoint 与交互状态双重约束');
  assert.ok(condStart < guidesCond, '参考线渲染条件必须在渲染之前');
  // 源码护栏二：拖拽完成/调整完成/平移完成/取消四条路径都必须清理 magnifierPoint，
  // 否则参考线残留覆盖截图完成后的画面。
  const pointerUp = source.indexOf('const handlePointerUp');
  const upBody = source.slice(pointerUp, pointerUp + 4000);
  // 源码护栏三：草稿完成分支（单击/拖拽收尾）必须与调整/平移完成对称地清理 magnifierPoint。
  // 缺失时残留指针点会让下一次单击（pointerMove 未触发）显示旧位置参考线。
  const draftIdx = upBody.indexOf('const clicked = isClickGesture(draft.start, end);');
  assert.ok(draftIdx >= 0, '草稿完成分支必须存在');
  const draftTail = upBody.slice(draftIdx, draftIdx + 700);
  assert.ok(draftTail.includes('setMagnifierPoint(null)'), '草稿完成必须清理指针点');
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

test('尺寸标签四要素以分隔符相连且渲染取实时选区', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 文本模型：四个要素（像素/比例/百万像素/位置）必须全部用 ` · ` 分隔符连接，
  // 缺一个分隔符就会渲染成粘连文本；且分隔符必须来自常量拼接，保证渲染一致性。
  assert.ok(source.includes("return `${pixels} · ${ratio} · ${megapixels} · ${position}`;"), '四要素必须以三个分隔符相连');
  // 渲染接线：尺寸标签必须取实时选区（拖拽过程中即时更新），
  // 且三个辅助函数（文本/类名/样式）必须使用同一实时选区表达式。
  const liveCount = (source.match(/selectionSizeLabelText\(liveSelection \|\| selection, bootstrap\)/g) || []).length;
  assert.ok(liveCount >= 1, '渲染必须取实时选区');
  assert.ok(source.includes('selectionSizeLabelClass(liveSelection || selection, bootstrap.bounds)'), '类名必须取实时选区');
  assert.ok(source.includes('selectionSizeLabelStyle(liveSelection || selection, bootstrap.bounds)'), '样式必须取实时选区');
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

test('双击选区内部时触发复制完成动作且防重入', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 双击是选区完成的快捷入口：无选区/控件区/选区外必须直接返回。
  const doubleStart = source.indexOf('const handleDoubleClick = (event) => {');
  const doubleBody = source.slice(doubleStart, doubleStart + 500);
  assert.ok(doubleBody.includes("if (!selectionRef.current || busyAction) return;"), '无选区或动作中必须直接返回');
  assert.ok(doubleBody.includes("closest('[data-screenshot-control]')"), '控件区必须忽略双击');
  assert.ok(doubleBody.includes("hitSelectionInterior(point, selectionRef.current, 0)"), '必须命中选区内部才完成');
  assert.ok(doubleBody.includes("void completeScreenshot('copy');"), '双击必须触发复制完成动作');
});

test('双击不因第二击进入移动调整状态而失效且成功后统一清空引用', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const doubleStart = source.indexOf('const handleDoubleClick = (event) => {');
  const doubleBody = source.slice(doubleStart, doubleStart + 500);
  // 双击完成不因第二击（pointerdown 已把选区当内部命中并进入移动/调整模式）而失效：
  // 处理器只检查选区存在、busyAction、控件区、命中内部，绝不检查 moveRef/resizeRef/draftRef 状态。
  // 负向断言先剥行注释，规避注释含被测字面的误命中。
  const codeLines = doubleBody.split('\n').filter((line) => !line.trimStart().startsWith('//'));
  const stripped = codeLines.join('\n');
  assert.ok(!stripped.includes('moveRef.current'), '双击处理器不得因第二击移动状态失效');
  assert.ok(!stripped.includes('resizeRef.current'), '双击处理器不得因第二击调整状态失效');
  assert.ok(!stripped.includes('draftRef.current'), '双击处理器不得因第二击拖拽状态失效');
  // 双击成功完成复制后，第二击留下的移动/调整引用必须由成功路径统一清空（不残留孤儿状态）。
  const completeIdx = source.indexOf('const completeScreenshot = async (action) => {');
  const completeBody = source.slice(completeIdx, source.indexOf('const handlePointerDown', completeIdx));
  assert.ok(completeBody.includes('resetInteractionState({ draftRef, selectionRef, moveRef, resizeRef, setSelection, setSelecting, setMoving, setResizing });'), '成功路径必须清空第二击留下的引用');
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

test('选区边缘命中优先于内部平移且记录历史进入调整', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const downIndex = source.indexOf('const handlePointerDown = (event) => {');
  const body = source.slice(downIndex);
  // 顺序不变量：边缘命中检查必须先于内部平移检查。边缘点若落到平移分支，
  // 手柄拖拽会变成选区整体移动（ShareX 行为：边缘优先调整大小）。
  const edgeHit = body.indexOf('const edge = hitSelectionEdge(start, selectionRef.current, RESIZE_TOLERANCE);');
  const interiorHit = body.indexOf('if (hitSelectionInterior(start, selectionRef.current, MOVE_INSET)) {');
  assert.ok(edgeHit >= 0 && interiorHit >= 0 && edgeHit < interiorHit, '边缘命中必须先于内部平移');
  // 边缘命中后必须保留撤销历史（调整可被 Ctrl+Z 撤销）并进入 resize 模式。
  const resizeBlock = body.slice(edgeHit, edgeHit + 400);
  assert.ok(resizeBlock.includes('selectionHistoryRef.current = pushSelectionHistory(selectionHistoryRef.current, selectionRef.current);'), '边缘命中必须保留撤销历史');
  assert.ok(resizeBlock.includes('resizeRef.current = { pointerId: event.pointerId, edge };'), '边缘命中必须设置 resize 引用');
  assert.ok(resizeBlock.includes('setResizing(true);'), '边缘命中必须进入调整状态');
  // 内部命中（平移）也必须保留撤销历史。
  const moveBlock = body.slice(interiorHit, interiorHit + 350);
  assert.ok(moveBlock.includes('pushSelectionHistory'), '内部平移也必须保留撤销历史');
  assert.ok(moveBlock.includes('moveRef.current = { pointerId: event.pointerId, start, selectionStart: selectionRef.current };'), '内部命中必须设置移动引用');
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

test('Ctrl+Z 撤销在调整守卫后且空历史保护当前选区并双写同步', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const keydownIndex = source.indexOf('const handleKeyDown = (event) => {');
  const body = source.slice(keydownIndex);
  // 顺序不变量：撤销分支必须位于调整中守卫之后，拖拽/调整进行中 Ctrl+Z 不触发撤销（防误触）。
  const guardIndex = body.indexOf('busyAction || draftRef.current || moveRef.current || resizeRef.current) return;');
  const zIndex = body.indexOf("if (event.ctrlKey && event.key.toLowerCase() === 'z') {");
  assert.ok(guardIndex >= 0 && zIndex >= 0 && guardIndex < zIndex, '撤销分支必须位于调整中守卫之后');
  // 空历史保护：undo 返回 null 时必须直接返回，不得触碰/清空当前选区。
  const undoIdx = body.indexOf('const undone = undoSelectionHistory(selectionHistoryRef.current);');
  const nullGuard = body.indexOf('if (!undone) return;', undoIdx);
  assert.ok(nullGuard >= 0 && nullGuard < undoIdx + 200, 'undo 空历史必须直接返回保护当前选区');
  // 双写同步：撤销恢复必须同时写 selectionHistoryRef、selectionRef 与 setSelection，缺一不可。
  const undoBlock = body.slice(undoIdx, undoIdx + 400);
  assert.ok(undoBlock.includes('selectionHistoryRef.current = undone.history;'), '撤销后必须同步历史 ref');
  assert.ok(undoBlock.includes('selectionRef.current = undone.selection;'), '撤销后必须同步选区 ref');
  assert.ok(undoBlock.includes('setSelection(undone.selection);'), '撤销后必须同步选区 state');
  assert.ok(undoBlock.includes('applySelectionStyle(rootRef.current, undone.selection);'), '撤销后必须重绘样式');
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

test('动作工具栏快捷键提示完整且点击走单一完成入口', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // ACTIONS 定义：四个动作的快捷键映射必须完整正确（ai 无快捷键）。
  const actionsStart = source.indexOf('const ACTIONS = [');
  const actionsBody = source.slice(actionsStart, actionsStart + 200);
  assert.ok(actionsBody.includes("{ id: 'copy', shortcut: 'Enter' }"), 'copy 必须映射 Enter');
  assert.ok(actionsBody.includes("{ id: 'save', shortcut: 'Ctrl+S' }"), 'save 必须映射 Ctrl+S');
  assert.ok(actionsBody.includes("{ id: 'pin', shortcut: 'Ctrl+P' }"), 'pin 必须映射 Ctrl+P');
  assert.ok(actionsBody.includes("{ id: 'ai', shortcut: '' }"), 'ai 必须无默认快捷键');
  // 按钮 title 必须合并 action.shortcut 与 hotkeyForAction（用户悬停可见完整快捷键）。
  const toolbarStart = source.indexOf('className="screenshot-toolbar"');
  const toolbarBody = source.slice(toolbarStart, toolbarStart + 1200);
  assert.ok(toolbarBody.includes('title={t(\'screenshot.shortcutHint\', { label, shortcut: [action.shortcut, hotkeyForAction(action.id)].filter(Boolean).join(\' / \') })}'), '按钮 title 必须合并快捷键');
  // 按钮点击必须走 completeScreenshot 单一完成入口（不允许直接 invoke）。
  assert.ok(toolbarBody.includes('onClick={() => void completeScreenshot(action.id)}'), '按钮点击必须走完成入口');
});

test('工具栏动作按钮使用原生 button 且不被禁用时保持键盘可达', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 动作按钮必须用原生 button（天然 Tab 聚焦 + Enter/Space 触发），
  // 不能用 div role=button 代替，否则键盘导航与辅助技术支持断裂。
  const toolbarStart = source.indexOf('className="screenshot-toolbar"');
  const toolbarBody = source.slice(toolbarStart, toolbarStart + 1200);
  assert.ok(toolbarBody.includes('ACTIONS.map((action) => { const label = actionLabel(action.id, t); return <button key={action.id} type="button" className="screenshot-action" data-screenshot-control'), '工具栏动作必须是原生 button');
  // 不可禁用时不能加 tabIndex=-1 剥夺焦点（仅 disabled 会天然移出 Tab 序）。
  assert.ok(!toolbarBody.includes('tabIndex={-1}'), '工具栏按钮不得主动剥夺焦点');
  // 取消按钮同样是原生 button 且带可访问名称。
  assert.ok(source.includes('<button type="button" className="screenshot-cancel" data-screenshot-control'), '取消按钮必须是原生 button');
  assert.ok(source.includes('aria-label={t(\'screenshot.cancelLabel\')}'), '取消按钮必须有可访问名称');
});

test('新拖拽开始时先递增手势并清空选区历史', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 新拖拽（草稿选区）必须递增手势代号并清空历史：
  // 若只清空历史不递增手势，撤销会把旧会话的调整记录误应用到新选区。
  assert.ok(source.includes('gestureIdRef.current += 1;'), '新拖拽必须递增手势代号');
  // 锚定 handlePointerDown 内的草稿拖拽块（含唯一 draftRef 赋值），避免与
  // 其它 reset 路径的相邻序列误命中：手势递增必须紧邻并先于历史清空。
  const draftAnchor = "gestureIdRef.current += 1;\n    selectionHistoryRef.current = [];\n    draftRef.current = { start, end: start };";
  assert.ok(source.includes(draftAnchor), '手势递增必须紧邻并先于历史清空');
});

test('尺寸标注文案必须聚合像素/比例/百万像素/位置四要素', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 尺寸标签是选区最直观的信息载体：像素尺寸、宽高比、百万像素与屏幕位置缺一不可。
  assert.ok(source.includes('const pixels = formatPixelSize(physical);'), '尺寸文案必须包含像素尺寸');
  assert.ok(source.includes('const ratio = formatAspectRatio(physical);'), '尺寸文案必须包含宽高比');
  assert.ok(source.includes('const megapixels = formatMegapixels(physical);'), '尺寸文案必须包含百万像素');
  assert.ok(source.includes('formatSelectionPosition(selection'), '尺寸文案必须包含屏幕位置');
  assert.ok(source.includes('const physical = physicalSize(selection, bootstrap.dpr);'), '物理尺寸必须按 DPI 换算');
  // 四要素必须拼接进同一返回值。
  assert.ok(source.includes('return `${pixels} · ${ratio} · ${megapixels} · ${position}`;'), '四要素必须聚合进同一文案');
});

test('空闲与模式提示渲染受截图提示开关守卫', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 提示开关关闭时不得渲染空闲引导与模式提示；帮助面板（F1）是用户主动请求，不受此开关控制。
  assert.ok(source.includes('bootstrap.screenshotHintsEnabled && !selecting && !selection && !showHelp'), '空闲提示必须受提示开关守卫');
  assert.ok(source.includes('bootstrap.screenshotHintsEnabled && modeHint(modeForState'), '模式提示必须受提示开关守卫');
  // bootstrap 解析必须携带该字段（默认开启，与 Rust AppSettings 默认一致）。
  const model = readFileSync(new URL('./screenshotModel.js', import.meta.url), 'utf8');
  assert.ok(model.includes('screenshotHintsEnabled: payload.screenshotHintsEnabled !== false'), 'bootstrap 解析必须携带提示开关');
});

test('动作工具栏 id 与热键映射一致且处理中互斥显示 processing', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  const actionsStart = source.indexOf('const ACTIONS = [');
  const actionsBody = source.slice(actionsStart, actionsStart + 300);
  // 源码护栏一：ACTIONS 必须包含 copy/save/pin/ai 四个 id，且与 actionModel 热键映射值完全一致
  // （工具栏动作集合与数字键映射不得漂移）。
  const actionModel = readFileSync(new URL('./actionModel.js', import.meta.url), 'utf8');
  const expected = ['copy', 'save', 'pin', 'ai'];
  for (const id of expected) {
    assert.ok(actionModel.includes(`'${id}'`), `热键映射必须包含动作 ${id}`);
    assert.ok(actionsBody.includes(`id: '${id}'`), `ACTIONS 必须包含动作 ${id}`);
  }
  // 源码护栏二：处理中互斥——busyAction 非空时所有动作按钮禁用。
  const toolbarStart = source.indexOf('className="screenshot-toolbar"');
  const toolbarBody = source.slice(toolbarStart, toolbarStart + 1200);
  assert.ok(toolbarBody.includes('disabled={Boolean(busyAction) || !actionIsEnabled(action.id, bootstrap)}'), '处理中必须禁用全部动作按钮');
  // 源码护栏三：处理中的当前动作按钮必须显示 processing 文案而非普通标签。
  assert.ok(toolbarBody.includes("busyAction === action.id ? t('screenshot.processing') : label"), '处理中的当前动作必须显示 processing 文案');
  // 源码护栏四：完成入口必须二次校验可用性（按钮禁用只是第一道防线）。
  const completeStart = source.indexOf('const completeScreenshot = async');
  const completeBody = source.slice(completeStart, completeStart + 900);
  assert.ok(completeBody.includes('if (!actionIsEnabled(action, bootstrap)) {'), '完成入口必须二次校验动作可用性');
});

test('AI 未配置时工具栏禁用 AI 动作且数字键 4 引导进入设置', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 工具栏按钮：disabled 判定必须与可用性函数一致。
  assert.ok(source.includes('disabled={Boolean(busyAction) || !actionIsEnabled(action.id, bootstrap)}'), '按钮禁用必须跟随可用性');
  // completeScreenshot 内二次守卫：AI 未配置时按数字键 4 引导进入设置（与按钮点击一致）。
  assert.ok(source.includes("if (action === 'ai') void openAiSettings();"), 'AI 未配置时数字键 4 必须引导进入设置');
  assert.ok(source.includes('return action !== \'ai\' || (bootstrap.screenshotAiEnabled !== false && bootstrap.screenshotAiConfigured === true);'), 'AI 可用性判断必须要求已配置');
});

test('方向键微调排除修饰键且每次保留撤销历史', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 方向键微调只响应纯方向键；alt/meta 组合留给其它语义（窗口切换等）。
  const nudgeStart = source.indexOf("if (nudgeX !== undefined && !event.altKey && !event.metaKey) {", 620);
  const nudgeBlock = source.slice(nudgeStart, nudgeStart + 400);
  assert.ok(nudgeBlock.includes("!event.altKey && !event.metaKey"), '必须排除 alt/meta 修饰键');
  // 每次微调前保留撤销历史：方向键连续操作可逐步撤销。
  assert.ok(nudgeBlock.includes("selectionHistoryRef.current = pushSelectionHistory(selectionHistoryRef.current, selectionRef.current);"), '微调前必须保留撤销历史');
  // 步长必须由 Ctrl 快进/普通两种取值决定。
  assert.ok(nudgeBlock.includes("event.ctrlKey ? NUDGE_FAST_STEP : NUDGE_STEP"), '步长必须区分快进与普通');
});

test('截图键盘微调接线调用 nudgeSelection 并同步选区状态', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  assert.ok(source.includes('const next = nudgeSelection(selectionRef.current, nudgeX * step, nudgeY * step, bootstrap.bounds);'));
  assert.ok(source.includes('setSelection(next);'));
  assert.ok(source.includes('applySelectionStyle(rootRef.current, next);'));
});

test('completeScreenshot 失败分支设置可见错误且不卡处理中', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 失败处理不变量：
  // 1) 无 sessionId 时必须设置 sessionNotReady 错误（用户可见）。
  // 2) invoke 抛错时 catch 必须设置 actionFailed 错误（用户可见），不能静默吞掉。
  // 3) catch 内设置错误早于 finally 解除占用，失败也不会永久卡在处理中。
  const start = source.indexOf('const completeScreenshot = async (action) => {');
  const body = source.slice(start, source.indexOf('const handlePointerDown', start));
  assert.ok(body.includes("setActionError(t('screenshot.sessionNotReady'))"), '无 session 必须设置 sessionNotReady');
  assert.ok(body.includes("setActionError(t('screenshot.actionFailed', { action: actionLabel(action, t), error: String(error) }))"), 'invoke 失败必须设置 actionFailed');
  const catchIdx = body.indexOf("setActionError(t('screenshot.actionFailed'");
  const finallyIdx = body.indexOf('} finally {');
  assert.ok(catchIdx >= 0 && finallyIdx >= 0 && catchIdx < finallyIdx, '错误提示必须设置在 finally 解除占用之前');
});

test('completeScreenshot 防重入且 finally 必解除占用并 dispose 条件关闭', () => {
  const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
  // 防重入核心：动作处理中禁止二次触发（工具栏/快捷键/双击都走此入口）。
  assert.ok(source.includes('const completeScreenshot = async (action) => {\n    const currentSelection = selectionRef.current;\n    if (!currentSelection || busyAction) return;'), '动作入口必须防重入');
  assert.ok(source.includes('setBusyAction(action);'), '执行前必须标记占用');
  // 无论成功失败都解除占用：finally 语义防止成功路径永久卡在处理中。
  const finallyIdx = source.indexOf('// 无论成功失败都解除动作占用，避免成功路径永久卡在处理中。');
  assert.ok(finallyIdx !== -1, '必须显式注释 finally 解除语义');
  assert.ok(source.includes('} finally {\n      // 无论成功失败都解除动作占用，避免成功路径永久卡在处理中。\n      setBusyAction(\'\');\n    }'), 'finally 必须解除动作占用');
  // 成功路径按生命周期模式条件关闭窗口（dispose 销毁 / quick 复用）。
  assert.ok(source.includes("if (bootstrap.lifecycleMode === 'dispose') {"), '成功路径必须按生命周期模式条件关闭');
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
      screenshotHintsEnabled: true,
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
