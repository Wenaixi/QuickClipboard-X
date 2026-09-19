// R6 屏幕标尺前端：透明可调小窗，画横纵刻度 + 实时显示宽高像素。
// 跟随窗口尺寸变化重绘刻度；拖窗边缘即测量对应屏幕范围（对齐
// ShareX ScreenRuler 的拖拽标尺语义）。Esc 关闭。

import { getCurrentWindow } from '@tauri-apps/api/window';

const TICK_MAJOR = 10; // 主刻度像素
const TICK_MINOR = 5; // 次刻度像素

function drawRuler() {
  const canvas = document.getElementById('ruler-canvas');
  const sizeLabel = document.getElementById('size-value');
  const dpr = window.devicePixelRatio || 1;
  const width = window.innerWidth;
  const height = window.innerHeight;
  canvas.width = width * dpr;
  canvas.height = height * dpr;
  const ctx = canvas.getContext('2d');
  ctx.scale(dpr, dpr);
  ctx.clearRect(0, 0, width, height);

  // 背景：半透明白，便于看被测量区域。
  ctx.fillStyle = 'rgba(255, 255, 255, 0.92)';
  ctx.fillRect(0, 0, width, height);

  // 外边框。
  ctx.strokeStyle = '#3b82f6';
  ctx.lineWidth = 2;
  ctx.strokeRect(1, 1, width - 2, height - 2);

  // 横刻度（上缘）。
  ctx.strokeStyle = '#1f2937';
  ctx.fillStyle = '#1f2937';
  ctx.font = '10px sans-serif';
  ctx.lineWidth = 1;
  for (let x = 0; x <= width; x += 1) {
    if (x % TICK_MAJOR === 0) {
      ctx.beginPath();
      ctx.moveTo(x + 0.5, 0);
      ctx.lineTo(x + 0.5, 14);
      ctx.stroke();
      ctx.fillText(String(x), x + 2, 22);
    } else if (x % TICK_MINOR === 0) {
      ctx.beginPath();
      ctx.moveTo(x + 0.5, 0);
      ctx.lineTo(x + 0.5, 8);
      ctx.stroke();
    }
  }

  // 纵刻度（左缘）。
  for (let y = 0; y <= height; y += 1) {
    if (y % TICK_MAJOR === 0) {
      ctx.beginPath();
      ctx.moveTo(0, y + 0.5);
      ctx.lineTo(14, y + 0.5);
      ctx.stroke();
    } else if (y % TICK_MINOR === 0) {
      ctx.beginPath();
      ctx.moveTo(0, y + 0.5);
      ctx.lineTo(8, y + 0.5);
      ctx.stroke();
    }
  }

  if (sizeLabel) {
    sizeLabel.textContent = `${Math.round(width)} × ${Math.round(height)}`;
  }
}

function start() {
  const window = getCurrentWindow();
  drawRuler();
  window.onResized(() => drawRuler());
  document.addEventListener('keydown', (event) => {
    if (event.key === 'Escape') {
      window.close().catch(() => {});
    }
  });
}

window.addEventListener('DOMContentLoaded', start);
