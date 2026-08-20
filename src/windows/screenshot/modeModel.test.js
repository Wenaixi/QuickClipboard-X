import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { modeForState, modeHint } from './modeModel.js';

test('modeForState 调整大小优先于移动优先于框选', () => {
  assert.equal(modeForState({ selecting: true, moving: false, resizing: true }), 'resize');
  assert.equal(modeForState({ selecting: false, moving: true, resizing: false }), 'move');
  assert.equal(modeForState({ selecting: true, moving: true, resizing: false }), 'move');
  assert.equal(modeForState({ selecting: true, moving: false, resizing: false }), 'select');
});

test('modeForState 无任何交互时返回 null', () => {
  assert.equal(modeForState({ selecting: false, moving: false, resizing: false }), null);
  assert.equal(modeForState({}), null);
});

test('modeForState 源码优先级顺序锁定为调整大于移动大于框选且缺省字段按 falsy', () => {
  const source = readFileSync(new URL('./modeModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('export function modeForState');
  const body = source.slice(start, start + 400);
  // 源码护栏一：三个分支必须依次出现（resizing 先于 moving 先于 selecting），
  // 顺序颠倒会改变 UI 提示优先级——调整中又移动时必须显示调整提示。
  const resizeIdx = body.indexOf("if (state.resizing) return 'resize';");
  const moveIdx = body.indexOf("if (state.moving) return 'move';");
  const selectIdx = body.indexOf("if (state.selecting) return 'select';");
  assert.ok(resizeIdx !== -1 && moveIdx !== -1 && selectIdx !== -1, '三个分支必须全部存在');
  assert.ok(resizeIdx < moveIdx && moveIdx < selectIdx, '分支顺序必须为 resizing → moving → selecting');
  // 源码护栏二：优先级判定必须直接用字段真值（缺省 undefined 按 falsy 返回下一级）。
  assert.ok(body.includes("if (state.resizing) return 'resize';"), 'resizing 必须直接判定');
  assert.ok(!body.includes('state.resizing === true'), '禁止严格等于 true（undefined 应按 falsy 处理）');
  // 行为佐证：调整+框选并存显示调整；移动+框选并存显示移动；仅框选显示框选。
  assert.equal(modeForState({ resizing: true, moving: true, selecting: true }), 'resize');
  assert.equal(modeForState({ moving: true, selecting: true }), 'move');
  assert.equal(modeForState({ selecting: true }), 'select');
  assert.equal(modeForState({}), null);
});

test('modeForState 拒绝非法输入', () => {
  assert.throws(() => modeForState(null), /状态对象/);
  assert.throws(() => modeForState('x'), /状态对象/);
});

test('modeHint 各模式返回对应提示文案', () => {
  const t = (key) => ({ 'screenshot.mode.select': '拖动以框选截图区域', 'screenshot.mode.move': '拖动以移动选区', 'screenshot.mode.resize': '拖动以调整选区大小' }[key]);
  assert.equal(modeHint('select', t), '拖动以框选截图区域');
  assert.equal(modeHint('move', t), '拖动以移动选区');
  assert.equal(modeHint('resize', t), '拖动以调整选区大小');
});

test('modeHint 空闲模式或未知模式返回 null', () => {
  const t = (key) => key;
  assert.equal(modeHint(null, t), null);
  assert.equal(modeHint('unknown', t), null);
});


test('MODE_KEYS 源码必须覆盖 modeForState 全部可返回模式且 key 名跳过模块名', () => {
  const source = readFileSync(new URL('./modeModel.js', import.meta.url), 'utf8');
  const start = source.indexOf('const MODE_KEYS = {');
  const body = source.slice(start, start + 300);
  // 源码护栏：三个模式的翻译 key 必须全部存在（缺一任一 modeForState 返回的模式在 UI 上就显示空提示）。
  assert.ok(body.includes("select: 'screenshot.mode.select'"), '必须有 select key');
  assert.ok(body.includes("move: 'screenshot.mode.move'"), '必须有 move key');
  assert.ok(body.includes("resize: 'screenshot.mode.resize'"), '必须有 resize key');
  // 运行时完整性：每个模式的 key 都能通过翻译函数获得文案。
  const calls = [];
  const t = (key) => { calls.push(key); return 'ok'; };
  for (const mode of ['select', 'move', 'resize']) {
    assert.equal(modeHint(mode, t), 'ok', `${mode} 必须返回翻译文案`);
  }
  assert.deepEqual(calls, ['screenshot.mode.select', 'screenshot.mode.move', 'screenshot.mode.resize'], '三个模式必须传入各自的翻译 key');
  // 反向验证：非权威 key 不在表中（防止拼写错后无文案也不报错）。
  assert.equal(modeHint('selecting', t), null, '非模式名必须返回 null');
});
test('modeHint 拒绝非法输入', () => {
  assert.throws(() => modeHint('select', null), /翻译函数/);
});
