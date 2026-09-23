// image_id 白名单护栏:ImageContent 把非 data/非 files 来源的 image_id
// 直接拼 `${dataDir}/clipboard_images/{id}.png` 调 convertFileSrc,恶意 `..`/
// 绝对路径可越出图片目录读写 appdata 内任意文件,拼路径前必须白名单校验
// (对齐 HtmlContent/HtmlPreview/preview 三处同款护栏,本组件是主窗口纯
// 图片项渲染主路径,原为第四处缺口)。
// 与后端 is_valid_image_id 语义一致(/^[A-Za-z0-9_-]{1,128}$/)。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./ImageContent.jsx', import.meta.url), 'utf8');
// 剥行注释,避免注释字面误命中(与仓库守卫测试同构)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');
const code = strip(source);

test('ImageContent 必须定义 image_id 白名单正则', () => {
  assert.match(
    code,
    /IMAGE_ID_WHITELIST\s*=\s*\/\^\[A-Za-z0-9_-\]\{1,128\}\$\//,
    '必须声明与后端 is_valid_image_id 同语义的白名单正则',
  );
  assert.ok(
    code.includes('IMAGE_ID_WHITELIST.test(imageId)'),
    'image_id 分支拼路径前必须校验白名单',
  );
});

test('ImageContent 加载中文案必须走 common.loading', () => {
  assert.ok(
    code.includes("t('common.loading')"),
    '加载中必须走 common.loading 键',
  );
  assert.ok(
    !code.includes('>加载中...</span>'),
    '加载文案不得残留裸中文',
  );
});

test('ImageContent 拼 clipboard_images 路径前必须经过白名单校验', () => {
  const whitelistPos = code.indexOf('IMAGE_ID_WHITELIST.test(imageId)');
  const pathPos = code.indexOf('${normalizedDataDir}/clipboard_images/${imageId}.png');
  assert.ok(whitelistPos !== -1 && pathPos !== -1, '两处字面必须都存在');
  assert.ok(
    whitelistPos < pathPos,
    '白名单校验必须早于路径拼接,否则恶意 id 仍会被 convertFileSrc 发送',
  );
});