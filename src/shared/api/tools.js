// R6 工具集前端 API：取色器吸管入口（后端 open_color_picker 创建
// 全屏取色窗口）。工具入口统一收口本文件，后续哈希/标尺/白板在此扩展。

import { invoke } from '@tauri-apps/api/core';

export async function openColorPicker() {
  return await invoke('open_color_picker');
}
