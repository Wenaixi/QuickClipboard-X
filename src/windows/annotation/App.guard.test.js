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

test('编辑器保存逆映射平移项必须为负(居中偏移漂移符号)', () => {
  // E-1:显示→图像逆映射 图像坐标=(显示坐标-居中偏移)/scale,
  // setTransform 平移项必须带负号;若写成正号,标注会整体向右下偏
  // 移 offset/scale 像素(缩放显示缩放越大越严重)。负号被改成正号即
  // 见红。
  const transformLine = appSource
    .split('\n')
    .find((line) => line.includes('setTransform(1 / scale'));
  assert.ok(transformLine, '必须存在逆映射 setTransform 行');
  assert.ok(transformLine.includes('-offsetX'), 'X 平移项必须带负号');
  assert.ok(transformLine.includes('-offsetY'), 'Y 平移项必须带负号');
});

test('编辑器保存导出画布必须为原图像素尺寸(不得乘 dpr 超采样放大)', () => {
  // D-新1:r3 起 out.width = naturalWidth * dpr 把显示 DPI 烤进输出,
  // 高分屏标注产物被放大 dpr² 倍(体积 2-4 倍+重采样损质)。护栏锁死
  // 导出尺寸必须与原图一致,防 dpr 乘法复活。
  assert.ok(
    appSource.includes('out.width = image.naturalWidth;'),
    '导出画布宽必须等于原图宽(乘 dpr 会让产物放大)',
  );
  assert.ok(
    appSource.includes('out.height = image.naturalHeight;'),
    '导出画布高必须等于原图高',
  );
  const exportSeg = appSource.slice(
    appSource.indexOf('out.width = image.naturalWidth;'),
    appSource.indexOf('const base64 = out.toDataURL'),
  );
  assert.ok(
    !exportSeg.includes('naturalWidth * dpr') && !exportSeg.includes('naturalHeight * dpr'),
    '导出尺寸不得出现 ×dpr(超采样放大复活)',
  );
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

test('编辑器绘制必须捕获指针并随窗口尺寸重绘', () => {
  // 指针捕获:绘制中把笔划拖出画布边界(工具栏/窗口外)释放时仍能收到
  // 抬起事件收口,否则提交半截笔画(与白板同款保护)。
  assert.ok(appSource.includes('canvasRef.current.setPointerCapture(event.pointerId)'), '画布必须捕获指针');
  // 窗口尺寸/跨 DPI 变化必须重跑 draw,否则位图被 CSS 拉伸显示模糊。
  assert.ok(appSource.includes('.onResized(redraw)'), '窗口尺寸变化必须重绘');
  assert.ok(appSource.includes("matchMedia('(resolution: ' + window.devicePixelRatio + 'dppx)')"), '跨 DPI 变化必须重绘');
});