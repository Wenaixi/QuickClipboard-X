// 来源图标哈希护栏：ClipboardItem 必须对 source_icon_hash 白名单校验后
// 才拼 app_icons 路径。历史上该字段从远端同步原样入库（本地捕获侧哈希
// 天然 16 位 hex，但 LAN/WebDAV 导入链路不校验），前端直接拼
// `${dataDir}/app_icons/${hash}.png` 引 asset 协议——与 image_id 的
// IMAGE_ID_WHITELIST（ImageContent/HtmlContent/HtmlPreview 三处均有）
// 不对称，是"别处都封了唯独这里漏"的纵深缺口。白名单对齐本地哈希值域
// [0-9a-f]{16}，误伤为零。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, './ClipboardItem.jsx'), 'utf8');
// 剥行注释，防止注释字面误命中被测代码模式
const bare = source
  .split('\n')
  .filter((line) => !line.trimStart().startsWith('//'))
  .join('\n');

test('ClipboardItem 必须声明 source_icon_hash 白名单(与 image_id 同风格)', () => {
  assert.match(
    bare,
    /ICON_HASH_WHITELIST\s*=\s*\/\^\[0-9a-f\]\{16\}\$\//,
    '必须声明 16 位 hex 白名单(对齐本地捕获侧哈希值域)',
  );
});

test('ClipboardItem 拼 app_icons 路径前必须先过白名单', () => {
  const whitelistPos = bare.indexOf('ICON_HASH_WHITELIST.test(item.source_icon_hash)');
  assert.ok(whitelistPos >= 0, '必须对 source_icon_hash 做白名单校验');
  const pathPos = bare.indexOf('app_icons');
  assert.ok(pathPos >= 0, '必须拼接 app_icons 路径');
  assert.ok(
    whitelistPos < pathPos,
    '白名单校验必须先于路径拼接(先校验后引用,与 image_id 同款)',
  );
});

test('ClipboardItem 白名单校验失败必须置图标加载失败(不静默跳过)', () => {
  const effectStart = bare.indexOf('useEffect(() => {');
  assert.ok(effectStart >= 0, '缺少 sourceIcon 加载 effect');
  const effectSeg = bare.slice(effectStart, effectStart + 800);
  assert.match(
    effectSeg,
    /setIconLoadFailed\(true\);/,
    '白名单校验失败必须走图标加载失败分支(前端可感知容错)',
  );
});
