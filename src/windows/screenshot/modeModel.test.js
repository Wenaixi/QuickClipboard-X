import { test } from 'node:test';
import assert from 'node:assert/strict';
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

test('modeHint 拒绝非法输入', () => {
  assert.throws(() => modeHint('select', null), /翻译函数/);
});
