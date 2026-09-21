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