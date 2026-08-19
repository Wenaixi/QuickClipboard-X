import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { normalizeBootstrap } from './screenshotModel.js';
import { magnifierGeometry } from './magnifierModel.js';
import { coordinatePanelPosition, formatCursorCoordinate } from './coordinateModel.js';
import { guideLines } from './guideModel.js';
import { actionForHotkey, hotkeyForAction } from './actionModel.js';
import { selectionLabelPlacement } from './labelModel.js';
import { cursorForSelectionHover } from './cursorModel.js';
import { lineStyle } from './annotationModel.js';
import { readCenterPixel, formatRgb, hexFromRgb } from './colorModel.js';
import { magnifierScaleForWheel } from './magnifierZoomModel.js';
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

function CrosshairGuides({ point, bounds }) {
  const lines = guideLines(point, bounds);
  return (
    <>
      <div className="screenshot-guide screenshot-guide-v" data-screenshot-guide="true" style={{ left: `${lines.vertical.left}px`, top: `${lines.vertical.top}px`, width: `${lines.vertical.width}px`, height: `${lines.vertical.height}px` }} />
      <div className="screenshot-guide screenshot-guide-h" data-screenshot-guide="true" style={{ left: `${lines.horizontal.left}px`, top: `${lines.horizontal.top}px`, width: `${lines.horizontal.width}px`, height: `${lines.horizontal.height}px` }} />
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
  const gestureIdRef = useRef(0);
  const [bootstrap, setBootstrap] = useState(() => normalizeBootstrap(globalThis.__QC_SCREENSHOT_BOOT__ || {}, {
    width: globalThis.innerWidth,
    height: globalThis.innerHeight,
    devicePixelRatio: globalThis.devicePixelRatio,
  }));
  const [selection, setSelection] = useState(null);
  const [selecting, setSelecting] = useState(false);
  const [magnifierPoint, setMagnifierPoint] = useState(null);
  const [magnifierColor, setMagnifierColor] = useState(null);
  const [magnifierScale, setMagnifierScale] = useState(6);
  const [busyAction, setBusyAction] = useState('');
  const [actionError, setActionError] = useState('');
  const initialActionRef = useRef('');

  const toolbarStyle = useMemo(() => {
    if (!selection) return undefined;
    const toolbarWidth = 300;
    const toolbarHeight = 48;
    const left = clamp(selection.left, 8, Math.max(8, bootstrap.bounds.width - toolbarWidth - 8));
    const belowTop = selection.bottom + 12;
    const top = belowTop + toolbarHeight <= bootstrap.bounds.height - 8
      ? belowTop
      : Math.max(8, selection.top - toolbarHeight - 12);
    return { left: `${left}px`, top: `${top}px` };
  }, [bootstrap.bounds.height, bootstrap.bounds.width, selection]);

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
      return;
    }
    const image = new Image();
    image.onload = () => {
      const geometry = magnifierGeometry(magnifierPoint, bootstrap.bounds, { scale: magnifierScale });
      canvas.width = geometry.panel.width;
      canvas.height = geometry.panel.height;
      const context = canvas.getContext('2d');
      if (!context) return;
      context.imageSmoothingEnabled = false;
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(
        image,
        geometry.source.left,
        geometry.source.top,
        geometry.source.cols,
        geometry.source.rows,
        0,
        0,
        canvas.width,
        canvas.height
      );
      const pixel = readCenterPixel(context.getImageData(0, 0, canvas.width, canvas.height).data, canvas.width, canvas.height);
      setMagnifierColor(pixel);
    };
    image.src = bootstrap.magnifierBackground;
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
      draftRef.current = null;
      selectionRef.current = null;
      setSelection(null);
      setSelecting(false);
      setActionError('');
      applySelectionStyle(rootRef.current, null);
      if (rootRef.current) rootRef.current.style.cursor = 'crosshair';
    });
    configurePromise.then((cleanup) => {
      if (!active) {
        cleanup();
        return;
      }
      unlisten = cleanup;
      invoke(VIEWPORT_READY_COMMAND).catch(() => {});
    });
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
    draftRef.current = null;
    selectionRef.current = null;
    setMagnifierPoint(null);
    rafWriterRef.current?.cancel();
    applySelectionStyle(rootRef.current, null);
    if (rootRef.current) rootRef.current.style.cursor = 'crosshair';
    setSelection(null);
    setSelecting(false);
    setActionError('');
    if (sessionId) {
      try { await invoke(CANCEL_COMMAND, { sessionId }); }
      catch (error) { setActionError(t('screenshot.cancelFailed', { error: String(error) })); return; }
    }
    try { await getCurrentWindow().close(); } catch {}
  };

  const completeScreenshot = async (action) => {
    const currentSelection = selectionRef.current;
    if (!currentSelection || busyAction || !actionIsEnabled(action, bootstrap)) return;
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
        resizeRef.current = { pointerId: event.pointerId, edge };
        root.setPointerCapture?.(event.pointerId);
        return;
      }
      if (hitSelectionInterior(start, selectionRef.current, MOVE_INSET)) {
        moveRef.current = { pointerId: event.pointerId, start, selectionStart: selectionRef.current };
        root.setPointerCapture?.(event.pointerId);
        return;
      }
    }
    gestureIdRef.current += 1;
    draftRef.current = { start, end: start };
    pointerIdRef.current = event.pointerId;
    root.setPointerCapture?.(event.pointerId);
    setSelecting(true);
    setSelection(null);
    setActionError('');
    root.style.cursor = 'crosshair';
    applySelectionStyle(root, normalizeSelection(start, start, bootstrap.bounds));
  };

  const handlePointerMove = (event) => {
    const root = rootRef.current;
    const resizing = resizeRef.current;
    if (resizing && root && event.pointerId === resizing.pointerId) {
      const current = pointFromPointerEvent(event, root);
      setMagnifierPoint(current);
      const next = resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey });
      selectionRef.current = next;
      rafWriterRef.current?.schedule(next);
      return;
    }
    const moving = moveRef.current;
    if (moving && root && event.pointerId === moving.pointerId) {
      const current = pointFromPointerEvent(event, root);
      setMagnifierPoint(current);
      const next = nudgeSelection(moving.selectionStart, current.x - moving.start.x, current.y - moving.start.y, bootstrap.bounds);
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
    const next = event.shiftKey
      ? squareSelection(draft.start, draft.end, bootstrap.bounds)
      : normalizeSelection(draft.start, draft.end, bootstrap.bounds);
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
      const finalSelection = resizeSelection(selectionRef.current, resizing.edge, current, bootstrap.bounds, { keepAspectRatio: event.shiftKey });
      resizeRef.current = null;
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
      const finalSelection = nudgeSelection(moving.selectionStart, current.x - moving.start.x, current.y - moving.start.y, bootstrap.bounds);
      moveRef.current = null;
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

    if (!isCurrentGesture(gestureId, gestureIdRef.current)) return;
    applySelectionStyle(root, finalSelection);
    selectionRef.current = finalSelection;
    setSelection(finalSelection);
    setSelecting(false);
    if (initialActionRef.current) {
      const action = initialActionRef.current;
      initialActionRef.current = '';
      void completeScreenshot(action);
    }
  };

  useEffect(() => {
    const handleKeyDown = (event) => {
      if (event.key === 'Escape') { event.preventDefault(); void cancelScreenshot(); return; }
      if (event.target instanceof Element && event.target.closest('[data-screenshot-control]')) return;
      if (!selectionRef.current || busyAction) return;
      const [nudgeX, nudgeY] = NUDGE_DIRECTIONS[event.key] || [];
      if (nudgeX !== undefined) {
        event.preventDefault();
        const step = event.ctrlKey ? NUDGE_FAST_STEP : NUDGE_STEP;
        const next = nudgeSelection(selectionRef.current, nudgeX * step, nudgeY * step, bootstrap.bounds);
        selectionRef.current = next;
        setSelection(next);
        applySelectionStyle(rootRef.current, next);
        return;
      }
      const hotkeyAction = actionForHotkey(event.key);
      if (hotkeyAction) { event.preventDefault(); void completeScreenshot(hotkeyAction); return; }
      if (event.key === 'Enter') { event.preventDefault(); void completeScreenshot('copy'); }
      else if (event.ctrlKey && event.key.toLowerCase() === 's') { event.preventDefault(); void completeScreenshot('save'); }
      else if (event.ctrlKey && event.key.toLowerCase() === 'p') { event.preventDefault(); void completeScreenshot('pin'); }
    };
    const handleBlur = () => { if (draftRef.current || selectionRef.current) void cancelScreenshot(); };
    window.addEventListener('keydown', handleKeyDown);
    window.addEventListener('blur', handleBlur);
    return () => {
      window.removeEventListener('keydown', handleKeyDown);
      window.removeEventListener('blur', handleBlur);
    };
  }, [bootstrap.sessionId, busyAction]);

  return (
    <main ref={rootRef} className="screenshot-root" data-selection-active="false" data-selecting={selecting ? 'true' : 'false'} onPointerDown={handlePointerDown} onPointerMove={handlePointerMove} onPointerUp={handlePointerUp} onPointerCancel={cancelScreenshot} onDoubleClick={handleDoubleClick} onWheel={handleWheel} onContextMenu={(event) => { event.preventDefault(); void cancelScreenshot(); }} aria-label={t('screenshot.selectionLabel')}>
      <div className="screenshot-mask screenshot-mask-top" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-left" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-right" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-bottom" aria-hidden="true" />
      <div className="screenshot-selection screenshot-selection-line" aria-hidden="true" style={selectionLineStyle()}>{selection && <span className={selectionSizeLabelClass(selection, bootstrap.bounds)} style={selectionSizeLabelStyle(selection, bootstrap.bounds)}>{Math.round(selection.width)} × {Math.round(selection.height)}</span>}</div>
      {selection && <div className="screenshot-toolbar" style={toolbarStyle} data-screenshot-control onPointerDown={(event) => event.stopPropagation()}>{ACTIONS.map((action) => { const label = actionLabel(action.id, t); return <button key={action.id} type="button" className="screenshot-action" data-screenshot-control disabled={Boolean(busyAction) || !actionIsEnabled(action.id, bootstrap)} onClick={() => void completeScreenshot(action.id)} title={[action.shortcut && t('screenshot.shortcutHint', { label, shortcut: action.shortcut }), hotkeyForAction(action.id) && t('screenshot.shortcutHint', { label, shortcut: hotkeyForAction(action.id) })].filter(Boolean).join(' · ') || label}>{busyAction === action.id ? t('screenshot.processing') : label}</button>; })}{!bootstrap.screenshotAiConfigured && <button type="button" className="screenshot-action" data-screenshot-control disabled={Boolean(busyAction)} onClick={() => void openAiSettings()}>{t('screenshot.actions.configureAi')}</button>}</div>}
      {bootstrap.screenshotMagnifierEnabled && bootstrap.magnifierBackground && magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && (
        <>
          <canvas className="screenshot-magnifier" data-screenshot-magnifier="true" style={magnifierCanvasStyle(magnifierPoint, bootstrap.bounds, magnifierScale)} ref={magnifierCanvasRef} />
          {magnifierColor && <div className="screenshot-color" data-screenshot-color="true" style={{ left: `${magnifierCanvasStyle(magnifierPoint, bootstrap.bounds, magnifierScale).left}`, top: `${magnifierCanvasStyle(magnifierPoint, bootstrap.bounds, magnifierScale).top}`, backgroundColor: hexFromRgb(magnifierColor) }}>{formatRgb(magnifierColor)}</div>}
        </>
      )}
      {magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && (
        <div className="screenshot-coordinates" data-screenshot-coordinates="true" style={coordinatePanelStyle(magnifierPoint, bootstrap.bounds)}>{formatCursorCoordinate(magnifierPoint)}</div>
      )}
      {magnifierPoint && (draftRef.current || moveRef.current || resizeRef.current) && (
        <CrosshairGuides point={magnifierPoint} bounds={bootstrap.bounds} />
      )}
      {actionError && <div className="screenshot-error" role="alert" data-screenshot-control>{actionError}</div>}
      <button type="button" className="screenshot-cancel" data-screenshot-control onPointerDown={(event) => event.stopPropagation()} onClick={() => void cancelScreenshot()} aria-label={t('screenshot.cancelLabel')} title={t('screenshot.shortcutHint', { label: t('screenshot.cancelLabel'), shortcut: 'Esc' })}>{t('screenshot.cancel')}</button>
    </main>
  );
}

export default App;
