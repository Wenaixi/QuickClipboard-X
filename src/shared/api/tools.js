// R6 工具集前端 API：取色器吸管与二维码生成入口（后端 save_qr_png_base64
// 把前端 qrcode 生成的 PNG 存为剪贴板图片并落历史）。工具入口统一收口
// 本文件，后续哈希/标尺/白板在此扩展。

import { invoke } from '@tauri-apps/api/core';

export async function openColorPicker() {
  return await invoke('open_color_picker');
}

export async function saveQrPngBase64(pngBase64) {
  return await invoke('save_qr_png_base64', { pngBase64 });
}

export async function hashFileSha256(filePath) {
  return await invoke('hash_file_sha256', { filePath });
}

export async function openRuler() {
  return await invoke('open_ruler');
}

export async function openWhiteboard() {
  return await invoke('open_whiteboard');
}

export async function saveImgPngBase64(pngBase64) {
  return await invoke('save_img_png_base64', { pngBase64 });
}
