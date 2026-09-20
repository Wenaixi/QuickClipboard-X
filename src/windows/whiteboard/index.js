// 白板前端：透明全屏画布，钢笔/直线/箭头/矩形/椭圆工具 + 颜色、
// 粗细、清空、保存。保存把 canvas 以 PNG（data URL base64）交后端
// save_img_png_base64 走截图历史链路（复制进剪贴板+历史入库）。
//
// 形状持久层：shapes 数组保存每一笔完成的形状（工具/颜色/粗细/锚点/终点/路径快照），
// renderPreview 每帧先清屏再全栈重绘，保证多笔画互不丢失。绘制逻辑集中在
// drawShape，预览帧与成稿共用同一套几何。

import { getCurrentWindow } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { showError } from '@shared/utils/dialog';

const TOOLS = ['pen', 'line', 'arrow', 'rect', 'ellipse'];
const COLORS = ['#1f2937', '#ef4444', '#3b82f6', '#22c55e'];

let tool = 'pen';
let color = COLORS[0];
let lineWidth = 3;
let drawing = false;
let anchor = null;
let end = null;
let path = [];

// 形状历史栈：每笔完成即入栈；重绘/清空/撤销都基于它，
// 保证画布任意时刻 = 全栈形状 + 当前进行中一笔。
const shapes = [];

const canvas = document.getElementById('board');
const ctx = canvas.getContext('2d');

function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = window.innerWidth * dpr;
  canvas.height = window.innerHeight * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  renderPreview();
}

// 基底：透明背景（白板区域本身是画布，桌面透出），只留极浅参考网格帮助定位。
function drawBase() {
  ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
}

function localPoint(event) {
  return { x: event.clientX, y: event.clientY };
}

// 形状绘制纯函数：按 shape 字段画出几何。预览帧与已入栈形状共用。
function drawShape(shape) {
  ctx.strokeStyle = shape.color;
  ctx.fillStyle = shape.color;
  ctx.lineWidth = shape.lineWidth;
  ctx.lineCap = 'round';
  if (shape.tool === 'pen') {
    if (shape.path.length > 1) {
      ctx.beginPath();
      ctx.moveTo(shape.path[0].x, shape.path[0].y);
      for (let i = 1; i < shape.path.length; i += 1) ctx.lineTo(shape.path[i].x, shape.path[i].y);
      ctx.stroke();
    }
    return;
  }
  if (!shape.anchor || !shape.end) return;
  ctx.beginPath();
  if (shape.tool === 'line') {
    ctx.moveTo(shape.anchor.x, shape.anchor.y);
    ctx.lineTo(shape.end.x, shape.end.y);
  } else if (shape.tool === 'arrow') {
    const dx = shape.end.x - shape.anchor.x;
    const dy = shape.end.y - shape.anchor.y;
    const len = Math.max(1, Math.hypot(dx, dy));
    const ang = Math.atan2(dy, dx);
    ctx.moveTo(shape.anchor.x, shape.anchor.y);
    ctx.lineTo(shape.end.x, shape.end.y);
    ctx.moveTo(shape.end.x, shape.end.y);
    ctx.lineTo(shape.end.x - 12 * Math.cos(ang - 0.4), shape.end.y - 12 * Math.sin(ang - 0.4));
    ctx.moveTo(shape.end.x, shape.end.y);
    ctx.lineTo(shape.end.x - 12 * Math.cos(ang + 0.4), shape.end.y - 12 * Math.sin(ang + 0.4));
  } else if (shape.tool === 'rect') {
    ctx.rect(shape.anchor.x, shape.anchor.y, shape.end.x - shape.anchor.x, shape.end.y - shape.anchor.y);
  } else if (shape.tool === 'ellipse') {
    ctx.ellipse(
      (shape.anchor.x + shape.end.x) / 2,
      (shape.anchor.y + shape.end.y) / 2,
      Math.abs(shape.end.x - shape.anchor.x) / 2,
      Math.abs(shape.end.y - shape.anchor.y) / 2,
      0, 0, Math.PI * 2,
    );
  }
  ctx.stroke();
}

// 全量重绘：清屏 → 遍历形状栈逐笔画出 → 叠加当前进行中一笔。
function renderPreview() {
  drawBase();
  for (const shape of shapes) drawShape(shape);
  if (!drawing) return;
  if (tool === 'pen') {
    drawShape({ tool, color, lineWidth, anchor, end: null, path });
  } else if (anchor && end) {
    drawShape({ tool, color, lineWidth, anchor, end, path: [] });
  }
}

function setupToolButtons() {
  TOOLS.forEach((id) => {
    const btn = document.getElementById(`tool-${id}`);
    if (btn) {
      btn.addEventListener('click', () => {
        tool = id;
        anchor = null;
        end = null;
        document.querySelectorAll('.tool').forEach((b) => b.classList.toggle('active'));
        btn.classList.add('active');
      });
    }
  });
  COLORS.forEach((c, i) => {
    const btn = document.getElementById(`color-${i}`);
    if (btn) {
      btn.style.background = c;
      btn.addEventListener('click', () => {
        color = c;
        document.querySelectorAll('.color').forEach((b) => b.classList.toggle('active'));
        btn.classList.add('active');
      });
    }
  });
}

function startDraw(event) {
  event.preventDefault();
  // 指针捕获:不捕获时绘制中指针滑到工具栏(fixed 悬浮层)释放,pointerup
  // 目标变工具栏按钮,endDraw 不触发,drawing 永久卡死(此后无法再画,
  // 清空/撤销都不重置 drawing)。捕获后释放事件仍落在 canvas。
  if (event.pointerId !== undefined) {
    try {
      canvas.setPointerCapture(event.pointerId);
    } catch {
      // 指针已抬起等情形捕获失败,不阻塞绘制
    }
  }
  if (!drawing) {
    drawing = true;
    anchor = localPoint(event);
    end = anchor === null ? null : { ...anchor };
    path = [anchor];
  }
}

function moveDraw(event) {
  if (!drawing) return;
  if (tool === 'pen') path.push(localPoint(event));
  else end = localPoint(event);
  renderPreview();
}

function endDraw(event) {
  if (!drawing) return;
  if (event && event.pointerId !== undefined) {
    try {
      canvas.releasePointerCapture(event.pointerId);
    } catch {
      // 未捕获时释放无意义,忽略
    }
  }
  // 完成一笔：快照入栈后再清进行中状态，全栈重绘让成稿常驻。
  if (tool === 'pen') {
    if (path.length > 1) {
      shapes.push({ tool, color, lineWidth, anchor, end: null, path: [...path] });
    }
  } else if (anchor && end) {
    shapes.push({ tool, color, lineWidth, anchor, end: { ...end }, path: [] });
  }
  drawing = false;
  anchor = null;
  end = null;
  path = [];
  renderPreview();
}

// 绘制中断兜底(指针移出窗口/系统打断/多点触控切换):与 endDraw 同一
// 收口语义——完成当前一笔或丢弃空笔,重置 drawing 防永久卡死。
function cancelDraw() {
  endDraw();
}

async function save() {
  // 白板画布透明 → 合成白底再转 PNG（PNG 不支持透明画布预览，白底保证
  // 保存产物可见）。canvas 此刻已是全栈形状重绘结果，保存即所见。
  const dpr = window.devicePixelRatio || 1;
  const out = document.createElement('canvas');
  out.width = window.innerWidth * dpr;
  out.height = window.innerHeight * dpr;
  const octx = out.getContext('2d');
  octx.fillStyle = '#ffffff';
  octx.fillRect(0, 0, out.width, out.height);
  octx.drawImage(canvas, 0, 0);
  const base64 = out.toDataURL('image/png').split(',')[1];
  try {
    await invoke('save_img_png_base64', { pngBase64: base64 });
  } catch (error) {
    // 保存失败必须提示并保持窗口打开——无条件关窗让用户误以为保存成功,
    // 白板绘制成果静默丢失。
    console.error('保存白板失败:', error);
    await showError('保存失败,请重试:' + String(error?.message || error));
    return;
  }
  getCurrentWindow().close().catch(() => {});
}

// 撤销最后一笔：弹栈后全栈重绘（保留工具/颜色/粗细设置，仅回退一笔）。
function undo() {
  if (!shapes.length) return;
  shapes.pop();
  renderPreview();
}

function start() {
  const window = getCurrentWindow();
  resizeCanvas();
  window.onResized(() => resizeCanvas());
  setupToolButtons();
  document.getElementById('clear').addEventListener('click', () => {
    shapes.length = 0;
    renderPreview();
  });
  document.getElementById('save').addEventListener('click', save);
  const undoBtn = document.getElementById('undo');
  if (undoBtn) undoBtn.addEventListener('click', undo);
  canvas.addEventListener('pointerdown', startDraw);
  canvas.addEventListener('pointermove', moveDraw);
  canvas.addEventListener('pointerup', endDraw);
  // 绘制中断兜底:指针移出窗口/系统打断都会触发 pointercancel/pointerleave,
  // 必须走 endDraw 收口,否则 drawing 残留卡死(见 startDraw 注释)。
  canvas.addEventListener('pointercancel', cancelDraw);
  canvas.addEventListener('pointerleave', cancelDraw);
  document.addEventListener('pointerup', endDraw);
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') window.close().catch(() => {});
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'z') {
      event.preventDefault();
      undo();
    }
  });
  // 默认钢笔激活。
  document.getElementById('tool-pen')?.classList.add('active');
  document.getElementById('color-0')?.classList.add('active');
}

window.addEventListener('DOMContentLoaded', start);