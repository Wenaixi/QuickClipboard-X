import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { normalizeBootstrap } from './screenshotModel.js';
import { magnifierGeometry } from './magnifierModel.js';
import { magnifierGridLines, magnifierCrosshair } from './magnifierGridModel.js';
import { coordinatePanelPosition, formatCursorCoordinate } from './coordinateModel.js';
import { guideLines } from './guideModel.js';
import { thirdsGrid } from './gridModel.js';
import { actionForHotkey, hotkeyForAction } from './actionModel.js';
import { selectionLabelPlacement } from './labelModel.js';
import { cursorForSelectionHover } from './cursorModel.js';
import { lineStyle } from './annotationModel.js';
import { readCenterPixel, formatRgb } from './colorModel.js';
import { magnifierScaleForWheel } from './magnifierZoomModel.js';
import { formatPixelSize, formatMegapixels, formatAspectRatio, physicalSize } from './sizeModel.js';
import { magnetSelection } from './magnetModel.js';
import { toolbarPlacement, toolbarStyle as toolbarStyleModel } from './toolbarModel.js';
import { pushSelectionHistory, undoSelectionHistory } from './historyModel.js';
import { rulerTicks } from './rulerModel.js';
import { modeForState, modeHint } from './modeModel.js';
import { selectionHandles } from './handleModel.js';
import { formatSelectionPosition } from './positionModel.js';
import { selectionFromDraft, resetInteractionState } from './draftModel.js';
import { helpEntries, isHelpShortcut } from './helpModel.js';
import { canResetSelection } from './resetModel.js';
import { fullScreenSelection } from './fullScreenModel.js';
import { completeShortcutForEvent } from './completeShortcutModel.js';
import { idleHint } from './idleModel.js';
import { includeInvite } from './inviteModel.js';
import {
  createRafWriter,
  hitSelectionEdge,
  hitSelectionInterior,
  isClickGesture,
  isCurrentGesture,
  normalizeSelection,
  nudgeSelection,
  resizeSelection,
  selectionForPointerGesture,
  selectionFromPhysical,
  selectionToPhysical,
  squareSelection,
} from './selectionModel.js';

const CONFIGURE_EVENT = 'screenshot:configure';
const VIEWPORT_READY_COMMAND = 'screenshot_window_ready';
const COMPLETE_COMMAND = 'complete_screenshot';
const CANCEL_COMMAND = 'cancel_screenshot';
const FIND_WINDOW_COMMAND = 'find_screenshot_window_at_point';

const ACTIONS = [
  { id: 'copy', shortcut: 'Enter' },
  { id: 'save', shortcut: 'Ctrl+S' },
  { id: 'pin', shortcut: 'Ctrl+P' },
  { id: 'ai', shortcut: '' },
];

const NUDGE_STEP = 1;
const NUDGE_FAST_STEP = 10;
const MOVE_INSET = 4;
const RESIZE_TOLERANCE = 4;
const DEFAULT_MAGNIFIER_SCALE = 6;
const NUDGE_DIRECTIONS = {
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
};

function pointFromPointerEvent(event, element) {
  const rect = element.getBoundingClientRect();
  return {
    x: event.clientX - rect.left,
    y: event.clientY - rect.top,
  };
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function selectionSizeLabelText(selection, bootstrap) {
  const physical = physicalSize(selection, bootstrap.dpr);
  const pixels = formatPixelSize(physical);
  const ratio = formatAspectRatio(physical);
  const megapixels = formatMegapixels(physical);
  const position = formatSelectionPosition(selection, { dpr: bootstrap.dpr, monitorLeft: bootstrap.monitorLeft, monitorTop: bootstrap.monitorTop });
  return `${pixels} · ${ratio} · ${megapixels} · ${position}`;
}

function selectionSizeLabelClass(selection, bounds) {
  const placement = selectionLabelPlacement(selection, bounds);
  const classes = ['screenshot-selection-size'];
  if (!placement.above) classes.push('screenshot-selection-size-below');
  if (placement.alignLeft) classes.push('screenshot-selection-size-left');
  return classes.join(' ');
}

function selectionLineStyle() {
  const { borderWidth, borderColor } = lineStyle(1, 'rgba(255, 255, 255, 0.96)', 1);
  return { borderWidth, borderColor };
}

function selectionSizeLabelStyle(selection, bounds) {
  const placement = selectionLabelPlacement(selection, bounds);
  return placement.alignLeft ? { left: `${selection.left}px` } : { right: `${Math.max(0, bounds.width - selection.right)}px` };
}

function coordinatePanelStyle(point, bounds) {
  const position = coordinatePanelPosition(point, bounds);
  return {
    left: `${position.left}px`,
    top: `${position.top}px`,
  };
}

function ThirdsGrid({ bounds }) {
  const grid = thirdsGrid(bounds);
  return (
    <>
      {grid.vertical.map((line) => <div key={`v-${line.left}`} aria-hidden="true" className="screenshot-thirds screenshot-thirds-v" data-screenshot-thirds="true" style={{ left: `${line.left}px`, top: `${line.top}px`, width: `${line.width}px`, height: `${line.height}px` }} />)}
      {grid.horizontal.map((line) => <div key={`h-${line.top}`} aria-hidden="true" className="screenshot-thirds screenshot-thirds-h" data-screenshot-thirds="true" style={{ left: `${line.left}px`, top: `${line.top}px`, width: `${line.width}px`, height: `${line.height}px` }} />)}
    </>
  );
}

function Ruler({ bounds }) {
  const horizontalTicks = rulerTicks(bounds.width);
  const verticalTicks = rulerTicks(bounds.height);
  return (
    <>
      <div className="screenshot-ruler screenshot-ruler-h" aria-hidden="true">
        {horizontalTicks.map((tick) => (
          <span key={tick.position} className={tick.label !== null ? 'screenshot-ruler-tick screenshot-ruler-tick-major' : 'screenshot-ruler-tick'} style={{ left: `${tick.position}px` }}>{tick.label ?? ''}</span>
        ))}
      </div>
      <div className="screenshot-ruler screenshot-ruler-v" aria-hidden="true">
        {verticalTicks.map((tick) => (
          <span key={tick.position} className={tick.label !== null ? 'screenshot-ruler-tick screenshot-ruler-tick-major' : 'screenshot-ruler-tick'} style={{ top: `${tick.position}px` }}>{tick.label ?? ''}</span>
        ))}
      </div>
    </>
  );
}

function SelectionHandles({ selection }) {
  return (
    <>
      {selectionHandles(selection).map((handle) => (
        <div key={handle.edge} aria-hidden="true" className={`screenshot-handle screenshot-handle-${handle.edge}`} data-screenshot-handle="true" style={{ left: `${handle.left}px`, top: `${handle.top}px` }} />
      ))}
    </>
  );
}

function CrosshairGuides({ point, bounds }) {
  const lines = guideLines(point, bounds);
  return (
    <>
      <div aria-hidden="true" className="screenshot-guide screenshot-guide-v" data-screenshot-guide="true" style={{ left: `${lines.vertical.left}px`, top: `${lines.vertical.top}px`, width: `${lines.vertical.width}px`, height: `${lines.vertical.height}px` }} />
      <div aria-hidden="true" className="screenshot-guide screenshot-guide-h" data-screenshot-guide="true" style={{ left: `${lines.horizontal.left}px`, top: `${lines.horizontal.top}px`, width: `${lines.horizontal.width}px`, height: `${lines.horizontal.height}px` }} />
    </>
  );
}

function magnifierCanvasStyle(point, bounds, scale = 6) {
  const geometry = magnifierGeometry(point, bounds, { scale });
  return {
    left: `${geometry.panel.left}px`,
    top: `${geometry.panel.top}px`,
    width: `${geometry.panel.width}px`,
    height: `${geometry.panel.height}px`,
  };
}

// 颜色板跟随放大镜翻转：放大镜贴底翻转上置时，颜色板放放大镜上方，
// 避免 `top = panelTop + panelHeight + 4` 在贴底时溢出 bounds.height 被裁掉。
function colorPanelStyle(magnifierLayout, bounds) {
  const panelTop = parseFloat(magnifierLayout.top);
  const panelHeight = parseFloat(magnifierLayout.height);
  const below = panelTop + panelHeight + 4;
  if (below + 20 <= bounds.height) {
    return { left: magnifierLayout.left, top: `${below}px`, backgroundColor: undefined };
  }
  return { left: magnifierLayout.left, top: `${Math.max(0, panelTop - 24)}px`, backgroundColor: undefined };
}

function applySelectionStyle(element, selection) {
  if (!element) return;
  if (!selection) {
    element.dataset.selectionActive = 'false';
    return;
  }
  element.dataset.selectionActive = 'true';
  element.style.setProperty('--selection-left', `${selection.left}px`);
  element.style.setProperty('--selection-top', `${selection.top}px`);
  element.style.setProperty('--selection-right', `${selection.right}px`);
  element.style.setProperty('--selection-bottom', `${selection.bottom}px`);
  element.style.setProperty('--selection-width', `${selection.width}px`);
  element.style.setProperty('--selection-height', `${selection.height}px`);
}

function actionLabel(action, t) {
  return t(`screenshot.actions.${action}`, { defaultValue: action });
}

function actionIsEnabled(action, bootstrap) {
  return action !== 'ai' || (bootstrap.screenshotAiEnabled !== false && bootstrap.screenshotAiConfigured === true);
}

function isPrimaryPointer(event) {
  return event.button === 0 || (event.pointerType === 'touch' && event.isPrimary);
}

function App() {
  const { t } = useTranslation();
  const rootRef = useRef(null);
  const magnifierCanvasRef = useRef(null);
  const rafWriterRef = useRef(null);
  const draftRef = useRef(null);
  const selectionRef = useRef(null);
  const pointerIdRef = useRef(null);
  const moveRef = useRef(null);
  const resizeRef = useRef(null);
  const selectionHistoryRef = useRef([]);
  const gestureIdRef = useRef(0);
  const [bootstrap, setBootstrap] = useState(() => normalizeBootstrap(globalThis.__QC_SCREENSHOT_BOOT__ || {}, {
    width: globalThis.innerWidth,
    height: globalThis.innerHeight,
    devicePixelRatio: globalThis.devicePixelRatio,
  }));
  const [selection, setSelection] = useState(null);
  const [showHelp, setShowHelp] = useState(false);
  const [selecting, setSelecting] = useState(false);
  const [moving, setMoving] = useState(false);
  const [resizing, setResizing] = useState(false);
  const [magnifierPoint, setMagnifierPoint] = useState(null);
  const [magnifierColor, setMagnifierColor] = useState(null);
  const [magnifierScale, setMagnifierScale] = useState(DEFAULT_MAGNIFIER_SCALE);
  const [busyAction, setBusyAction] = useState('');
  const [actionError, setActionError] = useState('');
  const initialActionRef = useRef('');

  const toolbarStyle = useMemo(() => {
    if (!selection) return undefined;
    const placement = toolbarPlacement(selection, bootstrap.bounds);
    return toolbarStyleModel(selection, bootstrap.bounds, placement);
  }, [bootstrap.bounds, selection]);

  useEffect(() => {
    const root = rootRef.current;
    if (!root) return undefined;
    rafWriterRef.current = createRafWriter((nextSelection) => applySelectionStyle(root, nextSelection));
    return () => {
      rafWriterRef.current?.cancel();
      rafWriterRef.current = null;
    };
  }, []);

  useEffect(() => {
    const canvas = magnifierCanvasRef.current;
    if (!canvas || !magnifierPoint || !bootstrap.magnifierBackground) {
      setMagnifierColor(null);
      return undefined;
    }
    // 过期帧令牌：鼠标快速移动时旧 Image 的 onload 可能晚于新帧触发，
    // 闭包捕获旧 magnifierPoint 覆盖新渲染并污染颜色读数，必须按代丢弃。
    let generation = 0;
    const image = new Image();
    image.onload = () => {
      const frame = generation;
      const geometry = magnifierGeometry(magnifierPoint, bootstrap.bounds, { scale: magnifierScale });
      canvas.width = geometry.panel.width;
      canvas.height = geometry.panel.height;
      const context = canvas.getContext('2d');
      if (!context) return;
      context.imageSmoothingEnabled = false;
      context.clearRect(0, 0, canvas.width, canvas.height);
      const dpr = bootstrap.dpr;
      // 背景快照是物理像素 PNG，采样源几何为逻辑像素，绘制前按 DPR 换算避免高 DPI 错位。
      context.drawImage(
        image,
        geometry.source.left * dpr,
        geometry.source.top * dpr,
        geometry.source.cols * dpr,
        geometry.source.rows * dpr,
        0,
        0,
        canvas.width,
        canvas.height
      );
      // 必须在绘制任何叠加线之前读取中心像素：十字线 alpha=0.85 会污染
      // 中心像素的 RGB，导致颜色板永远偏红。
      const centerPixel = readCenterPixel(context.getImageData(0, 0, canvas.width, canvas.height).data, canvas.width, canvas.height);
      if (frame !== generation) return;
      const grid = magnifierGridLines(geometry);
      const cross = magnifierCrosshair(geometry);
      context.strokeStyle = 'rgba(255, 255, 255, 0.28)';
      context.lineWidth = 1;
      context.beginPath();
      for (const x of grid.vertical) { context.moveTo(x + 0.5, 0); context.lineTo(x + 0.5, canvas.height); }
      for (const y of grid.horizontal) { context.moveTo(0, y + 0.5); context.lineTo(canvas.width, y + 0.5); }
      context.stroke();
      context.strokeStyle = 'rgba(255, 80, 80, 0.85)';
      context.beginPath();
      context.moveTo(cross.x + 0.5, 0);
      context.lineTo(cross.x + 0.5, canvas.height);
      context.moveTo(0, cross.y + 0.5);
      context.lineTo(canvas.width, cross.y + 0.5);
      context.stroke();
      if (frame !== generation) return;
      setMagnifierColor(centerPixel);
    };
    image.src = bootstrap.magnifierBackground;
    return () => {
      generation += 1;
      image.onload = null;
    };
  }, [bootstrap.magnifierBackground, bootstrap.bounds, magnifierPoint, magnifierScale]);

  useEffect(() => {
    let active = true;
    let unlisten;
    const configurePromise = listen(CONFIGURE_EVENT, (event) => {
      if (!active) return;
      const nextBootstrap = normalizeBootstrap(event.payload || {}, {
        width: globalThis.innerWidth,
        height: globalThis.innerHeight,
        devicePixelRatio: globalThis.devicePixelRatio,
      });
      setBootstrap(nextBootstrap);
      initialActionRef.current = nextBootstrap.initialAction;
      gestureIdRef.current += 1;
      selectionHistoryRef.current = [];
      resetInteractionState({ draftRef, selectionRef, moveRef, resizeRef, setSelection, setSelecting, setMoving, setResizing });
      setMagnifierPoint(null);
      setMagnifierScale(DEFAULT_MAGNIFIER_SCALE);
      setActionError('');
      applySelectionStyle(rootRef.current, null);
      if (rootRef.current) rootRef.current.style.cursor = 'crosshair';
    });
    configurePromise
      .then((cleanup) => {
        if (!active) {
          cleanup();
          return;
        }
        unlisten = cleanup;
        invoke(VIEWPORT_READY_COMMAND).catch(() => {});
      })
      .catch(() => {});
    return () => {
      active = false;
      unlisten?.();
    };
  }, []);

  const openAiSettings = async () => {
    try {
      await cancelScreenshot();
      await invoke('open_settings_window');
    } catch (error) {
      setActionError(t('screenshot.openAiSettingsFailed', { error: String(error) }));
    }
  };

  const cancelScreenshot = async () => {
    const sessionId = bootstrap.sessionId;
    gestureIdRef.current += 1;
    selectionHistoryRef.current = [];
    resetInteractionState({ draftRef, selectionRef, moveRef, resizeRef, setSelection, setSelecting, setMoving, setResizing });
    setMagnifierPoint(null);
    setMagnifierScale(DEFAULT_MAGNIFIER_SCALE);
    rafWriterRef.current?.cancel();
    applySelectionStyle(rootRef.current, null);
    if (rootRef.current) rootRef.current.style.cursor = 'crosshair';
    setActionError('');
    if (sessionId) {
      try { await invoke(CANCEL_COMMAND, { sessionId }); }
      catch (error) { setActionError(t('screenshot.cancelFailed', { error: String(error) })); return; }
    }
    try { await getCurrentWindow().close(); } catch {}
  };

  const completeScreenshot = async (action) => {
    const currentSelection = selectionRef.current;
    if (!currentSelection || busyAction) return;
    if (!actionIsEnabled(action, bootstrap)) {
      // 未配置 AI 时按数字键 4 引导进入设置，与工具栏“配置 AI”按钮行为一致。
      if (action === 'ai') void openAiSettings();
      return;
    }
    if (!bootstrap.sessionId) {
      setActionError(t('screenshot.sessionNotReady'));
      return;
    }
    setBusyAction(action);
    setActionError('');
    try {
      const physicalSelection = selectionToPhysical(currentSelection, bootstrap.dpr, bootstrap.physicalBounds);
      await invoke(COMPLETE_COMMAND, { sessionId: bootstrap.sessionId, selection: physicalSelection, action });
    } catch (error) {
      setActionError(t('screenshot.actionFailed', { action: actionLabel(action, t), error: String(error) }));
    } finally {
      // 无论成功失败都解除动作占用，避免成功路径永久卡在处理中。
      setBusyAction('');
    }
  };

  const handlePointerDown = (event) => {
    if (!isPrimaryPointer(event) || busyAction) return;
    const target = event.target;
    if (target instanceof Element && target.closest('[data-screenshot-control]')) return;
    event.preventDefault();
    const root = rootRef.current;
    if (!root) return;
    const start = pointFromPointerEvent(event, root);
    if (selectionRef.current) {
      const edge = hitSelectionEdge(start, selectionRef.current, RESIZE_TOLERANCE);
      if (edge) {
        selectionHistoryRef.current = pushSelectionHistory(selectionHistoryRef.current, selectionRef.current);
        resizeRef.current = { pointerId: event.pointerId, edge };
        setSelecting(false);
        setResizing(true);
        root.setPointerCapture?.(event.pointerId);
        return;
      }
      if (hitSelectionInterior(start, selectionRef.current, MOVE_INSET)) {
        selectionHistoryRef.current = pushSelectionHistory(selectionHistoryRef.current, selectionRef.current);
        moveRef.current = { pointerId: event.pointerId, start, selectionStart: selectionRef.current };
        setSelecting(false);
        setMoving(true);
        root.setPointerCapture?.(event.pointerId);
        return;
      }
    }
    gestureIdRef.current += 1;
    selectionHistoryRef.current = [];
    draftRef.current = { start, end: start };
    pointerIdRef.current = event.pointerId;
    root.setPointerCapture?.(event.pointerId);
    setSelecting(true);
    selectionRef.current = null;
    setSelection(null);
    setActionError('');
    root.style.cursor = 'crosshair';
    applySelectionStyle(root, normalizeSelection(start, start, bootstrap.bounds));
  };

  const handlePointerMove = (event) => {
    if (busyAction) return;
    const root = rootRef.current;
    const resizing = resizeRef.current;
    if (resizing && root && event.pointerId === resizing.pointerId) {
      const current = pointFromPointerEvent(event, root);
      setMagnifierPoint(current);
      const next = magnetSelection(resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey, fromCenter: event.ctrlKey }), bootstrap.bounds, { edge: resizing.edge });
      selectionRef.current = next;
      rafWriterRef.current?.schedule(next);
      return;
    }
    const moving = moveRef.current;
    if (moving && root && event.pointerId === moving.pointerId) {
      const current = pointFromPointerEvent(event, root);
      setMagnifierPoint(current);
      const next = magnetSelection(nudgeSelection(moving.selectionStart, current.x - moving.start.x, current.y - moving.start.y, bootstrap.bounds), bootstrap.bounds);
      selectionRef.current = next;
      rafWriterRef.current?.schedule(next);
      return;
    }
    const draft = draftRef.current;
    if (!draft) {
      if (!root) return;
      const hover = cursorForSelectionHover(pointFromPointerEvent(event, root), selectionRef.current, bootstrap.bounds);
      root.style.cursor = hover || 'crosshair';
      return;
    }
    if (!root || event.pointerId !== pointerIdRef.current) return;
    draft.end = pointFromPointerEvent(event, root);
    setMagnifierPoint(draft.end);
    const next = selectionFromDraft(draft, event, bootstrap.bounds);
    rafWriterRef.current?.schedule(next);
  };

  const handleWheel = (event) => {
    if (!magnifierPoint) return;
    const target = event.target;
    if (target instanceof Element && target.closest('[data-screenshot-control]')) return;
    event.preventDefault();
    setMagnifierScale((current) => magnifierScaleForWheel(current, event.deltaY));
  };

  const handleDoubleClick = (event) => {
    if (!selectionRef.current || busyAction) return;
    const target = event.target;
    if (target instanceof Element && target.closest('[data-screenshot-control]')) return;
    const root = rootRef.current;
    if (!root) return;
    const point = pointFromPointerEvent(event, root);
    if (!hitSelectionInterior(point, selectionRef.current, 0)) return;
    event.preventDefault();
    void completeScreenshot('copy');
  };

  const handlePointerUp = async (event) => {
    const root = rootRef.current;
    const resizing = resizeRef.current;
    if (resizing && root && event.pointerId === resizing.pointerId) {
      event.preventDefault();
      const current = pointFromPointerEvent(event, root);
      const finalSelection = magnetSelection(resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey, fromCenter: event.ctrlKey }), bootstrap.bounds, { edge: resizing.edge });
      resizeRef.current = null;
      setSelecting(false);
      setResizing(false);
      pointerIdRef.current = null;
      setMagnifierPoint(null);
      rafWriterRef.current?.cancel();
      root.releasePointerCapture?.(event.pointerId);
      applySelectionStyle(root, finalSelection);
      selectionRef.current = finalSelection;
      setSelection(finalSelection);
      return;
    }
    const moving = moveRef.current;
    if (moving && root && event.pointerId === moving.pointerId) {
      event.preventDefault();
      const current = pointFromPointerEvent(event, root);
      const finalSelection = magnetSelection(nudgeSelection(moving.selectionStart, current.x - moving.start.x, current.y - moving.start.y, bootstrap.bounds), bootstrap.bounds);
      moveRef.current = null;
      setSelecting(false);
      setMoving(false);
      pointerIdRef.current = null;
      setMagnifierPoint(null);
      rafWriterRef.current?.cancel();
      root.releasePointerCapture?.(event.pointerId);
      applySelectionStyle(root, finalSelection);
      selectionRef.current = finalSelection;
      setSelection(finalSelection);
      return;
    }
    const draft = draftRef.current;
    if (!draft || !root || event.pointerId !== pointerIdRef.current) return;
    event.preventDefault();
    const end = pointFromPointerEvent(event, root);
    const clicked = isClickGesture(draft.start, end);
    const gestureId = gestureIdRef.current;
    draftRef.current = null;
    pointerIdRef.current = null;
    rafWriterRef.current?.cancel();
    root.releasePointerCapture?.(event.pointerId);

    const square = event.shiftKey && !clicked ? squareSelection(draft.start, end, bootstrap.bounds) : null;
    let finalSelection = square ?? selectionForPointerGesture(draft.start, end, bootstrap.bounds);
    if (!square && clicked && bootstrap.sessionId) {
      try {
        const physicalSelection = await invoke(FIND_WINDOW_COMMAND, {
          sessionId: bootstrap.sessionId,
          x: Math.round((bootstrap.monitorLeft + draft.start.x) * bootstrap.dpr),
          y: Math.round((bootstrap.monitorTop + draft.start.y) * bootstrap.dpr),
        });
        if (!isCurrentGesture(gestureId, gestureIdRef.current)) return;
        if (physicalSelection) {
          finalSelection = selectionFromPhysical(physicalSelection, bootstrap.dpr, bootstrap.bounds);
        }
      } catch {
        // 未命中可截图窗口时保留 1×1 的单击选区，用户仍可继续拖动选择。
      }
    }

    // 单击选窗保留窗口精确矩形不吸附；拖动（含正方形）收尾时吸附到屏幕引导线。
    if (!clicked) finalSelection = magnetSelection(finalSelection, bootstrap.bounds);

    if (!isCurrentGesture(gestureId, gestureIdRef.current)) return;
    applySelectionStyle(root, finalSelection);
    selectionRef.current = finalSelection;
    setSelection(finalSelection);
    setSelecting(false);
    setMoving(false);
    setResizing(false);
    if (initialActionRef.current) {
      const action = initialActionRef.current;
      initialActionRef.current = '';
      void completeScreenshot(action);
    }
  };

  useEffect(() => {
    const handleKeyDown = (event) => {
      if (event.key === 'Escape' && showHelp) { event.preventDefault(); setShowHelp(false); return; }
      if (event.key === 'Escape' && (!selectionRef.current || canResetSelection(selectionRef.current))) { event.preventDefault(); void cancelScreenshot(); return; }
      if (event.target instanceof Element && event.target.closest('[data-screenshot-control]')) return;
      if (isHelpShortcut(event)) { event.preventDefault(); setShowHelp((current) => !current); return; }
      if (event.ctrlKey && event.key.toLowerCase() === 'a') {
        event.preventDefault();
        if (selectionRef.current) {
          selectionHistoryRef.current = pushSelectionHistory(selectionHistoryRef.current, selectionRef.current);
        }
        const next = fullScreenSelection(bootstrap.bounds);
        selectionRef.current = next;
        setSelecting(false);
        setMoving(false);
        setResizing(false);
        setSelection(next);
        applySelectionStyle(rootRef.current, next);
        return;
      }
      if (!selectionRef.current || busyAction || draftRef.current || moveRef.current || resizeRef.current) return;
      const [nudgeX, nudgeY] = NUDGE_DIRECTIONS[event.key] || [];
      if (nudgeX !== undefined && !event.altKey && !event.metaKey) {
        event.preventDefault();
        const step = event.ctrlKey ? NUDGE_FAST_STEP : NUDGE_STEP;
        selectionHistoryRef.current = pushSelectionHistory(selectionHistoryRef.current, selectionRef.current);
        const next = nudgeSelection(selectionRef.current, nudgeX * step, nudgeY * step, bootstrap.bounds);
        selectionRef.current = next;
        setSelection(next);
        applySelectionStyle(rootRef.current, next);
        return;
      }
      if (event.ctrlKey && event.key.toLowerCase() === 'z') {
        event.preventDefault();
        const undone = undoSelectionHistory(selectionHistoryRef.current);
        if (!undone) return;
        selectionHistoryRef.current = undone.history;
        selectionRef.current = undone.selection;
        setSelection(undone.selection);
        applySelectionStyle(rootRef.current, undone.selection);
        return;
      }
      const hotkeyAction = actionForHotkey(event.key);
      if (hotkeyAction) { event.preventDefault(); void completeScreenshot(hotkeyAction); return; }
      const completeAction = completeShortcutForEvent(event);
      if (completeAction) { event.preventDefault(); void completeScreenshot(completeAction); }
    };
    const handleBlur = () => { if (busyAction) return; if (draftRef.current || selectionRef.current) void cancelScreenshot(); };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('blur', handleBlur);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('blur', handleBlur);
    };
  }, [bootstrap.sessionId, bootstrap.bounds, busyAction, showHelp]);

  const magnifierLayout = useMemo(() => (
    magnifierPoint ? magnifierCanvasStyle(magnifierPoint, bootstrap.bounds, magnifierScale) : null
  ), [bootstrap.bounds, magnifierPoint, magnifierScale]);

  return (
    <main ref={rootRef} className="screenshot-root" data-selection-active="false" data-selecting={selecting ? 'true' : 'false'} onPointerDown={handlePointerDown} onPointerMove={handlePointerMove} onPointerUp={handlePointerUp} onPointerCancel={cancelScreenshot} onDoubleClick={handleDoubleClick} onWheel={handleWheel} onContextMenu={(event) => { event.preventDefault(); if (!selectionRef.current || canResetSelection(selectionRef.current)) void cancelScreenshot(); }} aria-label={t('screenshot.selectionLabel')}>
      <div className="screenshot-mask screenshot-mask-top" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-left" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-right" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-bottom" aria-hidden="true" />
      <div className="screenshot-selection screenshot-selection-line" aria-hidden="true" style={selectionLineStyle()}>{selection && <span className={selectionSizeLabelClass(selection, bootstrap.bounds)} style={selectionSizeLabelStyle(selection, bootstrap.bounds)}>{selectionSizeLabelText(selection, bootstrap)}</span>}</div>
      {selection && <div className="screenshot-toolbar" style={toolbarStyle} role="toolbar" aria-label={t('screenshot.toolbarLabel')} data-screenshot-control onPointerDown={(event) => event.stopPropagation()}>{ACTIONS.map((action) => { const label = actionLabel(action.id, t); return <button key={action.id} type="button" className="screenshot-action" data-screenshot-control disabled={Boolean(busyAction) || !actionIsEnabled(action.id, bootstrap)} onClick={() => void completeScreenshot(action.id)} title={t('screenshot.shortcutHint', { label, shortcut: [action.shortcut, hotkeyForAction(action.id)].filter(Boolean).join(' / ') })}>{busyAction === action.id ? t('screenshot.processing') : label}</button>; })}{!bootstrap.screenshotAiConfigured && <button type="button" className="screenshot-action" data-screenshot-control disabled={Boolean(busyAction)} onClick={() => void openAiSettings()}>{t('screenshot.actions.configureAi')}</button>}</div>}
      {bootstrap.screenshotMagnifierEnabled && bootstrap.magnifierBackground && magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && (
        <>
          <canvas className="screenshot-magnifier" data-screenshot-magnifier="true" style={magnifierLayout} ref={magnifierCanvasRef} />
          {magnifierColor && <div className="screenshot-color" aria-hidden="true" data-screenshot-color="true" style={colorPanelStyle(magnifierLayout, bootstrap.bounds)}>{formatRgb(magnifierColor)}</div>}
        </>
      )}
      {magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && (
        <div className="screenshot-coordinates" aria-hidden="true" data-screenshot-coordinates="true" style={coordinatePanelStyle(magnifierPoint, bootstrap.bounds)}>{formatCursorCoordinate(magnifierPoint)}</div>
      )}
      {magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && (
        <CrosshairGuides point={magnifierPoint} bounds={bootstrap.bounds} />
      )}
      {selection && <ThirdsGrid bounds={bootstrap.bounds} />}
      {selection && <Ruler bounds={bootstrap.bounds} />}
      {selection && <SelectionHandles selection={selection} />}
      {!selecting && !selection && !showHelp && <div className="screenshot-idle-hint" aria-live="polite" data-screenshot-idle="true">{idleHint(t)}</div>}
      {selection && !selecting && !moving && !resizing && !busyAction && !showHelp && <div className="screenshot-invite" data-screenshot-invite="true">{includeInvite(t)}</div>}
      {modeHint(modeForState({ selecting, moving, resizing }), t) && <div className="screenshot-mode-hint" aria-hidden="true" data-screenshot-mode="true">{modeHint(modeForState({ selecting, moving, resizing }), t)}</div>}
      {showHelp && (
        <div className="screenshot-help" data-screenshot-help="true" data-screenshot-control role="dialog" aria-label={t('screenshot.help.title')} onPointerDown={(event) => event.stopPropagation()}>
          <div className="screenshot-help-title">{t('screenshot.help.title')}</div>
          {helpEntries(t).map((entry) => (
            <div key={entry.id} className="screenshot-help-row">
              <span className="screenshot-help-keys">{entry.keys.join(' / ')}</span>
              <span className="screenshot-help-label">{entry.label}</span>
            </div>
          ))}
        </div>
      )}
      {actionError && <div className="screenshot-error" role="alert" data-screenshot-control>{actionError}</div>}
      <button type="button" className="screenshot-cancel" data-screenshot-control onPointerDown={(event) => event.stopPropagation()} onClick={() => void cancelScreenshot()} aria-label={t('screenshot.cancelLabel')} title={t('screenshot.shortcutHint', { label: t('screenshot.cancelLabel'), shortcut: 'Esc' })}>{t('screenshot.cancel')}</button>
    </main>
  );
}

export default App;
