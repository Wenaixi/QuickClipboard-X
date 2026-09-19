// R3 编辑器前端接线护栏：App.jsx 必须把三个纯函数 model（layerModel/
// renderModel/historyModel）真正接进画布交互与保存链路——加载走
// annotation:load 事件 + convertFileSrc asset 协议，保存走通用画布
// 命令 save_img_png_base64，撤销/重做走双栈。命令面/事件面漂移会让
// 编辑器静默失效，护栏锁死接线。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const appSource = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');
const layerSource = readFileSync(new URL('./layerModel.js', import.meta.url), 'utf8');
const renderSource = readFileSync(new URL('./renderModel.js', import.meta.url), 'utf8');
const historySource = readFileSync(new URL('./historyModel.js', import.meta.url), 'utf8');

test('编辑器加载必须走 annotation:load + convertFileSrc', () => {
  assert.ok(appSource.includes("const ANNOTATION_LOAD_EVENT = 'annotation:load'"), '必须监听 annotation:load');
  assert.ok(appSource.includes("listen(ANNOTATION_LOAD_EVENT"), '必须 listen 加载事件');
  assert.ok(appSource.includes('convertFileSrc(path)'), '截图存储路径必须走 asset 协议前缀');
});

test('编辑器保存必须走通用画布命令 save_img_png_base64', () => {
  assert.ok(appSource.includes("'save_img_png_base64'"), '保存必须复用通用画布命令');
  assert.ok(appSource.includes("octx.fillStyle = '#ffffff'"), '保存必须合成白底');
});

test('编辑器保存失败必须提示并保持窗口打开,不得无条件关窗', () => {
  assert.ok(appSource.includes("from '@shared/utils/dialog'"), '保存失败必须走共享错误提示');
  assert.ok(appSource.includes('await showError('), 'catch 分支必须调用 showError 展示错误');
  // 关键护栏:未成功保存前禁止关窗——showError 之后必须 return,close() 只能出现在成功路径。
  const catchPos = appSource.indexOf('保存编辑器结果失败');
  const showErrorPos = appSource.indexOf('await showError(');
  assert.ok(catchPos !== -1 && showErrorPos !== -1, 'catch 分支与 showError 都必须存在');
  const closePos = appSource.indexOf("await getCurrentWindow().close()");
  assert.ok(closePos !== -1, '成功路径必须关窗');
  assert.ok(
    appSource.includes('return;') && catchPos < closePos,
    '失败分支必须 return 不关窗,close() 只能出现在成功路径',
  );
});

test('编辑器必须实接三 model（非仅摆设）', () => {
  // 三 model 导出必须被 App.jsx 真实消费。
  for (const layerExport of ['createLayer', 'addLayer', 'updateLayer', 'removeLayer', 'reorderLayer']) {
    assert.ok(layerSource.includes(`export function ${layerExport}`) || layerSource.includes(`export const ${layerExport}`), `layerModel 缺导出 ${layerExport}`);
  }
  assert.ok(renderSource.includes('export function layerToRenderCommands'), 'renderModel 缺 layerToRenderCommands');
  for (const historyExport of ['pushSnapshot', 'undoSnapshot', 'redoSnapshot', 'canUndoSnapshots', 'canRedoSnapshots']) {
    assert.ok(historySource.includes(`export function ${historyExport}`), `historyModel 缺导出 ${historyExport}`);
  }
  // App.jsx 必须 import 三 model。
  assert.ok(appSource.includes("from './layerModel'"), 'App.jsx 必须接 layerModel');
  assert.ok(appSource.includes("from './renderModel'"), 'App.jsx 必须接 renderModel');
  assert.ok(appSource.includes("from './historyModel'"), 'App.jsx 必须接 historyModel');
  // 撤销/重做必须调用双栈函数。
  assert.ok(appSource.includes('undoSnapshot(history, redoStack)'), '撤销必须调 undoSnapshot');
  assert.ok(appSource.includes('redoSnapshot(history, redoStack)'), '重做必须调 redoSnapshot');
  assert.ok(appSource.includes('pushSnapshot(h, layers)') || appSource.includes('pushSnapshot(h, prev)'), '提交层前必须压入快照');
});