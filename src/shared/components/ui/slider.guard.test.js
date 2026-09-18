import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(here, './Slider.jsx'), 'utf8');
// 剥行注释,避免注释字面误命中(§10.4 陷阱)
const strip = (src) =>
  src
    .split('\n')
    .filter((line) => !line.trimStart().startsWith('//'))
    .join('\n');

// 提取指定名称函数的函数体
function fnBody(code, fnName) {
  const start = code.indexOf(fnName);
  assert.ok(start > -1, `滑块源码缺少 ${fnName}`);
  let depth = 0;
  let i = start;
  for (; i < code.length; i += 1) {
    const ch = code[i];
    if (ch === '{') depth += 1;
    else if (ch === '}') {
      depth -= 1;
      if (depth === 0) break;
    }
  }
  assert.ok(i < code.length, `${fnName} 函数体未闭合`);
  return code.slice(start, i + 1);
}

test('Slider 键盘调整必须提交——设置滑杆吃不到键盘改变是跌落物', () => {
  const code = strip(source);
  assert.match(
    code,
    /onKeyUp=\{handleKeyUp\}/,
    'range 输入必须挂 onKeyUp 提交,否则键盘调整只改显示不改设置',
  );
  const keyupBody = fnBody(code, 'const handleKeyUp');
  // 键盘提交必须无条件执行 onChange(displayValue),不得依赖鼠标 isDragging 守卫
  assert.match(
    keyupBody,
    /onChange\(displayValue\)/,
    'handleKeyUp 必须提交 onChange(displayValue)',
  );
  assert.doesNotMatch(
    keyupBody,
    /isDragging/,
    'handleKeyUp 不得依赖鼠标拖动标志守卫',
  );
});