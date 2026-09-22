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
  // clear 按钮绑定在 start() 函数体内(document.getElementById('clear') 在
  // setupToolButtons 也出现一次,裸 indexOf 会命中靠前的变量赋值,300 字符
  // 切片切不到真正的 click 监听器)——从 start 起点切到文件尾的
  // initSettings 调用,确保覆盖 start 函数体内的绑定段。
  const startFn = bare.indexOf('function start');
  assert.ok(startFn >= 0, '缺少 start');
  const initPos = bare.indexOf('initSettings(');
  assert.ok(initPos > startFn, '缺少 initSettings 启动段');
  const startSeg = bare.slice(startFn, initPos);
  assert.ok(
    startSeg.includes('shapes.length = 0'),
    '清空必须重置形状栈（只 clearRect 会留下无法撤销/保存的形状）',
  );
  assert.ok(
    startSeg.includes('renderPreview()'),
    '清空必须走全量重绘入口（保证画布与栈一致）',
  );
  // 绘制中清空必须先收口当前一笔:否则 drawing 残留让清空后的画布仍画
  // 半笔,松手 endDraw 又把半笔补回已清空的栈(松手诈尸)。与 undo 同构。
  assert.ok(
    startSeg.includes('endDraw({ pointerId: activePointerId })'),
    '清空必须先收口进行中一笔(与撤销同构,防松手诈尸)',
  );
});

test('白板支持撤销(弹出栈顶并重绘)', () => {
  const undo = bodyOf('function undo', 'function start');
  assert.ok(
    undo.includes('shapes.pop()') && undo.includes('renderPreview()'),
    '撤销必须弹栈并重绘（有栈才有撤销语义）',
  );
});

test('白板绘制中断必须兜底 endDraw(防 drawing 卡死)', () => {  const start = bare.indexOf('function startDraw');
  assert.ok(start >= 0, '缺 startDraw');
  assert.ok(
    bare.includes('setPointerCapture('),
    'pointerdown 必须 setPointerCapture（否则滑到工具栏释放会丢 pointerup）',
  );
  assert.ok(
    bare.includes('pointercancel'),
    '必须监听 pointercancel（绘制中断兜底入口）',
  );
  // cancelDraw 在 start() 之后(函数声明靠后),bodyOf 用 save 作终点
  const cancel = bodyOf('function cancelDraw', 'async function save');
  assert.ok(
    cancel.includes('endDraw(event)'),
    'cancelDraw 必须透传事件给 endDraw(否则第二指中断会误收首指当前笔)',
  );
});

test('白板多指针触控只认第一根指针(防双指串线/误收笔)', () => {
  const start = bare.indexOf('function startDraw');
  assert.ok(start >= 0, '缺 startDraw');
  assert.ok(
    bare.includes('let activePointerId = null;'),
    '必须声明当前笔指针 id 状态(否则无法过滤第二指)',
  );
  assert.ok(
    bare.includes('activePointerId = event.pointerId ?? null;'),
    'startDraw 必须记录首指 pointerId',
  );
  const move = bodyOf('function moveDraw', 'function endDraw');
  assert.ok(
    move.includes('event.pointerId !== activePointerId'),
    'moveDraw 必须忽略非当前笔指针的移动(否则第二指混入 path/end)',
  );
  const end = bodyOf('function endDraw', 'function cancelDraw');
  assert.ok(
    end.includes('event.pointerId !== activePointerId'),
    'endDraw 必须忽略非当前笔指针的抬起(否则第二指松手即结束这一笔)',
  );
});

test('白板非笔工具最小尺寸判定(防 pointerleave 收口留微距噪点)', () => {
  const end = bodyOf('function endDraw', 'function cancelDraw');
  assert.ok(
    end.includes('Math.abs(end.x - anchor.x) + Math.abs(end.y - anchor.y) >= 3'),
    'endDraw 必须对非笔工具做最小尺寸判定(两点曼哈顿距离 < 3px 视为误触空笔丢弃)',
  );
});

// 保存输出尺寸必须直读画布物理尺寸而非 innerWidth*dpr:白板窗口禁缩放全屏,
// DPR 变化未触发 onResized 时 innerWidth*dpr 与 canvas.width 会不一致,
// 输出按旧值会拉伸/留边,读画布与 resizeCanvas 维护的物理尺寸天然同步。
test('白板保存必须读画布物理尺寸(与 DPR 解耦)', () => {
  const save = bodyOf('async function save', 'function undo');
  assert.ok(
    save.includes('out.width = canvas.width'),
    '保存输出宽度必须直读画布物理尺寸(resizeCanvas 维护)',
  );
  assert.ok(
    save.includes('out.height = canvas.height'),
    '保存输出高度必须直读画布物理尺寸',
  );
  assert.ok(
    !save.includes('window.innerWidth * dpr'),
    '输出尺寸不得再用 innerWidth*dpr(与画布物理尺寸解耦)',
  );
});

// 白板 Esc 与工具按钮语言护栏：Esc 在有未保存形状/绘制中时必须先经
// showConfirm 确认（shapes.length>0 || drawing 判定），否则误按 Esc 直接
// 丢全部成果；工具按钮 title/撤销/清空/保存文本不得再是静态中文（走
// i18n.t），保存失败文案走 whiteboard.saveFailed 键。
test('白板 Esc 有未保存内容必须先确认再关窗', () => {
  const start = bare.indexOf('document.addEventListener(\'keydown\'');
  const body = bare.slice(start, start + 900);
  assert.ok(
    body.includes('shapes.length > 0 || drawing'),
    'Esc 分支必须判定有未保存形状或正在绘制',
  );
  assert.ok(
    body.includes('showConfirm('),
    '有未保存内容时 Esc 必须走 showConfirm 确认',
  );
  assert.ok(
    body.includes('window.close().catch(() => {});'),
    '确认后必须关窗',
  );
});

test('白板工具按钮文本走语言包(i18n.t)不再静态中文', () => {
  const setup = bodyOf('function setupToolButtons', 'function startDraw');
  assert.ok(
    setup.includes('btn.title = i18n.t(`whiteboard.tool.${id}`)'),
    '工具按钮 title 必须走语言包',
  );
  assert.ok(
    setup.includes('i18n.t(`whiteboard.color.'),
    '颜色按钮 title 必须走语言包',
  );
  assert.ok(
    setup.includes("i18n.t('whiteboard.action.undo')")
      && setup.includes("i18n.t('whiteboard.action.clear')")
      && setup.includes("i18n.t('whiteboard.action.save')"),
    '撤销/清空/保存按钮文本必须走语言包',
  );
  const save = bodyOf('async function save', 'function undo');
  assert.ok(
    save.includes('i18n.t(\'whiteboard.saveFailed\'')
      || save.includes('i18n.t(`whiteboard.saveFailed`'),
    '保存失败文案必须走语言包',
  );
  assert.ok(
    !save.includes('保存失败,请重试'),
    '保存失败不得再是裸中文字面',
  );
});

// 白板启动竞态护栏(R126 新发现):initSettings 的跨进程 IPC(loadSettings
// 内 invoke)在模块脚本执行后才 resolve,DOMContentLoaded 可能在 resolve
// 前就已派发——若只靠 addEventListener 注册 start,DOM 就绪在先时会错过
// 事件,start 永不执行,白板死屏(画布不可画/按钮无响应)。必须用
// readyState 守卫:loading 时挂监听、否则直接调 start,两条路径必须都在。
test('白板 initSettings 完成后必须 readyState 守卫启动(防 DOMContentLoaded 错过死屏)', () => {
  assert.ok(
    bare.includes("document.readyState === 'loading'"),
    '必须用 readyState 判定文档是否仍在加载'
  );
  assert.ok(
    bare.includes("window.addEventListener('DOMContentLoaded', start)"),
    'loading 态必须挂 DOMContentLoaded 监听等 start'
  );
  assert.ok(
    bare.includes('} else {') && bare.includes('start();'),
    '文档已就绪时必须直接调 start(否则 IPC 后错过事件死屏)'
  );
  // 监听必须在 initSettings 的 finally 内注册,保证 start 只跑一次
  assert.ok(
    bare.indexOf('document.readyState') > bare.indexOf('initSettings()'),
    'readyState 守卫必须在 initSettings 之后(语言同步后才启动)'
  );
});
