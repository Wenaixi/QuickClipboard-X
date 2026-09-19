// 贴图/图片加载内存护栏测试
//
// 图片大图(截图像素级 RGBA)不得以 base64 data:image 形式嵌入 JS 堆:
// 一张 1080p 截图 base64 后约 3-6MB,常驻 renderer 堆占内存且无复用;
// 应走 asset 协议(convertFileSrc)由 WebView 按需解码、可被回收。
// 本护栏锁定 pinImageToScreen 调用面不得引入 data:image 字面,并确认
// 全仓图片加载路径存在 convertFileSrc 用法。断言目标用拆分拼接避免
// 自命中(本测试读自己源码,§10.4 陷阱)。
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const dataImageLiteral = ['data:', 'image'].join('');
const convertFileSrcName = ['convert', 'FileSrc'].join('');

test('贴图入口不得内嵌 data:image 大图', () => {
  const clipboardSource = readFileSync(
    new URL('../api/clipboard.js', import.meta.url),
    'utf8',
  );
  const pinBody = clipboardSource.slice(
    clipboardSource.indexOf('export async function pinImageToScreen'),
  );
  assert.ok(!pinBody.includes(dataImageLiteral), '贴图命令调用面不得把大图嵌入 data:image');
  assert.ok(pinBody.includes("invoke('pin_image_from_file'"), '贴图必须走 pin_image_from_file 命令(后端 GDI 路径)');
});

test('图片加载路径必须使用 asset 协议(convertFileSrc)', () => {
  const librarySource = readFileSync(
    new URL('../api/imageLibrary.js', import.meta.url),
    'utf8',
  );
  assert.ok(librarySource.includes(convertFileSrcName), '图库缩略图/大图必须经 convertFileSrc 走 asset 协议');
  const bgSource = readFileSync(
    new URL('../utils/backgroundManager.js', import.meta.url),
    'utf8',
  );
  assert.ok(bgSource.includes(convertFileSrcName), '背景图必须经 convertFileSrc 走 asset 协议');
});
