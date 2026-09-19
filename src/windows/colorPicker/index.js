// R6 取色器前端：透明全屏吸管窗（对齐 ShareX ScreenColorPickerWindow）。
// 后端在窗口启动后持续 push_color_to_window 推鼠标位置颜色；前端显示
// 放大镜（按颜色绘制）与 Hex/RGB 色值，点击复制 Hex（后端 copy 并关窗），
// Esc 关闭。跨屏 DPI 校正：坐标用物理像素（dpr 归一）避免高分屏错位。

import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

const COLOR_PICKER_UPDATE_EVENT = 'color-picker:update';

// 放大镜：把屏幕坐标颜色绘制成 7×7 色块网格 + 中心像素放大显示。
function drawMagnifier(canvas, { x, y, color }, dpr) {
  const ctx = canvas.getContext('2d');
  const size = 140 * dpr;
  canvas.width = size;
  canvas.height = size;
  ctx.clearRect(0, 0, size, size);
  if (!color) return;
  const cell = size / 7;
  for (let row = 0; row < 7; row += 1) {
    for (let col = 0; col < 7; col += 1) {
      const isCenter = row === 3 && col === 3;
      ctx.fillStyle = `#${color}`;
      ctx.fillRect(col * cell, row * cell, cell, cell);
      if (isCenter) {
        ctx.strokeStyle = '#ffffff';
        ctx.lineWidth = 2 * dpr;
        ctx.strokeRect(col * cell, row * cell, cell, cell);
      }
    }
  }
  // 中心十字准星。
  ctx.strokeStyle = 'rgba(255,255,255,0.9)';
  ctx.lineWidth = 1.5 * dpr;
  ctx.beginPath();
  ctx.moveTo(size / 2, 0);
  ctx.lineTo(size / 2, size);
  ctx.moveTo(0, size / 2);
  ctx.lineTo(size, size / 2);
  ctx.stroke();
}

function start() {
  const window = getCurrentWindow();
  const canvas = document.getElementById('magnifier');
  const hexLabel = document.getElementById('hex-value');
  const rgbLabel = document.getElementById('rgb-value');
  const dpr = window.devicePixelRatio || 1;

  let currentColor = null;
  listen(COLOR_PICKER_UPDATE_EVENT, (event) => {
    const { color } = event.payload || {};
    currentColor = color || null;
    drawMagnifier(canvas, event.payload || {}, dpr);
    if (hexLabel) hexLabel.textContent = currentColor ? `#${currentColor}` : '—';
    if (rgbLabel) {
      rgbLabel.textContent = currentColor
        ? `RGB(${parseInt(currentColor.slice(0, 2), 16)}, ${parseInt(currentColor.slice(2, 4), 16)}, ${parseInt(currentColor.slice(4, 6), 16)})`
        : '';
    }
  });

  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      window.close().catch(() => {});
    }
  });

  document.addEventListener('pointerdown', () => {
    if (currentColor) {
      // 后端按物理坐标确认取色并复制 Hex + 关窗。
      invoke('color_picker_pick_at').catch(() => {});
    }
  });

  // 窗口就绪：后端开始持续推送颜色。
  invoke('color_picker_ready').catch(() => {});
}

window.addEventListener('DOMContentLoaded', start);
