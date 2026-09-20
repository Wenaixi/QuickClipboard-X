// 白板形状持久化护栏：shapes 数组持久层必须在渲染路径中被消费。
// 历史上白板只有单幅 canvas 预览缓冲，renderPreview 每帧清屏重建当前
// 一笔，endDraw 不把完成的形状落盘，画第二个形状即丢第一个（保存产物
// 与所见不一致）。护栏锁定三个不变量：
//   1) 存在形状栈数组（shapes 声明）；
//   2) endDraw 必须把完成形状推入栈（push 在 endDraw 函数体内）；
//   3) renderPreview 必须遍历栈全量重绘（栈遍历在 renderPreview 函数体内）。

import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const source = readFileSync(new URL('./index.js', import.meta.url), 'utf8');

// 去掉行注释，防止注释字面误命中被测代码模式。
const bare = source
  .split('\n')
  .filter((line) => !line.trimStart().startsWith('//'))
  .join('\n');

function bodyOf(startMarker, endMarker) {
  const start = bare.indexOf(startMarker);
  assert.ok(start >= 0, `缺少 ${startMarker}`);
  const tail = bare.slice(start);
  const end = tail.indexOf(endMarker);
  assert.ok(end >= 0, `缺少 ${endMarker}`);
  return tail.slice(0, end);
}

test('白板必须有形状栈数组', () => {
  assert.ok(
    bare.includes('const shapes = [];'),
    '必须声明 shapes 形状栈（不存在则无持久化层）',
  );
});

test('白板 endDraw 必须把完成形状推入栈', () => {
  const endDraw = bodyOf('function endDraw', 'function save');
  assert.ok(
    endDraw.includes('shapes.push('),
    'endDraw 必须把完成的形状推入 shapes 栈（不推则画完即丢）',
  );
  assert.ok(
    endDraw.includes('[...path]'),
    '钢笔路径必须快照入栈（直接引用会被后续绘制清空）',
  );
});

test('白板 renderPreview 必须全量重绘形状栈', () => {
  const render = bodyOf('function renderPreview', 'function setupToolButtons');
  assert.ok(
    render.includes('for (const shape of shapes)'),
    'renderPreview 必须遍历 shapes 全量重绘（不清屏后只画当前一笔）',
  );
  assert.ok(
    render.includes('drawShape('),
    'renderPreview 必须经 drawShape 逐笔绘制（预览与成稿共用几何）',
  );
});

test('白板清空必须清栈并走全量重绘', () => {
  const start = bare.indexOf('document.getElementById(\'clear\')');
  assert.ok(start >= 0, '缺少 clear 按钮绑定');
  const clearSeg = bare.slice(start, start + 220);
  assert.ok(
    clearSeg.includes('shapes.length = 0'),
    '清空必须重置形状栈（只 clearRect 会留下无法撤销/保存的形状）',
  );
  assert.ok(
    clearSeg.includes('renderPreview()'),
    '清空必须走全量重绘入口（保证画布与栈一致）',
  );
});

test('白板支持撤销(弹出栈顶并重绘)', () => {
  const undo = bodyOf('function undo', 'function start');
  assert.ok(
    undo.includes('shapes.pop()') && undo.includes('renderPreview()'),
    '撤销必须弹栈并重绘（有栈才有撤销语义）',
  );
});

test('白板绘制中断必须兜底 endDraw(防 drawing 卡死)', () => {
  const start = bare.indexOf('function startDraw');
  assert.ok(start >= 0, '缺 startDraw');
  assert.ok(
    bare.includes('setPointerCapture('),
    'pointerdown 必须 setPointerCapture（否则滑到工具栏释放会丢 pointerup）',
  );
  assert.ok(
    bare.includes('pointercancel'),
    '必须监听 pointercancel（绘制中断兜底入口）',
  );
  const cancel = bodyOf('function cancelDraw', 'function start');
  assert.ok(
    cancel.includes('endDraw()'),
    'cancelDraw 必须委托 endDraw 收口（重置 drawing 防永久卡死）',
  );
});
