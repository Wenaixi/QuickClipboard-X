// R6 白板前端：透明全屏画布，钢笔/直线/箭头/矩形/椭圆工具 + 颜色、
// 粗细、清空、保存。保存把 canvas 以 PNG（data URL base64）交后端
// save_img_png_base64 走截图历史链路（复制进剪贴板+历史入库）。

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
let path = [];

const canvas = document.getElementById('board');
const ctx = canvas.getContext('2d');

function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = window.innerWidth * dpr;
  canvas.height = window.innerHeight * dpr;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  drawBase();
}

// 基底：透明背景（白板区域本身是画布，桌面透出），只留极浅参考网格帮助定位。
function drawBase() {
  ctx.clearRect(0, 0, window.innerWidth, window.innerHeight);
}

function localPoint(event) {
  return { x: event.clientX, y: event.clientY };
}

function setupToolButtons() {
  TOOLS.forEach((id) => {
    const btn = document.getElementById(`tool-${id}`);
    if (btn) {
      btn.addEventListener('click', () => {
        tool = id;
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

function renderPreview(end) {
  drawBase();
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = lineWidth;
  ctx.lineCap = 'round';
  if (tool === 'pen') {
    if (path.length > 1) {
      ctx.beginPath();
      ctx.moveTo(path[0].x, path[0].y);
      for (let i = 1; i < path.length; i += 1) ctx.lineTo(path[i].x, path[i].y);
      ctx.stroke();
    }
    return;
  }
  if (!anchor || !end) return;
  ctx.beginPath();
  if (tool === 'line') {
    ctx.moveTo(anchor.x, anchor.y);
    ctx.lineTo(end.x, end.y);
  } else if (tool === 'arrow') {
    const dx = end.x - anchor.x;
    const dy = end.y - anchor.y;
    const len = Math.max(1, Math.hypot(dx, dy));
    const ang = Math.atan2(dy, dx);
    ctx.moveTo(anchor.x, anchor.y);
    ctx.lineTo(end.x, end.y);
    ctx.moveTo(end.x, end.y);
    ctx.lineTo(end.x - 12 * Math.cos(ang - 0.4), end.y - 12 * Math.sin(ang - 0.4));
    ctx.moveTo(end.x, end.y);
    ctx.lineTo(end.x - 12 * Math.cos(ang + 0.4), end.y - 12 * Math.sin(ang + 0.4));
  } else if (tool === 'rect') {
    ctx.rect(anchor.x, anchor.y, end.x - anchor.x, end.y - anchor.y);
  } else if (tool === 'ellipse') {
    ctx.ellipse(
      (anchor.x + end.x) / 2,
      (anchor.y + end.y) / 2,
      Math.abs(end.x - anchor.x) / 2,
      Math.abs(end.y - anchor.y) / 2,
      0, 0, Math.PI * 2,
    );
  }
  ctx.stroke();
}

function startDraw(event) {
  event.preventDefault();
  drawing = true;
  anchor = localPoint(event);
  path = [anchor];
}

function moveDraw(event) {
  if (!drawing) return;
  if (tool === 'pen') path.push(localPoint(event));
  renderPreview(tool === 'pen' ? null : localPoint(event));
}

function endDraw() {
  if (!drawing) return;
  drawing = false;
  renderPreview(null);
}

async function save() {
  // 白板画布透明 → 合成白底再转 PNG（PNG 不支持透明画布预览，白底保证
  // 保存产物可见）。
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

function start() {
  const window = getCurrentWindow();
  resizeCanvas();
  window.onResized(() => resizeCanvas());
  setupToolButtons();
  document.getElementById('clear').addEventListener('click', () => {
    ctx.clearRect(0, 0, canvas.width, canvas.height);
    drawBase();
  });
  document.getElementById('save').addEventListener('click', save);
  canvas.addEventListener('pointerdown', startDraw);
  canvas.addEventListener('pointermove', moveDraw);
  canvas.addEventListener('pointerup', endDraw);
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') window.close().catch(() => {});
  });
  // 默认钢笔激活。
  document.getElementById('tool-pen')?.classList.add('active');
  document.getElementById('color-0')?.classList.add('active');
}

window.addEventListener('DOMContentLoaded', start);