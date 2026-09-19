// R6 工具集前端 API 护栏：六个工具命令名必须与后端 invoke 注册的
// 命令名完全一致（前端命令面漂移会让工具入口静默失败，工程不允许
// 前后端命令面出现 segundo 副本）。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const toolsSource = readFileSync(new URL('./tools.js', import.meta.url), 'utf8');
const apiIndex = readFileSync(new URL('./index.js', import.meta.url), 'utf8');

test('工具 API 六个命令名必须与后端注册一致', () => {
  // 取色器/二维码/哈希/标尺/白板/通用画布保存六命令，
  // 前端 invoke 名必须逐字等于后端 tauri 命令名。
  const expectedInvokes = [
    "'open_color_picker'",
    "'save_qr_png_base64'",
    "'hash_file_sha256'",
    "'open_ruler'",
    "'open_whiteboard'",
    "'save_img_png_base64'",
  ];
  for (const command of expectedInvokes) {
    assert.ok(
      toolsSource.includes(`invoke(${command}`) || toolsSource.includes(`invoke(${command},`),
      `工具 API 缺 invoke(${command}`
    );
  }
});

test('工具箱模块导出必须含六个工具函数', () => {
  // index.js 转发 tools 模块，缺导出会让 TitleBar 动态 import 失败。
  assert.ok(apiIndex.includes("export * from './tools'"), 'api/index.js 必须转发 tools');
  const expectedExports = [
    'openColorPicker',
    'saveQrPngBase64',
    'hashFileSha256',
    'openRuler',
    'openWhiteboard',
    'saveImgPngBase64',
  ];
  for (const fn of expectedExports) {
    assert.ok(toolsSource.includes(`export async function ${fn}`), `缺导出 ${fn}`);
  }
});