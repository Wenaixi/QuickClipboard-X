import { hitSelectionEdge, hitSelectionInterior } from './selectionModel.js';

const EDGE_CURSORS = {
  n: 'ns-resize',
  s: 'ns-resize',
  e: 'ew-resize',
  w: 'ew-resize',
  ne: 'nesw-resize',
  sw: 'nesw-resize',
  nw: 'nwse-resize',
  se: 'nwse-resize',
};

// 把选区边缘名映射为对应 CSS 光标（ShareX 公开行为：悬停到选区边缘/角点时切换调整光标）。
// 未知或空值回退到十字光标，保证调用方永远拿到合法 CSS 值。
export function cursorForEdge(edge) {
  return EDGE_CURSORS[edge] || 'crosshair';
}

// 计算鼠标悬停在选区上的光标：边缘/角点返回调整光标，内部返回移动光标，选区外或无边选区返回 null。
export function cursorForSelectionHover(point, selection, bounds) {
  if (!bounds || !Number.isFinite(bounds.width) || !Number.isFinite(bounds.height) || bounds.width <= 0 || bounds.height <= 0) {
    throw new RangeError('边界尺寸必须为正数');
  }
  if (!selection) return null;
  const edge = hitSelectionEdge(point, selection);
  if (edge) return cursorForEdge(edge);
  if (hitSelectionInterior(point, selection, 0)) return 'move';
  return null;
}
