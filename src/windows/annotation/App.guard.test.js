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

test('编辑器画布必须兜底 pointercancel(与白板/截图同款),防残稿卡死', () => {
  // 系统打断(快捷键弹窗/UAC/任务切换/笔尖脱落)触发 pointercancel 替代
  // pointerup:草稿残留+系统自动释放指针捕获,下一笔被首指守卫吞掉,
  // 用户只能撤销/清空/关窗恢复。canvas 必须挂 onPointerCancel 走与
  // pointerup 相同的收口(白板 index.js/截图 App.jsx 均有兜底,唯编辑器缺)。
  assert.ok(
    appSource.includes('onPointerCancel={handlePointerUp}'),
    '画布必须监听 pointercancel 并复用 pointerup 收口(否则残稿卡死画不出新笔)',
  );
});

test('编辑器跨图加载必须先清进行中草稿,再清图层(防旧草稿渲染+吞首笔)', () => {
  // 窗口复用路径:编辑器开着再编辑另一张图,onload 触发重绘,若旧草稿
  // 残留会被 draw 渲染到新图(坐标错位)且首指守卫吞掉新图第一笔。
  // 顺序断言:清草稿必须早于 setLayers(newEmptyLayers())。
  const loadEventPos = appSource.indexOf("listen(ANNOTATION_LOAD_EVENT");
  assert.ok(loadEventPos !== -1, '必须监听加载事件');
  const onloadPos = appSource.indexOf('img.onload = () =>', loadEventPos);
  assert.ok(onloadPos !== -1, 'onload 回调必须存在');
  const onloadBody = appSource.slice(onloadPos, onloadPos + 400);
  const clearDraftPos = onloadBody.indexOf('draftRef.current = null;');
  const clearLayersPos = onloadBody.indexOf('setLayers(newEmptyLayers())');
  assert.ok(clearDraftPos !== -1, 'onload 必须先清进行中草稿');
  assert.ok(clearLayersPos !== -1, 'onload 必须清空图层(跨图不累积)');
  assert.ok(clearDraftPos < clearLayersPos, '清草稿必须早于清图层(避免 draw 渲染旧草稿)');
});

test('编辑器独立窗口必须 initSettings 同步语言,按钮/提示接语言包', () => {
  // D-1:编辑器是独立窗口,不经主窗口启动流程,若不先加载设置并切换语言,
  // 用户在设置切 en-US 后编辑工具按钮恒中文(与截图窗口不对称)。必须:
  // index.jsx initSettings().finally(render) 异步同步语言再渲染;
  // App.jsx 接 useTranslation,六个工具按钮与提示文案走 annotation 语言段。
  const indexSource = readFileSync(new URL('./index.jsx', import.meta.url), 'utf8');
  assert.ok(indexSource.includes("import { initSettings } from '@shared/store/settingsStore'"), 'index.jsx 必须引入 initSettings');
  const initPos = indexSource.indexOf('initSettings()');
  assert.ok(initPos !== -1, 'index.jsx 必须调用 initSettings');
  const renderPos = indexSource.indexOf('root.render(<App />)');
  assert.ok(renderPos !== -1, 'index.jsx 必须渲染 App');
  assert.ok(initPos < renderPos, 'initSettings 必须早于 render(先同步语言再渲染,防启动闪烁)');
  assert.ok(indexSource.includes('.finally('), 'render 必须挂在 finally(语言加载失败也要渲染,不白屏)');
  assert.ok(appSource.includes("useTranslation"), 'App.jsx 必须接 useTranslation');
  for (const key of ['undo', 'redo', 'clear', 'cancel', 'doneAndCopy', 'waitingLoad', 'textPrompt']) {
    assert.ok(appSource.includes(`annotation.${key}`), `App.jsx 必须接 annotation.${key} 语言键`);
  }
});

test('编辑器多指针只认首指,undo 先清草稿', () => {
  // 多指针守卫(白板同款):第二根手指/手掌误触落下不得覆盖首笔草稿,
  // move/up 只处理本笔指针;撤销重做前先丢弃进行中草稿,避免撤销后
  // 残留半截笔画悬浮不可撤销。
  assert.ok(
    appSource.includes('pointerId: event.pointerId'),
    '草稿必须携带指针 id',
  );
  assert.ok(
    appSource.includes('if (draftRef.current && event.pointerId !== draftRef.current.pointerId) return;'),
    '第二指落下必须忽略',
  );
  assert.ok(
    appSource.includes('if (event.pointerId !== draft.pointerId) return;'),
    '移动/抬起必须忽略非本笔指针',
  );
  const undoPos = appSource.indexOf('const handleUndo = () =>');
  assert.ok(undoPos !== -1, '撤销处理必须存在');
  const undoBody = appSource.slice(undoPos, appSource.indexOf('const handleRedo', undoPos));
  const clearPos = undoBody.indexOf('draftRef.current = null');
  const popPos = undoBody.indexOf('undoSnapshot(history, redoStack)');
  assert.ok(clearPos !== -1 && popPos !== -1, '撤销必须先清草稿再弹栈');
  assert.ok(clearPos < popPos, '清草稿必须早于弹栈');

  // 清空/删除图层同款:这两个操作会触发 setLayers 重跑 draw,不清进行中
  // 草稿会让半截笔画悬浮在清空后的画布上(不可撤销不可保存,松手后还会
  // 补成正式图层)。顺序断言锁死"清草稿必须先于压栈快照"。
  const clearFnPos = appSource.indexOf('const handleClear = () =>');
  assert.ok(clearFnPos !== -1, '清空处理必须存在');
  const clearFnBody = appSource.slice(clearFnPos, appSource.indexOf('const handleRemoveSelected', clearFnPos));
  const clearDraftPos = clearFnBody.indexOf('draftRef.current = null');
  const clearPushPos = clearFnBody.indexOf('pushSnapshot(h, layers)');
  assert.ok(clearDraftPos !== -1 && clearPushPos !== -1, '清空必须先清草稿再压栈快照');
  assert.ok(clearDraftPos < clearPushPos, '清空时清草稿必须早于压栈');

  const removeFnPos = appSource.indexOf('const handleRemoveSelected = (id) =>');
  assert.ok(removeFnPos !== -1, '删除图层处理必须存在');
  const removeFnBody = appSource.slice(removeFnPos, appSource.indexOf('const handleSave', removeFnPos));
  const removeDraftPos = removeFnBody.indexOf('draftRef.current = null');
  const removePushPos = removeFnBody.indexOf('pushSnapshot(h, layers)');
  assert.ok(removeDraftPos !== -1 && removePushPos !== -1, '删除图层必须先清草稿再压栈快照');
  assert.ok(removeDraftPos < removePushPos, '删除图层时清草稿必须早于压栈');
});