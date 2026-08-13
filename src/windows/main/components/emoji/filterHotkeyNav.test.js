import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readSource } from './readSource.js';

// F1-2: G3 修复 no-op。handleFilterLeft/Right 先 resetKbNav() 再 setEmojiMode,
// 但 EmojiTab 的 [emojiMode] effect 无条件重置 kbZone→outside,最终状态与
// 不调 resetKbNav 完全一致,grid 高亮仍被清除,且注释描述的行为不存在。
// 修复:App 保存切换前 zone,setEmojiMode 后经 restoreKbNav 挂起恢复意图,
// emojiMode effect 按挂起意图在新数据上恢复 grid/search,而非一律重置。

test('F1-2 App handleFilterLeft/Right emoji 分支保存并恢复 zone,不再 resetKbNav', async () => {
  const body = await readSource('../../App.jsx');
  for (const fnName of ['handleFilterLeft', 'handleFilterRight']) {
    const fnStart = body.indexOf(`const ${fnName}`);
    const fnEnd = body.indexOf('const handleToggleSearch', fnStart);
    assert.notEqual(fnStart, -1, `缺 const ${fnName}`);
    const fn = body.slice(fnStart, fnEnd);
    assert.ok(fn.includes('getKbZone'), `${fnName} 应读切换前 zone(getKbZone)`);
    assert.ok(fn.includes('restoreKbNav'), `${fnName} 应恢复 zone(restoreKbNav)`);
    assert.equal(fn.includes('resetKbNav'), false, `${fnName} 不应再调 resetKbNav(no-op)`);
  }
});

test('F1-2 EmojiTab 暴露 restoreKbNav 且 effect 按挂起意图恢复', async () => {
  const body = await readSource('../EmojiTab.jsx');
  // useImperativeHandle 暴露 restoreKbNav
  const impStart = body.indexOf('useImperativeHandle(ref');
  const impEnd = body.indexOf('}), [executeCurrentItem', impStart);
  const imp = body.slice(impStart, impEnd);
  assert.ok(imp.includes('restoreKbNav'), 'useImperativeHandle 应暴露 restoreKbNav');
  // emojiMode effect 内存在挂起意图分支(pendingRestoreZone),grid 恢复走重新激活而非一律 outside
  const effectStart = body.indexOf('const pendingZone = pendingRestoreZoneRef.current');
  const effectEnd = body.indexOf('}, [emojiMode]);', effectStart);
  assert.notEqual(effectStart, -1, 'effect 应读挂起恢复意图 ref(pendingRestoreZoneRef)');
  const effect = body.slice(effectStart, effectEnd);
  assert.ok(effect.includes('tryActivateGrid'), 'grid 恢复应重新走统一网格激活 helper');
});
