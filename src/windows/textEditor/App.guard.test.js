// 编辑器保存失败提示护栏：textEditor 保存走 updateClipboardItem/
// updateFavorite/addFavorite，任一 Promise 拒绝必须 toast.error 提示
// 用户并保持窗口打开——静默吞错会让用户误以为保存成功,改动未落库
// 也不知情(ToastContainer 已挂载,直接复用)。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./App.jsx', import.meta.url), 'utf8');

test('textEditor 保存失败必须 toast 提示用户', () => {
  assert.ok(
    source.includes("from '@shared/store/toastStore'"),
    '必须接入共享 toastStore',
  );
  assert.ok(
    source.includes('toast.error('),
    '保存 catch 分支必须调用 toast.error',
  );
});

test('textEditor 保存失败不得关窗(改动不得静默丢失)', () => {
  // 取 handleSave 函数体:close() 必须位于 catch 之前(try 成功路径),
  // toast.error 必须位于 catch 内(catch 之后)。
  const saveStart = source.indexOf('const handleSave');
  const saveBody = source.slice(saveStart, source.indexOf('const handleCancel'));
  const tryPos = saveBody.indexOf('try');
  const catchPos = saveBody.indexOf('console.error(\'保存失败:');
  const toastPos = saveBody.indexOf('toast.error(');
  const closePos = saveBody.indexOf('await currentWindow.close()');
  assert.ok(tryPos !== -1 && catchPos !== -1, 'handleSave 必须有 try/catch 结构');
  assert.ok(toastPos !== -1, 'catch 分支必须调用 toast.error');
  assert.ok(
    toastPos > catchPos,
    'toast.error 必须位于 catch 分支内(catch 之后)',
  );
  assert.ok(
    closePos !== -1 && closePos < catchPos,
    'close() 必须只在 try 成功路径(早于 catch),失败路径不得关窗',
  );
});

test('textEditor 未保存改动关窗必须经确认拦截', () => {
  // 未保存改动误触 X/取消/Alt+F4 不得静默丢失:onCloseRequested 注册 +
  // hasChangesRef 读最新值(闭包会读到旧 false)+ showConfirm 确认 +
  // 未确认 preventDefault。hasChangesRef 必须在 onCloseRequested 之前
  // 声明(useRef 先于使用);showConfirm 必须位于 onCloseRequested 回调内。
  const refPos = source.indexOf('const hasChangesRef = useRef(hasChanges);');
  const closePos = source.indexOf('onCloseRequested(async (event) => {');
  assert.ok(refPos !== -1 && closePos !== -1 && refPos < closePos, 'hasChangesRef 必须先于 onCloseRequested 声明');
  const closeBody = source.slice(closePos, closePos + 700);
  assert.ok(closeBody.includes('hasChangesRef.current'), '回调必须读 ref 最新值(非闭包捕获)');
  assert.ok(closeBody.includes('showConfirm('), '回调必须调用 showConfirm 确认');
  assert.ok(closeBody.includes('event.preventDefault()'), '未确认必须 preventDefault 拦截关窗');
});

test('textEditor 保存失败 toast 必须走语言包(禁止裸中文硬编码)', () => {
  // E-候选2:toast.error 原为裸中文字面,en 用户保存失败收到中文提示。
  // 必须接 textEditor.saveFailed 语言键(带 error 插值),防硬编码复活。
  const saveStart = source.indexOf('const handleSave');
  const saveBody = source.slice(saveStart, source.indexOf('const handleCancel'));
  assert.ok(
    saveBody.includes("t('textEditor.saveFailed'"),
    '保存失败 toast 必须走 textEditor.saveFailed 语言键'
  );
  assert.ok(
    saveBody.includes("error: String(error?.message || error)"),
    'saveFailed 键必须携带 error 插值'
  );
  const bare = source
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');
  assert.ok(
    bare.includes('toast.error(\'保存失败:') === false,
    '禁止裸中文保存失败 toast(剥注释判定,防复活)'
  );
});

test('textEditor 保存成功必须同步复位 hasChangesRef(防二次确认框)', () => {
  // E-候选11:setHasChanges(false) 是异步 state,ref 更新依赖 effect 时序;
  // close() 经 onCloseRequested 读 ref,动态 import 先 resolve 时旧 true
  // 会二次弹确认框拦死正常关闭。必须 setHasChanges(false) 紧邻同步
  // hasChangesRef.current = false。
  const saveStart = source.indexOf('const handleSave');
  const saveBody = source.slice(saveStart, source.indexOf('const handleCancel'));
  const setPos = saveBody.indexOf('setHasChanges(false)');
  const refPos = saveBody.indexOf('hasChangesRef.current = false');
  assert.ok(setPos !== -1, '保存成功必须 setHasChanges(false)');
  assert.ok(refPos !== -1, '保存成功必须同步复位 hasChangesRef');
  assert.ok(refPos > setPos, 'ref 同步复位必须在 setHasChanges 之后(先标记 state 再复位 ref)');
  const closePos = saveBody.indexOf('await currentWindow.close()');
  assert.ok(refPos < closePos, 'ref 同步复位必须早于 close()(关窗前 ref 已是 false)');
});

test('textEditor 必须支持 Esc 关闭(与白板/取色器/标尺交互一致)', () => {
  // E-候选3:编辑器是唯一有未保存内容的工具窗,无 Esc 语义;白板/取色器/
  // 标尺均 Esc 关闭。Esc 走 close() 自动获得 onCloseRequested 未保存确认。
  assert.ok(
    source.includes("event.key === 'Escape'"),
    '必须监听 Escape 按键'
  );
  assert.ok(
    source.includes('document.addEventListener(\'keydown\', onKeyDown)'),
    'Escape 处理必须注册在 document keydown'
  );
  assert.ok(
    source.includes('void handleCancel()'),
    'Esc 必须调用 handleCancel(走 close(),自动获得未保存确认保护)'
  );
});

test('textEditor 编辑收藏必须校验原分组存在,失效回退「全部」', () => {
  // E-候选4:删除分组不级联清收藏 group_name,编辑收藏时原分组已删,
  // 继续用失效分组名保存必失败且提示无关。必须:加载收藏数据时校验
  // item.group_name 是否仍在 groupsSnap.groups,不在回退「全部」;
  // 编辑保存 updateFavorite 必须与新建同款 title.trim()。
  const bare = source
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');
  assert.ok(
    bare.includes('groupsSnap.groups.some((group) => group.name === item.group_name)'),
    '必须校验原分组是否仍存在'
  );
  assert.ok(
    bare.includes("setSelectedGroup(groupStillExists ? item.group_name : '全部')"),
    '原分组失效必须回退「全部」'
  );
  const saveStart = source.indexOf('const handleSave');
  const saveBody = source.slice(saveStart, source.indexOf('const handleCancel'));
  assert.ok(
    saveBody.includes('updateFavorite(editorData.id, title.trim()'),
    '编辑收藏保存必须与新建同款 title.trim()'
  );
});