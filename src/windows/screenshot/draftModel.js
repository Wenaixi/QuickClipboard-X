// 拖拽草稿选区与交互状态重置（ShareX 公开行为参考：拖拽期间以草稿驱动选区，结束统一收口）。
// selectionFromDraft 把 start/end 草稿转换为规范化选区（Shift 走正方形），
// resetInteractionState 统一清空草稿/选区/移动/调整引用，避免多处手工重置漂移。

function assertDraft(draft) {
  if (!draft || typeof draft !== 'object') {
    throw new TypeError('草稿缺失');
  }
}

function assertBounds(bounds) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
}

export function selectionFromDraft(draft, event, bounds) {
  assertDraft(draft);
  assertBounds(bounds);
  const start = draft.start;
  const end = draft.end;
  if (!start || !end || !Number.isFinite(start.x) || !Number.isFinite(start.y) || !Number.isFinite(end.x) || !Number.isFinite(end.y)) {
    throw new TypeError('草稿坐标必须是有限数字');
  }
  return event.shiftKey
    ? squareSelection(start, end, bounds)
    : normalizeSelection(start, end, bounds);
}

export function resetInteractionState(state) {
  if (!state || typeof state !== 'object') {
    throw new TypeError('状态容器缺失');
  }
  state.draftRef.current = null;
  state.selectionRef.current = null;
  state.moveRef.current = null;
  state.resizeRef.current = null;
  state.setSelection?.(null);
  state.setSelecting?.(false);
  state.setMoving?.(false);
  state.setResizing?.(false);
}

// 复用 selectionModel 的方形与规范化逻辑，避免重复实现。
import { normalizeSelection, squareSelection } from './selectionModel.js';
