// 局域网同步手动拉取入口护栏:push/pull 必须成对存在。
// 历史教训:pull 服务层实现完整但命令/前端入口三面全缺,用户在对端离线
// 期间无法主动拉回数据(自动同步是双向开关,手动侧却只有 push 单向)。
// 护栏锁定三点:命令成对定义、lib.rs 成对注册、前端按钮成对调用。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

// 去掉行注释,防止注释字面误命中被测代码模式。
function bare(path) {
  return readFileSync(path, 'utf8')
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');
}

// 定位仓库根:test 文件位于 src/shared/api/ 下,向上三级到 src/shared/api →
// src/shared → src → 仓库根。Windows 上 URL.pathname 带前导斜杠,剥掉避免
// 拼出 D:\D:\... 前缀。
const root = decodeURIComponent(new URL('../../..', import.meta.url).pathname).replace(/^\//, '').replace(/\/$/, '');

test('syncTransfer API 必须成对提供 push 与 pull', () => {
  const api = bare(`${root}/src/shared/api/syncTransfer.js`);
  assert.ok(api.includes("invoke('sync_transfer_lan_push_to_peer'"), '缺 push invoke');
  assert.ok(api.includes("invoke('sync_transfer_lan_pull_to_peer'"), '缺 pull invoke（pull 命令未接通）');
});

test('设置面板必须成对提供推送与拉取按钮', () => {
  const section = bare(`${root}/src/windows/settings/sections/SyncTransferSection.jsx`);
  assert.ok(section.includes('onPushPeer'), '缺 push 按钮处理器');
  assert.ok(section.includes('onPullPeer'), '缺 pull 按钮处理器（手动拉取入口缺失）');
  assert.ok(section.includes('pullSyncTransferLanPeer'), '缺 pull API 调用');
  assert.ok(section.includes("t('settings.syncTransfer.pullPeer')"), '缺 pull 按钮文案');
});

test('后端命令必须成对注册(pull/push 均在 invoke_handler)', () => {
  const lib = bare(`${root}/src-tauri/src/lib.rs`);
  assert.ok(lib.includes('commands::sync_transfer_lan_push_to_peer'), '缺 push 注册');
  assert.ok(lib.includes('commands::sync_transfer_lan_pull_to_peer'), '缺 pull 注册（命令未挂进 handler）');
});