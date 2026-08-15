import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { normalizeBootstrap } from './screenshotModel.js';
import {
  createRafWriter,
  normalizeSelection,
  selectionToPhysical,
} from './selectionModel.js';

const CONFIGURE_EVENT = 'screenshot:configure';
const VIEWPORT_READY_COMMAND = 'screenshot_window_ready';
const COMPLETE_COMMAND = 'complete_screenshot';
const CANCEL_COMMAND = 'cancel_screenshot';

const ACTIONS = [
  { id: 'copy', shortcut: 'Enter' },
  { id: 'save', shortcut: 'Ctrl+S' },
  { id: 'pin', shortcut: 'Ctrl+P' },
  { id: 'ai', shortcut: '' },
];

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
  const rafWriterRef = useRef(null);
  const draftRef = useRef(null);
  const selectionRef = useRef(null);
  const pointerIdRef = useRef(null);
  const [bootstrap, setBootstrap] = useState(() => normalizeBootstrap(globalThis.__QC_SCREENSHOT_BOOT__ || {}, {
    width: globalThis.innerWidth,
    height: globalThis.innerHeight,
    devicePixelRatio: globalThis.devicePixelRatio,
  }));
  const [selection, setSelection] = useState(null);
  const [selecting, setSelecting] = useState(false);
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
      draftRef.current = null;
      selectionRef.current = null;
      setSelection(null);
      setSelecting(false);
      setActionError('');
      applySelectionStyle(rootRef.current, null);
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
    draftRef.current = null;
    selectionRef.current = null;
    rafWriterRef.current?.cancel();
    applySelectionStyle(rootRef.current, null);
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
    draftRef.current = { start, end: start };
    pointerIdRef.current = event.pointerId;
    root.setPointerCapture?.(event.pointerId);
    setSelecting(true);
    setSelection(null);
    setActionError('');
    applySelectionStyle(root, normalizeSelection(start, start, bootstrap.bounds));
  };

  const handlePointerMove = (event) => {
    const draft = draftRef.current;
    const root = rootRef.current;
    if (!draft || !root || event.pointerId !== pointerIdRef.current) return;
    draft.end = pointFromPointerEvent(event, root);
    rafWriterRef.current?.schedule(normalizeSelection(draft.start, draft.end, bootstrap.bounds));
  };

  const handlePointerUp = (event) => {
    const draft = draftRef.current;
    const root = rootRef.current;
    if (!draft || !root || event.pointerId !== pointerIdRef.current) return;
    event.preventDefault();
    const finalSelection = normalizeSelection(draft.start, pointFromPointerEvent(event, root), bootstrap.bounds);
    draftRef.current = null;
    pointerIdRef.current = null;
    rafWriterRef.current?.cancel();
    applySelectionStyle(root, finalSelection);
    selectionRef.current = finalSelection;
    setSelection(finalSelection);
    setSelecting(false);
    if (initialActionRef.current) {
      const action = initialActionRef.current;
      initialActionRef.current = '';
      void completeScreenshot(action);
    }
    root.releasePointerCapture?.(event.pointerId);
  };

  useEffect(() => {
    const handleKeyDown = (event) => {
      if (event.key === 'Escape') { event.preventDefault(); void cancelScreenshot(); return; }
      if (!selectionRef.current || busyAction) return;
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
    <main ref={rootRef} className="screenshot-root" data-selection-active="false" data-selecting={selecting ? 'true' : 'false'} onPointerDown={handlePointerDown} onPointerMove={handlePointerMove} onPointerUp={handlePointerUp} onPointerCancel={cancelScreenshot} onContextMenu={(event) => { event.preventDefault(); void cancelScreenshot(); }} aria-label={t('screenshot.selectionLabel')}>
      <div className="screenshot-mask screenshot-mask-top" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-left" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-right" aria-hidden="true" />
      <div className="screenshot-mask screenshot-mask-bottom" aria-hidden="true" />
      <div className="screenshot-selection" aria-hidden="true"><span className="screenshot-selection-size">{selection ? `${Math.round(selection.width)} × ${Math.round(selection.height)}` : ''}</span></div>
      {selection && <div className="screenshot-toolbar" style={toolbarStyle} data-screenshot-control onPointerDown={(event) => event.stopPropagation()}>{ACTIONS.map((action) => { const label = actionLabel(action.id, t); return <button key={action.id} type="button" className="screenshot-action" data-screenshot-control disabled={Boolean(busyAction) || !actionIsEnabled(action.id, bootstrap)} onClick={() => void completeScreenshot(action.id)} title={action.shortcut ? t('screenshot.shortcutHint', { label, shortcut: action.shortcut }) : label}>{busyAction === action.id ? t('screenshot.processing') : label}</button>; })}{!bootstrap.screenshotAiConfigured && <button type="button" className="screenshot-action" data-screenshot-control disabled={Boolean(busyAction)} onClick={() => void openAiSettings()}>{t('screenshot.actions.configureAi')}</button>}</div>}
      {actionError && <div className="screenshot-error" role="alert" data-screenshot-control>{actionError}</div>}
      <button type="button" className="screenshot-cancel" data-screenshot-control onPointerDown={(event) => event.stopPropagation()} onClick={() => void cancelScreenshot()} aria-label={t('screenshot.cancelLabel')} title={t('screenshot.shortcutHint', { label: t('screenshot.cancelLabel'), shortcut: 'Esc' })}>{t('screenshot.cancel')}</button>
    </main>
  );
}

export default App;
