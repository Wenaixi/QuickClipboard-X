// 白板保存失败护栏:白板 index.js 的 save() catch 分支若仅 console.error 后
// 无条件 getCurrentWindow().close(),落盘/剪贴板/历史写库任一步失败时用户
// 看到窗口消失却误以为保存成功,绘制成果静默丢失。必须失败时提示并保持
// 窗口打开,仅成功才关窗。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./index.js', import.meta.url), 'utf8');
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');
const code = strip(source);

test('白板保存失败必须提示并保持窗口打开,不得无条件关窗', () => {
  assert.ok(code.includes("from '@shared/utils/dialog'"), '必须引入共享错误提示');
  assert.ok(code.includes('await showError('), 'catch 分支必须调用 showError 展示错误');
  const catchPos = code.indexOf('保存白板失败');
  const showErrorPos = code.indexOf('await showError(');
  assert.ok(catchPos !== -1 && showErrorPos !== -1, 'catch 分支与 showError 都必须存在');
  const closePos = code.indexOf('getCurrentWindow().close()');
  assert.ok(closePos !== -1, '成功路径必须关窗');
  assert.ok(
    code.includes('return;') && catchPos < closePos,
    '失败分支必须 return 不关窗,close() 只能出现在成功路径',
  );
});