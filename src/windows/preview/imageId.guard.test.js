// data-image-id 白名单护栏(预览窗口):HtmlPreview.jsx 的 resolveImageIdToAsset
// 与内联拼路径分支、preview/App.jsx 的 resolveImageUrlFromItem 都把 image-id
// 值拼 `${dataDir}/clipboard_images/{id}.png` 调 convertFileSrc,恶意 `..` 可越出
// 图片目录读写 appdata 内任意文件,拼路径前必须白名单校验(与后端 is_valid_image_id
// 同语义 /^[A-Za-z0-9_-]{1,128}$/)。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const htmlPreview = readFileSync(new URL('./views/HtmlPreview.jsx', import.meta.url), 'utf8');
const previewApp = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');
const previewCode = strip(htmlPreview);
const appCode = strip(previewApp);

test('HtmlPreview 必须定义 data-image-id 白名单正则并校验', () => {
  assert.match(
    previewCode,
    /IMAGE_ID_WHITELIST\s*=\s*\/\^\[A-Za-z0-9_-\]\{1,128\}\$\//,
    '必须声明与后端 is_valid_image_id 同语义的白名单正则',
  );
  assert.ok(
    previewCode.includes('IMAGE_ID_WHITELIST.test(imageId)'),
    'HtmlPreview 的 imageId 分支拼路径前必须校验白名单',
  );
  const whitelistPos = previewCode.indexOf('IMAGE_ID_WHITELIST.test(imageId)');
  const pathPos = previewCode.indexOf('${dataDir}/clipboard_images/${imageId}.png');
  assert.ok(whitelistPos !== -1 && pathPos !== -1, '两处字面必须都存在');
  assert.ok(
    whitelistPos < pathPos,
    'HtmlPreview 白名单校验必须早于路径拼接',
  );
});

test('preview/App.jsx 的 image-id 解析必须白名单校验', () => {
  const whitelistRe = /\/\^\[A-Za-z0-9_-\]\{1,128\}\$\/\.test\(/;
  const mainGuardPos = appCode.indexOf('clipboard_images/${imageId}.png');
  const mainValidPos = appCode.indexOf('/^[A-Za-z0-9_-]{1,128}$/.test(imageId)');
  assert.ok(mainValidPos !== -1 && mainValidPos < mainGuardPos, 'image_id 分支必须校验白名单且先于拼路径');
  const legacyValidPos = appCode.indexOf('/^[A-Za-z0-9_-]{1,128}$/.test(legacyImageId)');
  const legacyGuardPos = appCode.indexOf('clipboard_images/${legacyImageId}.png');
  assert.ok(legacyValidPos !== -1 && legacyValidPos < legacyGuardPos, 'image-id: 分支必须校验白名单且先于拼路径');
  assert.ok(whitelistRe.test(appCode), '校验必须用与后端同语义的白名单正则');
});